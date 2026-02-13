#![deny(warnings)]

use embedded_graphics::{
    mono_font::MonoTextStyleBuilder,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle},
    text::{Baseline, Text, TextStyleBuilder},
};
use embedded_hal::delay::DelayNs;
use epd_waveshare::{
    color::*,
    epd4in2bc::{Display4in2bc, Epd4in2bc},
    graphics::DisplayRotation,
    prelude::*,
};
use linux_embedded_hal::{
    spidev::{self, SpidevOptions},
    sysfs_gpio::Direction,
    Delay, SPIError, SpidevDevice, SysfsPin,
};

// activate spi, gpio in raspi-config
// needs to be run with sudo because of some sysfs_gpio permission problems and follow-up timing problems
// see https://github.com/rust-embedded/rust-sysfs-gpio/issues/5 and follow-up issues
/**                   BCM   Physical
* 	EPD_CS_PIN      = 26; = 37 (On raspberry Pi put it on CE0 pin 8 physical 24)
   EPD_BUSY_PIN    = 24; = 18
   EPD_DC_PIN      = 25; = 22
    EPD_RST_PIN     = 17;  = 11
*/
fn main() -> Result<(), SPIError> {
    // Configure SPI
    // Settings are taken from
    let mut spi = SpidevDevice::open("/dev/spidev0.0").expect("spidev directory");
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(4_000_000)
        .mode(spidev::SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options).expect("spi configuration");

    // Configure Digital I/O Pin to be used as Chip Select for SPI
    let cs = SysfsPin::new(26); //BCM7 CE0
    cs.export().expect("cs export");
    while !cs.is_exported() {}
    cs.set_direction(Direction::Out).expect("CS Direction");
    cs.set_value(1).expect("CS Value set to 1");

    let busy = SysfsPin::new(24); //pin 29
    busy.export().expect("busy export");
    while !busy.is_exported() {}
    busy.set_direction(Direction::In).expect("busy Direction");
    //busy.set_value(1).expect("busy Value set to 1");

    let dc = SysfsPin::new(25); //pin 31 //bcm6
    dc.export().expect("dc export");
    while !dc.is_exported() {}
    dc.set_direction(Direction::Out).expect("dc Direction");
    dc.set_value(1).expect("dc Value set to 1");

    let rst = SysfsPin::new(17); //pin 36 //bcm16
    rst.export().expect("rst export");
    while !rst.is_exported() {}
    rst.set_direction(Direction::Out).expect("rst Direction");
    rst.set_value(1).expect("rst Value set to 1");

    let mut delay = Delay {};

    let mut epd4in2bc =
        Epd4in2bc::new(&mut spi, busy, dc, rst, &mut delay, None).expect("eink initalize error");

    println!("Test all the rotations");
    let mut display = Display4in2bc::default();
    display.clear(TriColor::White).ok();

    display.set_rotation(DisplayRotation::Rotate0);
    draw_text(&mut display, "Rotation 0!", 5, 50);

    display.set_rotation(DisplayRotation::Rotate90);
    draw_text(&mut display, "Rotation 90!", 5, 50);

    display.set_rotation(DisplayRotation::Rotate180);
    draw_text(&mut display, "Rotation 180!", 5, 50);

    display.set_rotation(DisplayRotation::Rotate270);
    draw_text(&mut display, "Rotation 270!", 5, 50);

    epd4in2bc.update_color_frame(
        &mut spi,
        &mut delay,
        display.bw_buffer(),
        display.chromatic_buffer(),
    )?;
    epd4in2bc
        .display_frame(&mut spi, &mut delay)
        .expect("display frame new graphics");

    delay.delay_ms(5000);

    println!("Now test new graphics with default rotation and three colors:");
    display.clear(TriColor::White).ok();

    // draw a analog clock
    let _ = Circle::with_center(Point::new(64, 64), 80)
        .into_styled(PrimitiveStyle::with_stroke(TriColor::Black, 1))
        .draw(&mut display);
    let _ = Line::new(Point::new(64, 64), Point::new(30, 40))
        .into_styled(PrimitiveStyle::with_stroke(TriColor::Black, 4))
        .draw(&mut display);
    let _ = Line::new(Point::new(64, 64), Point::new(80, 40))
        .into_styled(PrimitiveStyle::with_stroke(TriColor::Black, 1))
        .draw(&mut display);

    // draw text white on Red background by using the chromatic buffer
    let style = MonoTextStyleBuilder::new()
        .font(&embedded_graphics::mono_font::ascii::FONT_6X10)
        .text_color(TriColor::White)
        .background_color(TriColor::Chromatic)
        .build();
    let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();

    let _ = Text::with_text_style("It's working-WoB!", Point::new(90, 10), style, text_style)
        .draw(&mut display);

    // use bigger/different font
    let style = MonoTextStyleBuilder::new()
        .font(&embedded_graphics::mono_font::ascii::FONT_10X20)
        .text_color(TriColor::White)
        .background_color(TriColor::Chromatic)
        .build();

    let _ = Text::with_text_style("It's working\nWoB!", Point::new(90, 40), style, text_style)
        .draw(&mut display);

    // we used three colors, so we need to update both bw-buffer and chromatic-buffer

    epd4in2bc.update_color_frame(
        &mut spi,
        &mut delay,
        display.bw_buffer(),
        display.chromatic_buffer(),
    )?;
    epd4in2bc
        .display_frame(&mut spi, &mut delay)
        .expect("display frame new graphics");

    println!("Second frame done. Waiting 5s");
    delay.delay_ms(5000);

    // clear both bw buffer and chromatic buffer
    display.clear(TriColor::White).ok();
    epd4in2bc.update_color_frame(
        &mut spi,
        &mut delay,
        display.bw_buffer(),
        display.chromatic_buffer(),
    )?;
    epd4in2bc.display_frame(&mut spi, &mut delay)?;

    println!("Finished tests - going to sleep");
    epd4in2bc.sleep(&mut spi, &mut delay)
}

fn draw_text(display: &mut Display4in2bc, text: &str, x: i32, y: i32) {
    let style = MonoTextStyleBuilder::new()
        .font(&embedded_graphics::mono_font::ascii::FONT_6X10)
        .text_color(TriColor::Black)
        .background_color(TriColor::White)
        .build();

    let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();

    let _ = Text::with_text_style(text, Point::new(x, y), style, text_style).draw(display);
}
