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
    interrupt::software::SoftwareInterruptControl,
    rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator},
    rng::Rng,
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;

esp_bootloader_esp_idf::esp_app_desc!();

const T0H: u16 = 40;
const T0L: u16 = 85;
const T1H: u16 = 80;
const T1L: u16 = 45;

fn create_led_bits(r: u8, g: u8, b: u8, w: u8) -> [PulseCode; 33] {
    let mut data = [PulseCode::default(); 33];
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

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let freq = Rate::from_mhz(80);

    let rmt = Rmt::new(peripherals.RMT, freq).unwrap().into_async();

    let mut channel = rmt
        .channel0
        .configure_tx(&TxChannelConfig::default().with_clk_divider(1))
        .unwrap()
        .with_pin(peripherals.GPIO4);

    let mut rng = Rng::new();

    loop {
        println!("Settings LED colors:");
        for _ in 0..5 {
            let r = rng.random() % 5;
            let g = rng.random() % 5;
            let b = rng.random() % 5;
            let w = 0;

            let data = create_led_bits(r as u8, g as u8, b as u8, w as u8);
            channel.transmit(&data).await.unwrap();
        }
        Timer::after(Duration::from_millis(100)).await;
    }
}