//! Runs TETRIS on display from sk6812 RGBW LED strip and buttons using interrupts.
//!
//! The following wiring is assumed:
//! - LED => GPIO8
//! - RIGHT_BUTTON => GPIO0 -> GND
//! - MIDDLE_BUTTON => GPIO1 -> GND
//! - LEFT_BUTTON => GPIO2 -> GND
//! - LED_STRIP_DATA => GPIO4
//!
//! Use Monitor to see on the output why is button debouncing important.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::{
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    rmt::{PulseCode, Rmt, TxChannelAsync, TxChannelConfig, TxChannelCreatorAsync},
    rng::Rng,
    time::{self, Rate},
};
use esp_println::println;

impl RandomGenerator for Rng {
    fn next_random(&mut self) -> usize {
        self.random() as usize
    }
}

// global config
const BOARD_WIDTH: usize = 10;
const BOARD_HEIGHT: usize = 20;
const FALL_INTERVAL: u64 = 500; // TODO: should be function of score -> higher score faster speed
const BRIGHTNESS: u8 = 6;
const T0H: u16 = 40;
const T0L: u16 = 85;
const T1H: u16 = 80;
const T1L: u16 = 45;

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
            piece_pos: (3, 0),
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
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let out_config = OutputConfig::default();
    let led = Output::new(peripherals.GPIO8, Level::High, out_config);
    let in_config = InputConfig::default().with_pull(Pull::Up); // Use pull-up resistor for button
    let right_button = Input::new(peripherals.GPIO0, in_config);
    let middle_button = Input::new(peripherals.GPIO1, in_config);
    let left_button = Input::new(peripherals.GPIO2, in_config);

    let freq = Rate::from_mhz(80);
    let rng = Rng::new(peripherals.RNG);
    let rmt = Rmt::new(peripherals.RMT, freq).unwrap().into_async();
    let mut channel = match rmt.channel0.configure(
        peripherals.GPIO4,
        TxChannelConfig::default().with_clk_divider(1),
    ) {
        Ok(channel) => channel,
        Err(err) => {
            panic!(
                "Failed to configure RMT channel for led controll: {:?}",
                err
            );
        }
    };

    let mut game = Tetris::new(rng);
    let mut last_update = time::Instant::now();
    let fall_interval = time::Duration::from_millis(FALL_INTERVAL);

    let mut last_key_time = time::Instant::now();
    let debounce_duration = time::Duration::from_millis(100); // 100ms debounce

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

        // Draw game
        for x in 0..BOARD_WIDTH {
            let range = create_range(x % 2 == 1);
            for y in range {
                if let Some(color) = game.board[y][x] {
                    let (r, g, b) = color_to_rgb(color);
                    let led_bits = create_led_bits(r, g, b, 0);
                    match channel.transmit(&led_bits).await {
                        Ok(_) => {}
                        Err(err) => {
                            println!("Error transmitting data to LED: {:?}", err);
                        }
                    }
                }
                if !game.game_over {
                    for &(dx, dy) in &game.current_piece.shape {
                        if (game.piece_pos.0 + dx as i8) as usize == x
                            && (game.piece_pos.1 + dy as i8) as usize == y
                        {
                            let (r, g, b) = color_to_rgb(game.current_piece.color);
                            let led_bits = create_led_bits(r, g, b, 0);
                            match channel.transmit(&led_bits).await {
                                Ok(_) => {}
                                Err(err) => {
                                    println!("Error transmitting data to LED: {:?}", err);
                                }
                            }
                        }
                    }
                }
            }
        }

        if game.is_game_over() {
            break 'game_loop;
        }
    }
    println!("Thanks for playing! You scored {} points.", game.score);
    loop {} // Keep the program running
}
