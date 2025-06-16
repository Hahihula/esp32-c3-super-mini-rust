//! Reads an AS5600
//!
//! The following wiring is assumed:
//! - SDA => GPIO8
//! - SCL => GPIO9
#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    i2c::master::{Config as I2cConfig, I2c},
    main,
    time::Rate,
    Blocking,
};
use esp_println::println;

// AS5600 constants
const AS5600_ADDR: u8 = 0x36; // I2C address of AS5600
const REG_RAW_ANGLE: u8 = 0x0C; // Register for raw angle (high byte)
const REG_STATUS: u8 = 0x0B; // Status register
const REG_CONF: u8 = 0x07; // Configuration register
const CONF_INIT: u8 = 0x00; // Default configuration: normal power mode
const CONF_INIT2: u8 = 0x00; // Second byte for configuration

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let delay = Delay::new();

    // Lower I2C frequency to 100 kHz for stability
    let config = I2cConfig::default().with_frequency(Rate::from_khz(100));
    // Initialize I2C
    let mut i2c: I2c<'_, Blocking> = match I2c::new(peripherals.I2C0, config) {
        Ok(i2c) => i2c,
        Err(e) => {
            panic!("Failed to initialize I2C: {:?}", e);
        }
    }
    .with_sda(peripherals.GPIO8)
    .with_scl(peripherals.GPIO9);

    // Initialize AS5600
    match i2c.write(AS5600_ADDR, &[REG_CONF, CONF_INIT, CONF_INIT2]) {
        Ok(_) => {
            println!("AS5600 sensor initialized!");
        }
        Err(e) => {
            panic!("Failed to initialize AS5600: {:?}", e);
        }
    }
    delay.delay_millis(10); // Short delay after initialization

    loop {
        // Check magnet status with retries
        let mut status = [0u8];
        let mut status_ok = false;
        for _ in 0..3 {
            if i2c
                .write_read(AS5600_ADDR, &[REG_STATUS], &mut status)
                .is_ok()
            {
                status_ok = true;
                break;
            }
            println!("Failed to read status from AS5600, retrying...");
            delay.delay_millis(10);
        }
        if !status_ok {
            println!("Failed to read status from AS5600 after retries");
            delay.delay_millis(2000);
            continue;
        }

        // Status bits: MH (0x20) = too strong, ML (0x10) = too weak, MD (0x08) = detected
        let magnet_detected = status[0] & 0x08 != 0;
        let magnet_too_strong = status[0] & 0x20 != 0;
        let magnet_too_weak = status[0] & 0x10 != 0;

        // Check for invalid status (MH and ML both set)
        if magnet_too_strong && magnet_too_weak {
            println!(
                "Invalid status: MH and ML both set (0x{:02X}), retrying...",
                status[0]
            );
            delay.delay_millis(10);
            continue;
        }

        if !magnet_detected || magnet_too_strong || magnet_too_weak {
            println!(
                "Magnet issue: Detected={}, Too Strong={}, Too Weak={} (Status: 0x{:02X})",
                magnet_detected, magnet_too_strong, magnet_too_weak, status[0]
            );
        }

        // Read raw angle (2 bytes: high byte at 0x0C, low byte at 0x0D) with retries
        let mut angle_buffer = [0u8; 2];
        let mut angle_ok = false;
        for _ in 0..3 {
            if i2c
                .write_read(AS5600_ADDR, &[REG_RAW_ANGLE], &mut angle_buffer)
                .is_ok()
            {
                angle_ok = true;
                break;
            }
            println!("Failed to read angle from AS5600, retrying...");
            delay.delay_millis(10);
        }
        if !angle_ok {
            println!("Failed to read angle from AS5600 after retries");
            delay.delay_millis(2000);
            continue;
        }

        // Combine high and low bytes into 12-bit raw angle (0-4095)
        let raw_angle = ((angle_buffer[0] as u16) << 4) | ((angle_buffer[1] as u16) >> 4);
        // Convert to degrees (360° / 4096 steps)
        let angle_degrees = (raw_angle as f32) * 360.0 / 4096.0;

        // Print raw data for debugging
        println!(
            "Raw angle: {} (0x{:04X}), Buffer: [{}, {}]",
            raw_angle, raw_angle, angle_buffer[0], angle_buffer[1]
        );

        // Validate angle is in reasonable range
        if !(0.0..=360.0).contains(&angle_degrees) {
            println!("Invalid angle from AS5600: {}", angle_degrees);
        } else {
            println!("Angle: {:.2} °", angle_degrees);
        }

        // Wait 500ms between readings
        delay.delay_millis(500);
    }
}
