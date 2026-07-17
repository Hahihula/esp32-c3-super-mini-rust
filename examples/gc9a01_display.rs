//! GC9A01 1.28-inch Round Display (240x240) Example for ESP32-C3
//!
//! ## Wiring (typical GC9A01 module -> ESP32-C3):
//! | Display | ESP32-C3 | Function     |
//! |-----------|----------|--------------|
//! | VCC       | 3.3V     | Power        |
//! | GND       | GND      | Ground       |
//! | SCL       | GPIO6    | SPI SCK      |
//! | SDA       | GPIO7    | SPI MOSI     |
//! | RES       | GPIO10   | Reset        |
//! | DC        | GPIO2    | Data/Command |
//! | CS        | GPIO3    | Chip Select  |
//! | BLK       | GPIO4    | Backlight    |

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::master::{Config as SpiConfig, Spi},
    spi::Mode as SpiMode,
    time::Rate,
};
use esp_println::println;

use gc9a01::prelude::*;
use gc9a01::mode::BufferedGraphics;
use gc9a01::Gc9a01;

// SPI display interface bridge (embedded-hal 1.0 compatible)
use display_interface_spi::SPIInterface;
use embedded_hal_bus::spi::ExclusiveDevice;

// 2D graphics library
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::Text,
};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut delay = Delay::new();

    println!("GC9A01 Display Example Starting...");

    // --- GPIO Setup ---
    let mut backlight = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    let _rst = Output::new(peripherals.GPIO10, Level::High, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let cs = Output::new(peripherals.GPIO3, Level::High, OutputConfig::default());

    // --- SPI Setup (esp-hal 1.1.1 API) ---
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(SpiMode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO6)
    .with_mosi(peripherals.GPIO7);

    // Wrap SPI + CS into ExclusiveDevice (embedded-hal 1.0 SpiDevice trait)
    let spi_dev = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

    // Create display interface: SPI device + DC pin
    let interface = SPIInterface::new(spi_dev, dc);

    // --- Display Driver Initialization ---
    let mut display = Gc9a01::new(
        interface,
        DisplayResolution240x240,
        DisplayRotation::Rotate0,
    )
    .into_buffered_graphics();

    // Send init sequence, clear screen
    display.init(&mut delay).unwrap();

    // Turn on backlight
    backlight.set_high();

    println!("Display initialized!");

    // --- Main Loop ---
    let mut frame: u32 = 0;

    loop {
        // Clear the framebuffer (RAM buffer, not hardware)
        display.clear();

        match frame % 4 {
            0 => draw_circles(&mut display),
            1 => draw_crosshairs(&mut display),
            2 => draw_text(&mut display),
            _ => draw_rectangles(&mut display),
        }

        // Flush RAM framebuffer to display hardware via SPI
        display.flush().unwrap();

        println!("Frame {}", frame);
        frame = frame.wrapping_add(1);
        delay.delay_millis(800);
    }
}

fn draw_circles(display: &mut impl DrawTarget<Color = Rgb565>) {
    Circle::new(Point::new(20, 20), 200)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 4))
        .draw(display)
        .ok();

    Circle::new(Point::new(70, 70), 100)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::MAGENTA))
        .draw(display)
        .ok();

    Circle::new(Point::new(115, 115), 10)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
        .draw(display)
        .ok();
}

fn draw_crosshairs(display: &mut impl DrawTarget<Color = Rgb565>) {
    let style = PrimitiveStyle::with_stroke(Rgb565::WHITE, 2);

    Rectangle::new(Point::new(120, 20), Size::new(1, 200))
        .into_styled(style)
        .draw(display)
        .ok();

    Rectangle::new(Point::new(20, 120), Size::new(200, 1))
        .into_styled(style)
        .draw(display)
        .ok();

    Circle::new(Point::new(100, 100), 40)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::RED, 2))
        .draw(display)
        .ok();
}

fn draw_text(display: &mut impl DrawTarget<Color = Rgb565>) {
    let title_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    Text::new("GC9A01", Point::new(80, 100), title_style)
        .draw(display)
        .ok();

    let res_style = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);
    Text::new("240x240", Point::new(70, 130), res_style)
        .draw(display)
        .ok();

    let round_style = MonoTextStyle::new(&FONT_10X20, Rgb565::YELLOW);
    Text::new("Round LCD", Point::new(55, 160), round_style)
        .draw(display)
        .ok();
}

fn draw_rectangles(display: &mut impl DrawTarget<Color = Rgb565>) {
    let colors = [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE];
    for i in 0..3usize {
        let offset = (i * 20) as i32;
        let color = colors[i % colors.len()];
        Rectangle::new(Point::new(60 + offset, 60 + offset), Size::new(120, 120))
            .into_styled(PrimitiveStyle::with_stroke(color, 3))
            .draw(display)
            .ok();
    }
}