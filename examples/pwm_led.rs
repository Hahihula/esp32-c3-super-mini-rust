//! Fades an LED using hardware PWM
//!
//! The following wiring is assumed:
//! - LED => GPIO8

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::DriveMode,
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self, LSClockSource, TimerIFace},
        Ledc, LowSpeed, LSGlobalClkSource,
    },
    main,
    time::Rate,
};
use esp_println::println;

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    println!("Init PWM LED fade!");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let delay = Delay::new();

    // 1. Create the LEDC controller and set the global slow clock
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    // 2. Configure a low-speed timer
    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    lstimer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty13Bit,
            clock_source: LSClockSource::APBClk,
            frequency: Rate::from_khz(5),
        })
        .unwrap();

    // 3. Configure the channel on GPIO8
    let mut channel0 = ledc.channel(channel::Number::Channel0, peripherals.GPIO8);
    channel0
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 0,
            drive_mode: DriveMode::PushPull,
        })
        .unwrap();

    loop {
        println!("Fade in...");
        // 0% -> 100% over 2000 ms using the hardware fade engine
        channel0.start_duty_fade(0, 100, 10000).unwrap();
        while channel0.is_duty_fade_running() {}

        delay.delay_millis(500);

        println!("Fade out...");
        // 100% -> 0% over 2000 ms
        channel0.start_duty_fade(100, 0, 10000).unwrap();
        while channel0.is_duty_fade_running() {}

        delay.delay_millis(5000);
    }
}