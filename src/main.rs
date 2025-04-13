//! Runs TETRIS on display from sk6812 RGBW LED strip and buttons using interrupts.
//!
//! The following wiring is assumed:
//! - LED => GPIO8
//! - RIGHT_BUTTON => GPIO0 -> GND
//! - MIDDLE_BUTTON => GPIO1 -> GND
//! - LEFT_BUTTON => GPIO2 -> GND
//! - SPI => GPIO4, GPIO6, GPIO7
//!
//! Use Monitor to see on the output why is button debouncing important.

#![no_std]
#![no_main]

use core::cell::RefCell;
use core::fmt;
use critical_section::Mutex;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embedded_hal_async::spi::SpiBus;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    dma::{DmaRxBuf, DmaTxBuf},
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    handler, main,
    rmt::{PulseCode, Rmt, TxChannelAsync, TxChannelConfig, TxChannelCreatorAsync},
    rng::Rng,
    spi::{
        master::{Config, Spi, SpiDmaBus},
        Mode,
    },
    time::{self, Rate},
    Async,
};
use esp_println::println;

// global config
const BOARD_WIDTH: usize = 8;
const BOARD_HEIGHT: usize = 32;
const FALL_INTERVAL: u64 = 500; // TODO: should be function of score -> higher score faster speed
const BRIGHTNESS: u8 = 6;
const T0H: u16 = 40;
const T0L: u16 = 85;
const T1H: u16 = 80;
const T1L: u16 = 45;

struct Max7219 {
    spi: SpiDmaBus<'static, Async>,
}

impl Max7219 {
    fn new(spi: SpiDmaBus<'static, Async>) -> Self {
        Max7219 { spi }
    }

    async fn init(&mut self) {
        // Initialize four MAX7219 modules
        for addr in 1..=4 {
            self.write_reg(addr, 0x09, 0x00).await; // No decode
            self.write_reg(addr, 0x0A, 0x01).await; // Low intensity
            self.write_reg(addr, 0x0B, 0x07).await; // Scan all 8 digits
            self.write_reg(addr, 0x0C, 0x01).await; // Normal operation
            self.write_reg(addr, 0x0F, 0x00).await; // Display test off
        }
    }

    async fn write_reg(&mut self, addr: u8, reg: u8, data: u8) {
        // Send 8 bytes (4 modules * 2 bytes), with NOP (0x00, 0x00) for others
        let mut buffer = [0u8; 8];
        let idx = (4 - addr) as usize * 2;
        buffer[idx] = reg;
        buffer[idx + 1] = data;
        self.spi.write(&buffer).expect("SPI write failed");
    }

    async fn set_row(&mut self, addr: u8, row: u8, value: u8) {
        self.write_reg(addr, row + 1, value).await;
    }

    async fn clear(&mut self) {
        for addr in 1..=4 {
            for row in 0..8 {
                self.set_row(addr, row, 0x00).await;
            }
        }
    }
}

fn create_led_bits(r: u8, g: u8, b: u8, w: u8) -> [u32; 33] {
    let mut data = [PulseCode::empty(); 33];
    let bytes = [g, r, b, w];

    let mut idx = 0;
    for byte in bytes {
        for bit in (0..8).rev() {
            data[idx] = if (byte & (1 << bit)) != 0 {
                PulseCode::new(Level::High, T1H, Level::Low, T1L)
            } else {
                PulseCode::new(Level::High, T0H, Level::Low, T0L)
            };
            idx += 1;
        }
    }
    data[32] = PulseCode::new(Level::Low, 800, Level::Low, 0);
    data
}

fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Red => (BRIGHTNESS, 0, 0),
        Color::Green => (0, BRIGHTNESS, 0),
        Color::Blue => (0, 0, BRIGHTNESS),
        Color::Yellow => (BRIGHTNESS / 2, BRIGHTNESS / 2, 0),
        Color::Cyan => (0, BRIGHTNESS / 2, BRIGHTNESS / 2),
        Color::Magenta => (BRIGHTNESS / 2, 0, BRIGHTNESS / 2),
        Color::White => (BRIGHTNESS / 3, BRIGHTNESS / 3, BRIGHTNESS / 3),
        _ => (0, 0, 0),
    }
}

// Tetris Library ( from https://github.com/Hahihula/no_std_tetris )

// Define colors
#[derive(Clone, Copy)]
pub enum Color {
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    White,
}

// Tetromino struct
#[derive(Clone, Copy)]
pub struct Tetromino {
    shape: [(u8, u8); 4],
    color: Color,
}

// Tetromino shapes with their rotations
const TETROMINOS: &[Tetromino; 7] = &[
    Tetromino {
        shape: [(0, 1), (1, 1), (2, 1), (3, 1)],
        color: Color::Cyan,
    }, // I
    Tetromino {
        shape: [(0, 0), (0, 1), (1, 0), (1, 1)],
        color: Color::Yellow,
    }, // O
    Tetromino {
        shape: [(0, 1), (1, 1), (2, 1), (1, 0)],
        color: Color::Magenta,
    }, // T
    Tetromino {
        shape: [(0, 0), (1, 0), (2, 0), (2, 1)],
        color: Color::Green,
    }, // L
    Tetromino {
        shape: [(0, 1), (1, 1), (2, 1), (2, 0)],
        color: Color::Red,
    }, // J
    Tetromino {
        shape: [(0, 0), (1, 0), (1, 1), (2, 1)],
        color: Color::Blue,
    }, // S
    Tetromino {
        shape: [(0, 1), (1, 1), (1, 0), (2, 0)],
        color: Color::White,
    }, // Z
];

// Game state
pub struct Tetris {
    board: [[Option<Color>; BOARD_WIDTH]; BOARD_HEIGHT],
    current_piece: Tetromino,
    piece_pos: (i8, i8),
    pub score: u32,
    game_over: bool,
    ran: Rng,
}

impl Tetris {
    pub fn new(rng: Rng) -> Self {
        let mut game = Tetris {
            board: [[None; BOARD_WIDTH]; BOARD_HEIGHT],
            current_piece: TETROMINOS[0].clone(),
            piece_pos: (2, 0),
            score: 0,
            game_over: false,
            ran: rng,
        };
        // Check initial spawn
        if !game.can_place(&game.current_piece.shape, game.piece_pos) {
            game.game_over = true;
        }
        game
    }

    pub fn is_game_over(&self) -> bool {
        self.game_over
    }

    // Control functions
    pub fn move_left(&mut self) -> bool {
        self.try_move((-1, 0))
    }

    pub fn move_right(&mut self) -> bool {
        self.try_move((1, 0))
    }

    pub fn move_down(&mut self) -> bool {
        if self.game_over {
            return false;
        }
        if !self.try_move((0, 1)) {
            self.lock_piece();
            self.spawn_new_piece();
            true
        } else {
            false
        }
    }

    pub fn rotate(&mut self) -> bool {
        if self.game_over {
            return false;
        }
        let mut rotated = [(0, 0); 4];
        for i in 0..4 {
            rotated[i] = (
                self.current_piece.shape[i].1,
                3 - self.current_piece.shape[i].0,
            );
        }

        if self.can_place(&rotated, self.piece_pos) {
            self.current_piece.shape = rotated;
            true
        } else {
            false
        }
    }

    fn try_move(&mut self, delta: (i8, i8)) -> bool {
        if self.game_over {
            return false;
        }
        let new_pos = (self.piece_pos.0 + delta.0, self.piece_pos.1 + delta.1);
        if self.can_place(&self.current_piece.shape, new_pos) {
            self.piece_pos = new_pos;
            true
        } else {
            false
        }
    }

    fn can_place(&self, piece: &[(u8, u8); 4], pos: (i8, i8)) -> bool {
        for &(dx, dy) in piece {
            let x = pos.0 + dx as i8;
            let y = pos.1 + dy as i8;
            if x < 0
                || x >= BOARD_WIDTH as i8
                || y >= BOARD_HEIGHT as i8
                || (y >= 0 && self.board[y as usize][x as usize].is_some())
            {
                return false;
            }
        }
        true
    }

    fn lock_piece(&mut self) {
        if self.game_over {
            return;
        }
        for &(dx, dy) in &self.current_piece.shape {
            let x = (self.piece_pos.0 + dx as i8) as usize;
            let y = (self.piece_pos.1 + dy as i8) as usize;
            self.board[y][x] = Some(self.current_piece.color);
        }
        self.check_lines();
    }

    // fn select_new_piece(tetrominos) {

    fn spawn_new_piece(&mut self) {
        if self.game_over {
            return;
        }
        // Simple random selection (in real impl would need RNG)
        let idx = (self.ran.random() % 7) as usize;
        self.current_piece = TETROMINOS[idx].clone();
        self.piece_pos = (3, 0);

        // Check if new piece can be placed, if not, game over
        if !self.can_place(&self.current_piece.shape, self.piece_pos) {
            self.game_over = true;
        }
    }

    fn check_lines(&mut self) {
        if self.game_over {
            return;
        }
        for y in 0..BOARD_HEIGHT {
            if self.board[y].iter().all(|&cell| cell.is_some()) {
                // Clear line
                for yy in (1..=y).rev() {
                    self.board[yy] = self.board[yy - 1];
                }
                self.board[0] = [None; BOARD_WIDTH];
                self.score += 100;
                println!("Score: {}", self.score); // TODO: replace with better scoring logic
            }
        }
    }
}

// Separate drawing function
pub fn draw_on_screen(tetris: &Tetris, f: &mut impl fmt::Write) -> fmt::Result {
    for y in 0..BOARD_HEIGHT {
        write!(f, "|")?;
        for x in 0..BOARD_WIDTH {
            let mut occupied = tetris.board[y][x].is_some();
            if !tetris.game_over {
                for &(dx, dy) in &tetris.current_piece.shape {
                    if (tetris.piece_pos.0 + dx as i8) as usize == x
                        && (tetris.piece_pos.1 + dy as i8) as usize == y
                    {
                        occupied = true;
                    }
                }
            }
            write!(f, "{}", if occupied { "#" } else { " " })?;
        }
        writeln!(f, "|")?;
    }
    if tetris.game_over {
        writeln!(f, "GAME OVER - Score: {}", tetris.score)
    } else {
        writeln!(f, "Score: {}", tetris.score)
    }
}

// global mutable state for button and LED
static RIGHT_BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static MIDDLE_BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static LEFT_BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));

static LED_STATE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(true));

#[handler]
fn handler() {
    critical_section::with(|cs| {
        let mut right_button = RIGHT_BUTTON.borrow_ref_mut(cs);
        let mut middle_button = MIDDLE_BUTTON.borrow_ref_mut(cs);
        let mut left_button = LEFT_BUTTON.borrow_ref_mut(cs);
        let mut led_state = LED_STATE.borrow_ref_mut(cs);
        let Some(right_button) = right_button.as_mut() else {
            // Some other interrupt has occurred
            // before the right_button was set up.
            return;
        };
        let Some(middle_button) = middle_button.as_mut() else {
            return;
        };
        let Some(left_button) = left_button.as_mut() else {
            return;
        };
        if right_button.is_interrupt_set() {
            println!("right_button pressed");
            if *led_state {
                *led_state = false;
            } else {
                *led_state = true;
            }
        }
        if middle_button.is_interrupt_set() {
            println!("middle_button pressed");
            if *led_state {
                *led_state = false;
            } else {
                *led_state = true;
            }
        }
        if left_button.is_interrupt_set() {
            println!("left_button pressed");
            if *led_state {
                *led_state = false;
            } else {
                *led_state = true;
            }
        }
    });
}

fn create_range(a: bool) -> [usize; BOARD_HEIGHT] {
    let mut range: [usize; BOARD_HEIGHT] = [0; BOARD_HEIGHT];

    if a {
        // Range from 0 to BOARD_HEIGHT - 1
        for i in 0..BOARD_HEIGHT {
            range[i] = i;
        }
    } else {
        // Range from BOARD_HEIGHT - 1 to 0
        for i in 0..BOARD_HEIGHT {
            range[i] = BOARD_HEIGHT - 1 - i;
        }
    }

    range
}

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    println!("Init!");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let out_config = OutputConfig::default();
    let mut led = Output::new(peripherals.GPIO8, Level::High, out_config);
    let in_config = InputConfig::default().with_pull(Pull::Up); // Use pull-up resistor for button
    let mut right_button = Input::new(peripherals.GPIO0, in_config);
    let mut middle_button = Input::new(peripherals.GPIO1, in_config);
    let mut left_button = Input::new(peripherals.GPIO2, in_config);

    // SPI setup for MAX7219
    let sclk = peripherals.GPIO4;
    let mosi = peripherals.GPIO6;
    let cs = peripherals.GPIO7;

    let dma_channel = peripherals.DMA_CH0;

    // DMA buffers
    let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = esp_hal::dma_buffers!(32);
    let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
    let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

    let mut spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_mhz(10))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(sclk)
    .with_mosi(mosi)
    .with_cs(cs)
    .with_dma(dma_channel)
    .with_buffers(dma_rx_buf, dma_tx_buf)
    .into_async();

    // Initialize MAX7219
    let mut max7219 = Max7219::new(spi);
    max7219.init().await;
    max7219.clear().await;

    // let mut io = Io::new(peripherals.IO_MUX);
    // io.set_interrupt_handler(handler);

    // critical_section::with(|cs| {
    //     LED_STATE.borrow_ref_mut(cs);
    //     right_button.listen(Event::FallingEdge);
    //     middle_button.listen(Event::FallingEdge);
    //     left_button.listen(Event::FallingEdge);

    //     RIGHT_BUTTON.borrow_ref_mut(cs).replace(right_button);
    //     MIDDLE_BUTTON.borrow_ref_mut(cs).replace(middle_button);
    //     LEFT_BUTTON.borrow_ref_mut(cs).replace(left_button);
    // });

    // TODO: controll using interupts

    let rng = Rng::new(peripherals.RNG);

    println!("Seting up game...");
    let mut game = Tetris::new(rng);

    let mut last_update = time::Instant::now();
    let fall_interval = time::Duration::from_millis(FALL_INTERVAL);

    // Debouncing TODO: implement in inmterupt handlers
    let mut last_key_time = time::Instant::now();
    let debounce_duration = time::Duration::from_millis(250); // 100ms debounce

    println!("Starting game loop...");
    // Game loop
    'game_loop: loop {
        // Handle timing
        let now = time::Instant::now();
        if now - last_update >= fall_interval {
            game.move_down();
            last_update = now;
        }

        if right_button.is_low() {
            println!("right_button pressed!");
            if now - last_key_time > debounce_duration {
                last_key_time = now;
                game.move_right();
            }
        }
        if left_button.is_low() {
            println!("left_button pressed!");
            if now - last_key_time > debounce_duration {
                last_key_time = now;
                game.move_left();
            }
        }
        if middle_button.is_low() {
            println!("middle_button pressed!");
            if now - last_key_time > debounce_duration {
                last_key_time = now;
                game.rotate();
            }
        }

        // Draw game on MAX7219
        max7219.clear().await;
        for module in 0..4 {
            let y_start = module * 8; // Board rows: 0-7, 8-15, 16-23, 24-31

            let mut column_data = [0u8; 8]; // Pre-calculate all column data for this module

            // Collect data for all 8 columns in this module
            for board_y in y_start..(y_start + 8) {
                let local_y = board_y - y_start; // Local row within this module (0-7)

                for board_x in 0..BOARD_WIDTH {
                    let mut occupied = game.board[board_y][board_x].is_some();
                    if !game.game_over {
                        for &(dx, dy) in &game.current_piece.shape {
                            if (game.piece_pos.0 + dx as i8) as usize == board_x
                                && (game.piece_pos.1 + dy as i8) as usize == board_y
                            {
                                occupied = true;
                            }
                        }
                    }
                    if occupied {
                        // For 90 degree rotation: x becomes y, y becomes 7-x
                        // Set the bit in the appropriate column
                        column_data[board_x] |= 1 << (7 - local_y);
                    }
                }
            }

            // Now send each column as a row to the MAX7219
            for col in 0..8 {
                max7219
                    .set_row(4 - module as u8, col as u8, column_data[col])
                    .await;
            }
        }

        if game.is_game_over() {
            break 'game_loop;
        }
    }
    println!("Thanks for playing! You scored {} points.", game.score);
    loop {} // Keep the program running
}
