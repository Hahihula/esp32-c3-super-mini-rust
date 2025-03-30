//! Blinks an LED on pres of button but this time using interrupts.
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
    gpio::{Event, Input, InputConfig, Io, Level, Output, OutputConfig, Pull},
    handler, main,
};
use esp_println::println;

use core::cell::RefCell;
use critical_section::Mutex;

// global mutable state for button and LED
static BUTTON: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static LED_STATE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(true));

#[handler]
fn handler() {
    critical_section::with(|cs| {
        let mut button = BUTTON.borrow_ref_mut(cs);
        let mut led_state = LED_STATE.borrow_ref_mut(cs);
        let Some(button) = button.as_mut() else {
            // Some other interrupt has occurred
            // before the button was set up.
            return;
        };
        if button.is_interrupt_set() {
            println!("Button pressed");
            if *led_state {
                *led_state = false;
            } else {
                *led_state = true;
            }
        }
    });
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let out_config = OutputConfig::default();
    let mut led = Output::new(peripherals.GPIO8, Level::High, out_config);
    let in_config = InputConfig::default().with_pull(Pull::Up); // Use pull-up resistor for button
    let mut button = Input::new(peripherals.GPIO0, in_config);

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(handler);

    critical_section::with(|cs| {
        LED_STATE.borrow_ref_mut(cs);
        button.listen(Event::FallingEdge);
        BUTTON.borrow_ref_mut(cs).replace(button);
    });

    let delay = Delay::new();

    loop {
        let led_state = critical_section::with(|cs| *LED_STATE.borrow_ref(cs));
        if led_state {
            led.set_level(Level::High);
        } else {
            led.set_level(Level::Low);
        }
        println!("Nothing to do");

        delay.delay_millis(100);
    }
}
