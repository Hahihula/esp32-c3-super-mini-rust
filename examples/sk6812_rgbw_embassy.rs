//! Demonstrates blinking LEDs using RMT and pulse sequences
//!
//! Connect a sk6812 RGBW LED strip to GPIO4.
//!
//! The following wiring is assumed:
//! - led_strip_data => GPIO4

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{
    gpio::Level,
    rmt::{PulseCode, Rmt, TxChannelAsync, TxChannelConfig, TxChannelCreatorAsync},
    rng::Rng,
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;

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

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    let freq = Rate::from_mhz(80);

    let rmt = Rmt::new(peripherals.RMT, freq).unwrap().into_async();

    let mut channel = rmt
        .channel0
        .configure(
            peripherals.GPIO4,
            TxChannelConfig::default().with_clk_divider(1),
        )
        .unwrap();

    let mut rng = Rng::new(peripherals.RNG);

    // let led_colors = [
    //     (5, 0, 0, 0),    // Red
    //     (0, 5, 0, 0),    // Green
    //     (0, 0, 5, 0),    // Blue
    //     (0, 0, 0, 5),    // White
    // ];

    loop {
        println!("Settings LED colors:");
        // for &(r, g, b, w) in led_colors.iter() {
        //     let data = create_led_bits(r, g, b, w);
        //     channel.transmit(&data).await.unwrap();
        // }
        for i in 0..5 {
            let r = rng.random() % 5;
            let g = rng.random() % 5;
            let b = rng.random() % 5;
            let w = 0; // turn off white

            let data = create_led_bits(r as u8, g as u8, b as u8, w as u8);
            channel.transmit(&data).await.unwrap();
        }
        Timer::after(Duration::from_millis(100)).await;
    }
}
