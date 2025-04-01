//! Blinks an LED on pres of button
//!
//! The following wiring is assumed:
//! - LED => GPIO8
//! - Button => GPIO0 -> GND
//!
//! Use Monitor to see on the output why is button debouncing important.

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    main, time,
};
use esp_println::println;

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let out_config = OutputConfig::default();
    let mut led = Output::new(peripherals.GPIO8, Level::High, out_config);
    let in_config = InputConfig::default().with_pull(Pull::Up); // Use pull-up resistor for button
    let button = Input::new(peripherals.GPIO0, in_config);

    let delay = Delay::new();

    let mut last_button_change = time::Instant::now().duration_since_epoch().as_millis();

    loop {
        if button.is_low() {
            println!("Button pressed!");
            let now = time::Instant::now().duration_since_epoch().as_millis();
            if now - last_button_change > 150 {
                last_button_change = now;
                led.toggle();
                println!("Blink!");
            }
        }
        delay.delay_millis(10);
    }
}
