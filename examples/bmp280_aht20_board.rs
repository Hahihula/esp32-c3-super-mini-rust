//! Reads a BMP280 pressure and temperature sensor
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

// BMP280 constants
const BMP280_ADDR_PRIMARY: u8 = 0x76; // Primary I2C address
const BMP280_ADDR_SECONDARY: u8 = 0x77; // Secondary I2C address (when SDO pin is pulled high)
const BMP280_ID: u8 = 0x58; // BMP280 chip ID

// ATH20 constants
const AHT20_ADDR: u8 = 0x38; // I2C address of AHT20
const CMD_INIT: u8 = 0xBE; // Initialize command
const INIT_PARAM1: u8 = 0x08;
const INIT_PARAM2: u8 = 0x00;

// Register addresses
const REG_ID: u8 = 0xD0;
const REG_RESET: u8 = 0xE0;
const REG_STATUS: u8 = 0xF3;
const REG_CTRL_MEAS: u8 = 0xF4;
const REG_CONFIG: u8 = 0xF5;
const REG_PRESS_MSB: u8 = 0xF7;
const REG_CALIB_START: u8 = 0x88;

// Commands
const RESET_CMD: u8 = 0xB6;

// Calibration data structure
#[derive(Debug, Default)]
struct CalibrationData {
    dig_t1: u16,
    dig_t2: i16,
    dig_t3: i16,
    dig_p1: u16,
    dig_p2: i16,
    dig_p3: i16,
    dig_p4: i16,
    dig_p5: i16,
    dig_p6: i16,
    dig_p7: i16,
    dig_p8: i16,
    dig_p9: i16,
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
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

    // Wait for sensors to power up and stabilize
    delay.delay_millis(100);

    // Try both possible BMP280 addresses
    let bmp280_addr = detect_bmp280_address(&mut i2c, &delay);

    println!("Using BMP280 address: 0x{:02X}", bmp280_addr);

    // Check if BMP280 is connected
    let mut id_buffer = [0u8];
    match i2c.write_read(bmp280_addr, &[REG_ID], &mut id_buffer) {
        Ok(_) => {
            if id_buffer[0] == BMP280_ID {
                println!("BMP280 sensor found! ID: 0x{:02X}", id_buffer[0]);
            } else {
                println!(
                    "Warning: Unexpected chip ID: 0x{:02X}, expected: 0x{:02X}",
                    id_buffer[0], BMP280_ID
                );
                println!("Will try to continue anyway...");
            }
        }
        Err(e) => {
            println!("Failed to read BMP280 ID after detection: {:?}", e);
            println!("Will try to continue anyway...");
        }
    }

    // Reset the sensor
    match i2c.write(bmp280_addr, &[REG_RESET, RESET_CMD]) {
        Ok(_) => {
            println!("BMP280 sensor reset!");
        }
        Err(e) => {
            println!("Failed to reset BMP280: {:?}", e);
            println!("Will try to continue anyway...");
        }
    }
    delay.delay_millis(50); // Wait for reset to complete

    // Read calibration data
    let mut calib_buffer = [0u8; 24]; // 24 bytes of calibration data
    let mut calib_data = CalibrationData::default(); // Initialize with default values

    match i2c.write_read(bmp280_addr, &[REG_CALIB_START], &mut calib_buffer) {
        Ok(_) => {
            println!("Calibration data read successfully!");
            // Parse calibration data
            calib_data = parse_calibration_data(&calib_buffer);
            println!("Calibration data parsed!");
        }
        Err(e) => {
            println!("Failed to read calibration data: {:?}", e);
            println!("Using default calibration values, measurements may be inaccurate");
            // Continue with default calibration values
        }
    }

    // Configure the sensor - 0xB7: Normal mode, temperature and pressure oversampling x4
    match i2c.write(bmp280_addr, &[REG_CTRL_MEAS, 0xB7]) {
        Ok(_) => {
            println!("BMP280 sensor configured!");
        }
        Err(e) => {
            println!("Failed to configure BMP280: {:?}", e);
            println!("Will try to continue anyway...");
        }
    }

    // Wait after configuration
    delay.delay_millis(50);

    // Configure filter and standby time - 0x50: Filter coefficient 8, standby time 500ms
    match i2c.write(bmp280_addr, &[REG_CONFIG, 0x50]) {
        Ok(_) => {
            println!("BMP280 filter configured!");
        }
        Err(e) => {
            println!("Failed to configure BMP280 filter: {:?}", e);
            println!("Will try to continue anyway...");
        }
    }
    delay.delay_millis(100);

    let mut t_fine: i32 = 0; // Global temperature adjustment variable

    // Initialize AHT20 (since both sensors are on the same board)
    match i2c.write(AHT20_ADDR, &[CMD_INIT, INIT_PARAM1, INIT_PARAM2]) {
        Ok(_) => {
            println!("AHT20 sensor initialized!");
        }
        Err(e) => {
            println!("Failed to initialize AHT20: {:?}", e);
            println!("Will continue with BMP280 only...");
        }
    }
    delay.delay_millis(40);

    loop {
        // Read BMP280 temperature and pressure data
        let mut bmp280_data_valid = false;
        let mut temperature = 0.0f32;
        let mut pressure_hpa = 0.0f32;

        // Read temperature and pressure data - 6 bytes starting from 0xF7
        let mut data_buffer = [0u8; 6];
        if let Ok(_) = i2c.write_read(bmp280_addr, &[REG_PRESS_MSB], &mut data_buffer) {
            // Extract pressure and temperature raw values
            let pressure_raw = ((data_buffer[0] as u32) << 12)
                | ((data_buffer[1] as u32) << 4)
                | ((data_buffer[2] as u32) >> 4);
            let temp_raw = ((data_buffer[3] as u32) << 12)
                | ((data_buffer[4] as u32) << 4)
                | ((data_buffer[5] as u32) >> 4);

            // Calculate temperature
            let (temp_value, t_fine_value) = compensate_temperature(temp_raw, &calib_data);
            temperature = temp_value;
            t_fine = t_fine_value;

            // Calculate pressure
            let pressure = compensate_pressure(pressure_raw, t_fine, &calib_data);

            // Convert pressure to hPa (Pa / 100)
            pressure_hpa = pressure as f32 / 100.0;

            bmp280_data_valid = true;
        } else {
            println!("Failed to read measurement data from BMP280");
        }

        // Now read from AHT20
        let mut aht20_data_valid = false;
        let mut humidity = 0.0f32;
        let mut aht20_temperature = 0.0f32;

        // AHT20 constants for measurement
        const CMD_MEASURE: u8 = 0xAC;
        const MEASURE_PARAM1: u8 = 0x33;
        const MEASURE_PARAM2: u8 = 0x00;

        if i2c
            .write(AHT20_ADDR, &[CMD_MEASURE, MEASURE_PARAM1, MEASURE_PARAM2])
            .is_ok()
        {
            // Wait for measurement to complete (at least 80ms)
            delay.delay_millis(80);

            // Read 7 bytes of data
            let mut aht_buffer = [0u8; 7];
            if let Ok(_) = i2c.read(AHT20_ADDR, &mut aht_buffer) {
                // Check status bit for calibration
                if (aht_buffer[0] & 0x08) == 0 {
                    println!("AHT20 sensor is not calibrated!");
                } else if (aht_buffer[0] & 0x80) != 0 {
                    println!("AHT20 sensor is busy!");
                } else {
                    // Process humidity data (20 bits) from buffer[1], buffer[2], and buffer[3]
                    let humidity_raw = ((aht_buffer[1] as u32) << 12)
                        | ((aht_buffer[2] as u32) << 4)
                        | ((aht_buffer[3] as u32) >> 4);
                    humidity = (humidity_raw as f32) * 100.0 / 1048576.0;

                    // Process temperature data (20 bits) from buffer[3], buffer[4], and buffer[5]
                    let temp_raw = ((aht_buffer[3] as u32 & 0x0F) << 16)
                        | ((aht_buffer[4] as u32) << 8)
                        | (aht_buffer[5] as u32);
                    aht20_temperature = (temp_raw as f32) * 200.0 / 1048576.0 - 50.0;

                    aht20_data_valid = true;
                }
            } else {
                println!("Failed to read data from AHT20");
            }
        } else {
            println!("Failed to send measurement command to AHT20");
        }

        // Print all available data
        if bmp280_data_valid && aht20_data_valid {
            println!(
                "BMP280: Temp: {:.2} °C, Pressure: {:.2} hPa | AHT20: Temp: {:.2} °C, Humidity: {:.2} %",
                temperature, pressure_hpa, aht20_temperature, humidity
            );
        } else if bmp280_data_valid {
            println!(
                "BMP280: Temperature: {:.2} °C, Pressure: {:.2} hPa | AHT20: No data",
                temperature, pressure_hpa
            );
        } else if aht20_data_valid {
            println!(
                "BMP280: No data | AHT20: Temperature: {:.2} °C, Humidity: {:.2} %",
                aht20_temperature, humidity
            );
        } else {
            println!("No valid data from either sensor!");
        }

        // Wait 2 seconds between readings
        delay.delay_millis(2000);
    }
}

// Parse calibration data from buffer
fn parse_calibration_data(buffer: &[u8; 24]) -> CalibrationData {
    CalibrationData {
        dig_t1: u16::from_le_bytes([buffer[0], buffer[1]]),
        dig_t2: i16::from_le_bytes([buffer[2], buffer[3]]),
        dig_t3: i16::from_le_bytes([buffer[4], buffer[5]]),
        dig_p1: u16::from_le_bytes([buffer[6], buffer[7]]),
        dig_p2: i16::from_le_bytes([buffer[8], buffer[9]]),
        dig_p3: i16::from_le_bytes([buffer[10], buffer[11]]),
        dig_p4: i16::from_le_bytes([buffer[12], buffer[13]]),
        dig_p5: i16::from_le_bytes([buffer[14], buffer[15]]),
        dig_p6: i16::from_le_bytes([buffer[16], buffer[17]]),
        dig_p7: i16::from_le_bytes([buffer[18], buffer[19]]),
        dig_p8: i16::from_le_bytes([buffer[20], buffer[21]]),
        dig_p9: i16::from_le_bytes([buffer[22], buffer[23]]),
    }
}

// Compensate temperature according to BMP280 datasheet formulas
fn compensate_temperature(raw_temp: u32, calib: &CalibrationData) -> (f32, i32) {
    let var1: i32 =
        (((raw_temp as i32) >> 3) - ((calib.dig_t1 as i32) << 1)) * (calib.dig_t2 as i32) >> 11;
    let var2: i32 = (((((raw_temp as i32) >> 4) - (calib.dig_t1 as i32))
        * ((raw_temp as i32) >> 4)
        - (calib.dig_t1 as i32))
        >> 12)
        * (calib.dig_t3 as i32)
        >> 14;
    let t_fine: i32 = var1 + var2;
    let temperature: f32 = (t_fine * 5 + 128) as f32 / 256.0 / 100.0;
    (temperature, t_fine)
}

// Compensate pressure according to BMP280 datasheet formulas
fn compensate_pressure(raw_pressure: u32, t_fine: i32, calib: &CalibrationData) -> u32 {
    // Use large integer to prevent overflow
    let mut var1: i64 = (t_fine as i64) - 128000;
    let mut var2: i64 = var1 * var1 * (calib.dig_p6 as i64);
    var2 = var2 + ((var1 * (calib.dig_p5 as i64)) << 17);
    var2 = var2 + ((calib.dig_p4 as i64) << 35);
    var1 = ((var1 * var1 * (calib.dig_p3 as i64)) >> 8) + ((var1 * (calib.dig_p2 as i64)) << 12);
    var1 = (((1 as i64) << 47) + var1) * (calib.dig_p1 as i64) >> 33;

    if var1 == 0 {
        return 0; // Avoid division by zero
    }

    let mut p: i64 = 1048576 - (raw_pressure as i64);
    p = (((p << 31) - var2) * 3125) / var1;
    var1 = ((calib.dig_p9 as i64) * (p >> 13) * (p >> 13)) >> 25;
    var2 = ((calib.dig_p8 as i64) * p) >> 19;
    p = ((p + var1 + var2) >> 8) + ((calib.dig_p7 as i64) << 4);

    // Final conversion and range check
    if p < 30000 || p > 110000 {
        // Invalid pressure range (300-1100 hPa is normal on Earth)
        // Return a sentinel value or use previous valid reading
        println!("Warning: Invalid pressure calculation: {} Pa", p);
        return 101325; // Standard pressure in Pa as fallback
    }

    (p as u32) // Return pressure in Pa
}

// Function to detect which address the BMP280 is using
fn detect_bmp280_address(i2c: &mut I2c<'_, Blocking>, delay: &Delay) -> u8 {
    // Try primary address first
    let mut id_buffer = [0u8];

    match i2c.write_read(BMP280_ADDR_PRIMARY, &[REG_ID], &mut id_buffer) {
        Ok(_) => {
            if id_buffer[0] == BMP280_ID {
                println!(
                    "BMP280 found at primary address 0x{:02X}!",
                    BMP280_ADDR_PRIMARY
                );
                return BMP280_ADDR_PRIMARY;
            } else {
                println!(
                    "Device at 0x{:02X} returned ID 0x{:02X}, not BMP280",
                    BMP280_ADDR_PRIMARY, id_buffer[0]
                );
            }
        }
        Err(_) => {
            println!(
                "No response from 0x{:02X}, trying secondary address",
                BMP280_ADDR_PRIMARY
            );
        }
    }

    // Try secondary address
    match i2c.write_read(BMP280_ADDR_SECONDARY, &[REG_ID], &mut id_buffer) {
        Ok(_) => {
            if id_buffer[0] == BMP280_ID {
                println!(
                    "BMP280 found at secondary address 0x{:02X}!",
                    BMP280_ADDR_SECONDARY
                );
                return BMP280_ADDR_SECONDARY;
            } else {
                println!(
                    "Device at 0x{:02X} returned ID 0x{:02X}, not BMP280",
                    BMP280_ADDR_SECONDARY, id_buffer[0]
                );
            }
        }
        Err(_) => {
            println!("No response from 0x{:02X} either", BMP280_ADDR_SECONDARY);
        }
    }

    // If we get here, we couldn't find the BMP280
    println!("Could not find BMP280 at either address. Is it connected properly?");
    println!("Will try to continue with primary address, but expect errors...");
    BMP280_ADDR_PRIMARY
}
