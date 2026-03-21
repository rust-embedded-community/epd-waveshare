use std::error::Error;

// This example tests rotations and draws analog clock, tests default fonts of embedded-graphics crate, displays an image of Ferris from examples/assets/ directory and showcases partial updating with a digital clock.
use embedded_graphics::{
    image::{Image, ImageRaw},
    mono_font::{ascii::*, MonoTextStyleBuilder},
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyleBuilder},
    text::{Baseline, Text, TextStyleBuilder},
};
use embedded_hal::delay::DelayNs;
#[cfg(feature = "graphics")]
use epd_waveshare::{color::Color, epd7in5_v2::*, graphics::DisplayRotation, prelude::*};
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{SpiModeFlags, SpidevOptions},
    CdevPin, Delay, SpidevDevice,
};

// GPIO pin definitions (BCM numbering - no offset needed for cdev)
const EPD_RST_PIN: u32 = 17;
const EPD_DC_PIN: u32 = 25;
const EPD_BUSY_PIN: u32 = 24;
const EPD_PWR_PIN: u32 = 18;

fn main() -> Result<(), Box<dyn Error>> {
    // Set up the device
    // Open the GPIO chip (usually gpiochip0 on Raspberry Pi)
    let mut chip = Chip::new("/dev/gpiochip0")?;

    // Get GPIO lines and configure them
    let rst_line = chip.get_line(EPD_RST_PIN)?;
    let rst_handle = rst_line.request(LineRequestFlags::OUTPUT, 0, "epd-rst")?;
    let rst_pin = CdevPin::new(rst_handle)?;

    let dc_line = chip.get_line(EPD_DC_PIN)?;
    let dc_handle = dc_line.request(LineRequestFlags::OUTPUT, 0, "epd-dc")?;
    let dc_pin = CdevPin::new(dc_handle)?;

    let busy_line = chip.get_line(EPD_BUSY_PIN)?;
    let busy_handle = busy_line.request(LineRequestFlags::INPUT, 0, "epd-busy")?;
    let busy_pin = CdevPin::new(busy_handle)?;

    let pwr_line = chip.get_line(EPD_PWR_PIN)?;
    let _ = pwr_line.request(LineRequestFlags::OUTPUT, 1, "epd-pwr")?;

    // Initialize SPI
    let mut spi = SpidevDevice::open("/dev/spidev0.0")?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(10_000_000)
        .mode(SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)?;

    let mut delay = Delay {};

    let mut epd7in5 =
        Epd7in5::new(&mut spi, busy_pin, dc_pin, rst_pin, &mut delay, None).expect("epd new");
    epd7in5.set_lut(&mut spi, &mut delay, Some(RefreshLut::Quick))?;
    let mut display = Display7in5::default();
    display.clear(Color::White);
    println!("Device successfully initialized!");

    // Test graphics display

    println!("Test all the rotations");

    display.set_rotation(DisplayRotation::Rotate0);
    draw_text(&mut display, "Rotate 0!", 5, 50);

    display.set_rotation(DisplayRotation::Rotate90);
    draw_text(&mut display, "Rotate 90!", 5, 50);

    display.set_rotation(DisplayRotation::Rotate180);
    draw_text(&mut display, "Rotate 180!", 5, 50);

    display.set_rotation(DisplayRotation::Rotate270);
    draw_text(&mut display, "Rotate 270!", 5, 50);

    epd7in5.update_and_display_frame(&mut spi, display.buffer(), &mut delay)?;
    delay.delay_ms(5000);

    // Draw an analog clock
    println!("Draw a clock");
    display.clear(Color::Black).ok();
    let style = PrimitiveStyleBuilder::new()
        .stroke_color(Color::White)
        .stroke_width(1)
        .build();

    let _ = Circle::with_center(Point::new(64, 64), 80)
        .into_styled(style)
        .draw(&mut display);
    let _ = Line::new(Point::new(64, 64), Point::new(0, 64))
        .into_styled(style)
        .draw(&mut display);
    let _ = Line::new(Point::new(64, 64), Point::new(80, 80))
        .into_styled(style)
        .draw(&mut display);
    epd7in5.update_and_display_frame(&mut spi, display.buffer(), &mut delay)?;
    delay.delay_ms(5000);

    // Draw some text
    println!("Print text in all sizes");
    // Color is inverted - black means white, white means black; the output will be black text on white background
    display.clear(Color::White).ok();
    let fonts = [
        &FONT_4X6,
        &FONT_5X7,
        &FONT_5X8,
        &FONT_6X9,
        &FONT_6X10,
        &FONT_6X12,
        &FONT_6X13,
        &FONT_6X13_BOLD,
        &FONT_6X13_ITALIC,
        &FONT_7X13,
        &FONT_7X13_BOLD,
        &FONT_7X13_ITALIC,
        &FONT_7X14,
        &FONT_7X14_BOLD,
        &FONT_8X13,
        &FONT_8X13_BOLD,
        &FONT_8X13_ITALIC,
        &FONT_9X15,
        &FONT_9X15_BOLD,
        &FONT_9X18,
        &FONT_9X18_BOLD,
        &FONT_10X20,
    ];
    for (n, font) in fonts.iter().enumerate() {
        let style = MonoTextStyleBuilder::new()
            .font(font)
            .text_color(Color::Black)
            .background_color(Color::White)
            .build();
        let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();
        let y = 10 + n * 30;
        let _ = Text::with_text_style(
            "Rust is awesome!",
            Point::new(20, y.try_into().unwrap()),
            style,
            text_style,
        )
        .draw(&mut display);
    }
    epd7in5.update_and_display_frame(&mut spi, display.buffer(), &mut delay)?;
    delay.delay_ms(5000);

    // Draw an image
    println!("Draw Ferris");
    display.clear(Color::White).ok();
    let data = include_bytes!("./assets/ferris.raw");
    let raw_image = ImageRaw::<Color>::new(data, 460);
    let image = Image::new(&raw_image, Point::zero());
    image.draw(&mut display).unwrap();
    epd7in5.update_and_display_frame(&mut spi, display.buffer(), &mut delay)?;

    delay.delay_ms(5000);

    println!("Clock Demo (partial update)");
    epd7in5.clear_frame(&mut spi, &mut delay)?;

    epd7in5.set_lut(&mut spi, &mut delay, Some(RefreshLut::PartialRefresh))?;

    // Clock parameters - using FONT_6X10 (6 pixels wide, 10 pixels tall)
    // "HH:MM:SS" = 8 characters
    let char_width = 6;
    let char_height = 10;
    let clock_string_length = 8; // "HH:MM:SS"

    // Create a buffer for the entire clock region (with padding)
    let partial_frame_x = 299;
    let partial_frame_y = 200;
    let clock_buffer_width = (char_width * clock_string_length) as u32 + 1;
    let clock_buffer_height = char_height as u32;

    let mut partial_frame_buffer = vec![
        0u8;
        Display7in5::partial_frame_buffer_size(
            partial_frame_x,
            clock_buffer_width,
            clock_buffer_height,
        )
    ];
    let mut partial_display = display.get_partial_frame(
        &mut partial_frame_buffer,
        partial_frame_x,
        partial_frame_y,
        clock_buffer_width,
        clock_buffer_height,
    );

    // Time variables
    let mut hours = 12u8;
    let mut minutes = 34u8;
    let mut seconds = 56u8;

    println!("Updating clock every second for 10 iterations...");

    for iteration in 0..10 {
        // Format current time
        let current_time = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);

        // Clear the clock buffer (white background)
        partial_display.clear(Color::White).unwrap();

        // Draw the entire time string on the clock buffer
        draw_text(&mut partial_display, &current_time, 1, 0);

        let params = partial_display.get_update_parameters();

        epd7in5
            .update_partial_frame(
                &mut spi,
                &mut delay,
                params.buffer,
                params.x,
                params.y,
                params.width,
                params.height,
            )
            .unwrap();

        epd7in5.display_frame(&mut spi, &mut delay)?;

        // Increment time
        seconds += 1;
        if seconds >= 60 {
            seconds = 0;
            minutes += 1;
            if minutes >= 60 {
                minutes = 0;
                hours += 1;
                if hours >= 24 {
                    hours = 0;
                }
            }
        }

        println!("[{}] Time: {}", iteration, current_time);
        delay.delay_ms(1000);
    }

    // Clear and sleep
    println!("Clear the display");
    epd7in5.set_lut(&mut spi, &mut delay, Some(RefreshLut::Full))?;
    epd7in5.clear_frame(&mut spi, &mut delay)?;
    println!("Finished tests - going to sleep");
    epd7in5.sleep(&mut spi, &mut delay)?;
    Ok(())
}

fn draw_text<D: DrawTarget<Color = Color>>(display: &mut D, text: &str, x: i32, y: i32) {
    let style = MonoTextStyleBuilder::new()
        .font(&embedded_graphics::mono_font::ascii::FONT_6X10)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();

    let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();

    let _ = Text::with_text_style(text, Point::new(x, y), style, text_style).draw(display);
}
