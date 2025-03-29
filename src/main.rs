//! Blinks an LED
//!
//! The following wiring is assumed:
//! - LED => GPIO8

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
};
use esp_println::println;

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let config = OutputConfig::default();
    let mut led = Output::new(peripherals.GPIO8, Level::High, config);

    let delay = Delay::new();

    loop {
        led.toggle();
        delay.delay_millis(500);
        led.toggle();
        println!("Blink!");
        delay.delay_millis(1000);
    }
}
