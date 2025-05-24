//! Reads a VL53L0X Time-of-Flight distance sensor
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

// VL53L0X constants
const VL53L0X_ADDR: u8 = 0x29; // I2C address of VL53L0X

// Register addresses
const REG_SYSRANGE_START: u8 = 0x00;
const REG_RESULT_INTERRUPT_STATUS: u8 = 0x13;
const REG_RESULT_RANGE_STATUS: u8 = 0x14;
const REG_RESULT_CORE_AMBIENT_WINDOW_EVENTS_RTN: u8 = 0xBC;
const REG_RESULT_CORE_RANGING_TOTAL_EVENTS_RTN: u8 = 0xC0;
const REG_RESULT_CORE_AMBIENT_WINDOW_EVENTS_REF: u8 = 0xD0;
const REG_RESULT_CORE_RANGING_TOTAL_EVENTS_REF: u8 = 0xD4;
const REG_RESULT_PEAK_SIGNAL_RATE_REF: u8 = 0xB6;
const REG_ALGO_PART_TO_PART_RANGE_OFFSET_MM: u8 = 0x28;
const REG_I2C_SLAVE_DEVICE_ADDRESS: u8 = 0x8A;
const REG_SYSTEM_RANGE_CONFIG: u8 = 0x09;
const REG_VHV_CONFIG_PAD_SCL_SDA_EXTSUP_HV: u8 = 0x89;
const REG_MSRC_CONFIG_CONTROL: u8 = 0x60;
const REG_SYSTEM_SEQUENCE_CONFIG: u8 = 0x01;
const REG_FINAL_RANGE_CONFIG_MIN_COUNT_RATE_RTN_LIMIT: u8 = 0x44;
const REG_GLOBAL_CONFIG_VCSEL_WIDTH: u8 = 0x32;
const REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_0: u8 = 0xB0;
const REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_1: u8 = 0xB1;
const REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_2: u8 = 0xB2;
const REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_3: u8 = 0xB3;
const REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_4: u8 = 0xB4;
const REG_GLOBAL_CONFIG_SPAD_ENABLES_REF_5: u8 = 0xB5;
const REG_GLOBAL_CONFIG_REF_EN_START_SELECT: u8 = 0xB6;
const REG_DYNAMIC_SPAD_NUM_REQUESTED_REF_SPAD: u8 = 0x4E;
const REG_DYNAMIC_SPAD_REF_EN_START_OFFSET: u8 = 0x4F;
const REG_POWER_MANAGEMENT_GO1_POWER_FORCE: u8 = 0x80;
const REG_WHO_AM_I: u8 = 0xC0;
const REG_VHV_CONFIG_TIMEOUT_MACROP_LOOP_BOUND: u8 = 0x08;
const REG_SYSTEM_THRESH_HIGH: u8 = 0x0C;
const REG_SYSTEM_THRESH_LOW: u8 = 0x0E;
const REG_SYSTEM_HISTOGRAM_BIN: u8 = 0x81;
const REG_SYSTEM_INTERRUPT_CONFIG_GPIO: u8 = 0x0A;
const REG_SYSTEM_INTERRUPT_CLEAR: u8 = 0x0B;
const REG_SYSTEM_CONTROL: u8 = 0xE0;
const REG_RESULT_RANGE_VAL: u8 = 0x62;

// Model ID and expected value
const EXPECTED_DEVICE_ID: u8 = 0xEE;

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

    // Power on delay
    delay.delay_millis(100);

    // Check device ID
    let mut id_buffer = [0u8];
    match i2c.write_read(VL53L0X_ADDR, &[REG_WHO_AM_I], &mut id_buffer) {
        Ok(_) => {
            if id_buffer[0] != EXPECTED_DEVICE_ID {
                panic!(
                    "Unexpected device ID: expected 0x{:02X}, got 0x{:02X}",
                    EXPECTED_DEVICE_ID, id_buffer[0]
                );
            }
            println!("VL53L0X sensor found! Device ID: 0x{:02X}", id_buffer[0]);
        }
        Err(e) => {
            panic!("Failed to read device ID: {:?}", e);
        }
    }

    // Sensor initialization sequence
    // This is a more complete initialization sequence based on ST's datasheet

    // Data initialization
    // Set bit 0 of VHV config to enable high voltage pad
    if write_register(&mut i2c, REG_VHV_CONFIG_PAD_SCL_SDA_EXTSUP_HV, 0x01).is_err() {
        println!("Failed to set high voltage pad control");
    }

    // Set I2C standard mode (not sure if needed for ESP32, but good practice)
    if write_register(&mut i2c, 0x88, 0x00).is_err() {
        println!("Failed to set I2C standard mode");
    }

    // Set pulse period pre-range to 18 VCSEL periods
    if write_register(&mut i2c, 0x50, 0x00).is_err() {
        println!("Failed to set pulse period pre-range");
    }
    if write_register(&mut i2c, 0x51, 0x12).is_err() {
        println!("Failed to set pulse period pre-range");
    }

    // Set pulse period final range to 14 VCSEL periods
    if write_register(&mut i2c, 0x52, 0x00).is_err() {
        println!("Failed to set pulse period final range");
    }
    if write_register(&mut i2c, 0x53, 0x0E).is_err() {
        println!("Failed to set pulse period final range");
    }

    // MSRC (Minimum Signal Rate Check) Configuration
    if write_register(&mut i2c, REG_MSRC_CONFIG_CONTROL, 0x12).is_err() {
        println!("Failed to set MSRC config");
    }

    // Set dynamic SPAD selection - these are crucial for proper operation
    if write_register(&mut i2c, 0x60, 0x00).is_err() {
        println!("Failed to configure dynamic SPAD selection");
    }
    if write_register(&mut i2c, 0x61, 0x00).is_err() {
        println!("Failed to configure dynamic SPAD selection");
    }
    if write_register(&mut i2c, 0x62, 0x00).is_err() {
        println!("Failed to configure dynamic SPAD selection");
    }

    // Static SPAD config - crucial for proper operation
    // Set reference SPAD map (ST's default for bare module)
    if write_register(&mut i2c, 0xB0, 0x00).is_err() {
        println!("Failed to set reference SPAD map");
    }
    if write_register(&mut i2c, 0xB1, 0x10).is_err() {
        println!("Failed to set reference SPAD map");
    }
    if write_register(&mut i2c, 0xB2, 0x00).is_err() {
        println!("Failed to set reference SPAD map");
    }
    if write_register(&mut i2c, 0xB3, 0x00).is_err() {
        println!("Failed to set reference SPAD map");
    }
    if write_register(&mut i2c, 0xB4, 0x00).is_err() {
        println!("Failed to set reference SPAD map");
    }
    if write_register(&mut i2c, 0xB5, 0x00).is_err() {
        println!("Failed to set reference SPAD map");
    }

    // Default SPAD count and type
    if write_register(&mut i2c, REG_DYNAMIC_SPAD_NUM_REQUESTED_REF_SPAD, 0x2C).is_err() {
        println!("Failed to set default SPAD count");
    }
    if write_register(&mut i2c, REG_DYNAMIC_SPAD_REF_EN_START_OFFSET, 0x00).is_err() {
        println!("Failed to set default SPAD type");
    }

    // Configure signal rate limit (adjustable)
    let signal_rate_limit: u16 = 0x0100; // Default is 0.25 MCPS (0x0100)
    if write_register(
        &mut i2c,
        REG_FINAL_RANGE_CONFIG_MIN_COUNT_RATE_RTN_LIMIT,
        (signal_rate_limit >> 8) as u8,
    )
    .is_err()
    {
        println!("Failed to set signal rate limit (high byte)");
    }
    if write_register(
        &mut i2c,
        REG_FINAL_RANGE_CONFIG_MIN_COUNT_RATE_RTN_LIMIT + 1,
        (signal_rate_limit & 0xFF) as u8,
    )
    .is_err()
    {
        println!("Failed to set signal rate limit (low byte)");
    }

    // Set default timing budget (adjustable)
    // Set measurement timing budget to 33 ms (default)
    if set_measurement_timing_budget(&mut i2c, 33000).is_err() {
        println!("Failed to set measurement timing budget");
    }

    // Configure system sequence steps
    if write_register(&mut i2c, REG_SYSTEM_SEQUENCE_CONFIG, 0xFF).is_err() {
        println!("Failed to configure system sequence steps");
    }

    // Configure interrupt - New sample ready/data ready on GPIO
    if write_register(&mut i2c, REG_SYSTEM_INTERRUPT_CONFIG_GPIO, 0x04).is_err() {
        println!("Failed to configure interrupt");
    }

    // Clear any pending interrupts
    if write_register(&mut i2c, REG_SYSTEM_INTERRUPT_CLEAR, 0x01).is_err() {
        println!("Failed to clear interrupts");
    }

    println!("VL53L0X sensor initialized!");

    // Wait a bit after initialization before first measurement
    delay.delay_millis(200);

    loop {
        // Start a single range measurement
        if write_register(&mut i2c, REG_SYSRANGE_START, 0x01).is_err() {
            println!("Failed to start measurement");
            delay.delay_millis(100);
            continue;
        }

        // Wait for measurement to complete
        let mut measurement_complete = false;
        let mut timeout_counter = 0;
        while !measurement_complete && timeout_counter < 1000 {
            // Increased timeout
            let mut status_buffer = [0u8];
            if i2c
                .write_read(
                    VL53L0X_ADDR,
                    &[REG_RESULT_INTERRUPT_STATUS],
                    &mut status_buffer,
                )
                .is_err()
            {
                println!("Failed to read interrupt status");
                break;
            }

            // Check bit 0 of the interrupt status register (new data ready)
            if (status_buffer[0] & 0x07) != 0 {
                measurement_complete = true;
            } else {
                delay.delay_millis(10);
                timeout_counter += 1;
            }
        }

        if !measurement_complete {
            println!("Measurement timeout after {} ms", timeout_counter * 10);
            // Try to recover by clearing interrupts
            if write_register(&mut i2c, REG_SYSTEM_INTERRUPT_CLEAR, 0x01).is_err() {
                println!("Failed to clear interrupts");
            }
            delay.delay_millis(500);
            continue;
        }

        // Read range status to validate measurement
        let mut status_buffer = [0u8];
        if i2c
            .write_read(
                VL53L0X_ADDR,
                &[REG_RESULT_RANGE_STATUS + 10],
                &mut status_buffer,
            )
            .is_err()
        {
            println!("Failed to read range status");
            delay.delay_millis(500);
            continue;
        }

        // Read measurement from register 0x14 + 10 (decimal index offset - see datasheet)
        let mut range_buffer = [0u8; 2];
        if i2c
            .write_read(
                VL53L0X_ADDR,
                &[REG_RESULT_RANGE_STATUS + 10],
                &mut range_buffer,
            )
            .is_err()
        {
            println!("Failed to read range data");
            delay.delay_millis(500);
            continue;
        }

        // Convert to distance in mm (little endian)
        let distance = ((range_buffer[1] as u16) << 8) | range_buffer[0] as u16;

        // Clear interrupt
        if write_register(&mut i2c, REG_SYSTEM_INTERRUPT_CLEAR, 0x01).is_err() {
            println!("Failed to clear interrupt");
        }

        // Get range status from bits 4:0 of the status register
        let mut range_status_reg = [0u8];
        if i2c
            .write_read(
                VL53L0X_ADDR,
                &[REG_RESULT_RANGE_STATUS],
                &mut range_status_reg,
            )
            .is_err()
        {
            println!("Failed to read range status register");
            delay.delay_millis(500);
            continue;
        }
        let range_status = (range_status_reg[0] >> 4) & 0x0F;

        // Status meanings from datasheet:
        // 0: No error
        // 1: VCSEL continuity test failed
        // 2: VCSEL watchdog test failed
        // 3: VCSEL watchdog test failed
        // 4: Phase calibration failed
        // 5: Reference calibration failed
        // 6: Signal failed
        // 7: Phase failed
        // 8: Hardware failed
        // 9: Reference phase error
        // 10: Target is too far (> 400mm, typically)
        // 11: Laser safety
        // 12: Signal not enough
        // 13: Range is too near (< 30mm, typically)
        // 14: Reference beam detected
        if range_status == 0 {
            println!("Distance: {} mm", distance);
        } else {
            println!(
                "Range status error: {}, Raw distance: {} mm",
                range_status, distance
            );
        }

        // Wait between readings
        delay.delay_millis(500);
    }
}

// Helper function to write a register
fn write_register(i2c: &mut I2c<'_, Blocking>, reg: u8, value: u8) -> Result<(), &'static str> {
    match i2c.write(VL53L0X_ADDR, &[reg, value]) {
        Ok(_) => Ok(()),
        Err(_) => Err("I2C write failed"),
    }
}

// Helper function to read a register
fn read_register(i2c: &mut I2c<'_, Blocking>, reg: u8) -> Result<u8, &'static str> {
    let mut buffer = [0u8];
    match i2c.write_read(VL53L0X_ADDR, &[reg], &mut buffer) {
        Ok(_) => Ok(buffer[0]),
        Err(_) => Err("I2C read failed"),
    }
}

// Helper function to set measurement timing budget
fn set_measurement_timing_budget(
    i2c: &mut I2c<'_, Blocking>,
    budget_us: u32,
) -> Result<(), &'static str> {
    if budget_us < 20000 {
        return Err("Budget too low (min 20ms)");
    }

    // Configure timing params based on budget - here we use a simplified approach
    // Divide budget into pre-range and final range phases
    let pre_range_us = budget_us / 3;
    let final_range_us = budget_us - pre_range_us;

    // Set pre-range timeout (register 0x51 and 0x52)
    let pre_range_vcsel_period_pclks = 18; // From init above
    let pre_range_mclks = (pre_range_us * 1000) / (pre_range_vcsel_period_pclks * 2);
    let pre_range_encoded = encode_timeout(pre_range_mclks as u16);

    write_register(i2c, 0x51, (pre_range_encoded >> 8) as u8)?;
    write_register(i2c, 0x52, (pre_range_encoded & 0xFF) as u8)?;

    // Set final range timeout (register 0x71 and 0x72)
    let final_range_vcsel_period_pclks = 14; // From init above
    let final_range_mclks = (final_range_us * 1000) / (final_range_vcsel_period_pclks * 2);
    let final_range_encoded = encode_timeout(final_range_mclks as u16);

    write_register(i2c, 0x71, (final_range_encoded >> 8) as u8)?;
    write_register(i2c, 0x72, (final_range_encoded & 0xFF) as u8)?;

    Ok(())
}

// Helper function to encode timeout value
fn encode_timeout(timeout_mclks: u16) -> u16 {
    // Encode timeout from macro-clock cycles to register value
    if timeout_mclks <= 0 {
        return 0;
    }

    let mut ls_byte = 0;
    let mut ms_byte = 0;

    if timeout_mclks > 0 {
        ls_byte = timeout_mclks - 1;

        while (ls_byte as u32 & 0xFFFFFF00) > 0 {
            ls_byte >>= 1;
            ms_byte += 1;
        }

        return (ms_byte << 8) | (ls_byte & 0xFF);
    }

    0
}
