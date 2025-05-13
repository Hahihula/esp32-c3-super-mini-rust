//! Reads an AHT20
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

// AHT20 constants
const AHT20_ADDR: u8 = 0x38; // I2C address of AHT20
const CMD_INIT: u8 = 0xBE; // Initialize command
const CMD_MEASURE: u8 = 0xAC; // Trigger measurement command
const INIT_PARAM1: u8 = 0x08;
const INIT_PARAM2: u8 = 0x00;
const MEASURE_PARAM1: u8 = 0x33;
const MEASURE_PARAM2: u8 = 0x00;

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // let mut led = Output::new(peripherals.GPIO8, Level::High, config);

    let delay = Delay::new();

    let config = I2cConfig::default().with_frequency(Rate::from_khz(400));
    // Initialize I2C
    let mut i2c: I2c<'_, Blocking> = match I2c::new(peripherals.I2C0, config) {
        Ok(i2c) => i2c,
        Err(e) => {
            panic!("Failed to initialize I2C: {:?}", e);
        }
    }
    .with_sda(peripherals.GPIO8)
    .with_scl(peripherals.GPIO9);

    // Initialize AHT20
    match i2c.write(AHT20_ADDR, &[CMD_INIT, INIT_PARAM1, INIT_PARAM2]) {
        Ok(_) => {
            println!("AHT20 sensor initialized!");
        }
        Err(e) => {
            panic!("Failed to initialize AHT20: {:?}", e);
        }
    }
    delay.delay_millis(40);

    loop {
        if i2c
            .write(AHT20_ADDR, &[CMD_MEASURE, MEASURE_PARAM1, MEASURE_PARAM2])
            .is_err()
        {
            println!("Failed to send measurement command to AHT20");
        }

        // Wait for measurement to complete (at least 80ms)
        delay.delay_millis(80);

        // Read 7 bytes of data
        let mut buffer = [0u8; 7];
        if i2c.read(AHT20_ADDR, &mut buffer).is_err() {
            println!("Failed to read data from AHT20");
        }

        // Check status bit for calibration
        if (buffer[0] & 0x08) == 0 {
            println!("AHT20 sensor is not calibrated!");
            println!("Resetting AHT20 sensor...");

            // Soft reset command
            i2c.write(AHT20_ADDR, &[0xBA]);
            delay.delay_millis(80);

            println!("Initializing AHT20 sensor...");
            i2c.write(AHT20_ADDR, &[CMD_INIT, INIT_PARAM1, INIT_PARAM2]);

            // Wait for calibration to complete - at least 10ms recommended
            delay.delay_millis(500);

            // Check if calibration was successful
            let mut status = [0u8];
            i2c.write_read(AHT20_ADDR, &[0x71], &mut status);

            if (status[0] & 0x08) == 0 {
                println!(
                    "Calibration still not successful. Status: {:02x}",
                    status[0]
                );
            } else {
                println!("Calibration successful!");
            }
        }

        // Check if device is busy
        if (buffer[0] & 0x80) != 0 {
            println!("AHT20 sensor is busy!");
        }

        // Process humidity data (20 bits) from buffer[1], buffer[2], and buffer[3]
        let humidity_raw =
            ((buffer[1] as u32) << 12) | ((buffer[2] as u32) << 4) | ((buffer[3] as u32) >> 4);
        let humidity = (humidity_raw as f32) * 100.0 / 1048576.0;

        // Process temperature data (20 bits) from buffer[3], buffer[4], and buffer[5]
        let temp_raw =
            ((buffer[3] as u32 & 0x0F) << 16) | ((buffer[4] as u32) << 8) | (buffer[5] as u32);
        let temperature = (temp_raw as f32) * 200.0 / 1048576.0 - 50.0;

        // Validate data is in reasonable ranges
        if !(0.0..=100.0).contains(&humidity) || !(-40.0..=85.0).contains(&temperature) {
            println!(
                "Invalid data from AHT20: temperature = {}, humidity = {}",
                temperature, humidity
            )
        } else {
            println!(
                "Temperature: {:.2} °C, Humidity: {:.2} %",
                temperature, humidity
            );
        }

        // Wait 2 seconds between readings
        delay.delay_millis(2000);
    }
}
