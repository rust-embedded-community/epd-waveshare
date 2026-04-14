//! Ferris walking demo for the Waveshare 2.13" E-Paper HAT V4 (SSD1680).
//!
//! Demonstrates partial refresh by scuttling Ferris across the screen
//! three times. The sequence is:
//!
//! 1. Full refresh — clear the panel white.
//! 2. [`Epd2in13::display_part_base_image`] — establish the base image
//!    in both SSD1680 RAM banks. This is required before any partial
//!    refresh will produce correct output.
//! 3. Partial refresh loop — on each step, draw a white rectangle over
//!    Ferris' previous position, draw him at the new position, then
//!    call [`Epd2in13::display_partial`] which runs the soft reset +
//!    partial waveform sequence.
//! 4. Final full refresh — clear the panel white.
//! 5. [`Epd2in13::sleep`] — enter deep sleep.
//!
//! # GPIO backend
//!
//! Uses `gpio_cdev` rather than `sysfs_gpio`: sysfs is deprecated on
//! Raspberry Pi OS Bookworm and the Pi 5 requires `gpio_cdev` due to
//! BCM offset changes on `gpiochip0`. The rest of the upstream examples
//! still use `sysfs_gpio`; this example targets newer RPi OS releases.
//!
//! # Wiring (Waveshare 2.13" V4 HAT, BCM numbering)
//!
//! | Signal | BCM |
//! |--------|-----|
//! | PWR    | 18  |
//! | RST    | 17  |
//! | DC     | 25  |
//! | BUSY   | 24  |
//! | SPI    | CE0 on /dev/spidev0.0, 4 MHz, mode 0 |

use embedded_graphics::{
    image::{Image, ImageRaw},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use embedded_hal::delay::DelayNs;
use epd_waveshare::{
    epd2in13_v4::{Display2in13, Epd2in13},
    graphics::DisplayRotation,
    prelude::*,
};
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{self, SpidevOptions},
    CdevPin, Delay, SpidevDevice,
};

const FERRIS_W: u32 = 110;
const FERRIS_H: u32 = 73;
const FERRIS_BYTES: &[u8] = include_bytes!("./assets/ferris_110x73.raw");

/// Pixels advanced per animation frame.
const STEP: i32 = 5;
/// Horizontal walking room: 250 - 110 = 140.
const X_MAX: i32 = 250 - FERRIS_W as i32;
/// Y position: 3 px from the bottom of the 122 px landscape canvas.
const FERRIS_Y: i32 = 122 - FERRIS_H as i32 - 3;
/// Delay between partial-refresh steps.
const STEP_DELAY_MS: u32 = 150;
/// Number of full back-and-forth walk cycles.
const WALK_CYCLES: u32 = 3;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- SPI setup ---------------------------------------------------------
    let mut spi =
        SpidevDevice::open("/dev/spidev0.0").map_err(|e| format!("open /dev/spidev0.0: {e}"))?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(4_000_000)
        .mode(spidev::SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)
        .map_err(|e| format!("configure /dev/spidev0.0: {e}"))?;

    // --- GPIO setup (gpio_cdev) --------------------------------------------
    let mut chip = Chip::new("/dev/gpiochip0").map_err(|e| format!("open /dev/gpiochip0: {e}"))?;
    let busy = CdevPin::new(
        chip.get_line(24)
            .map_err(|e| format!("claim GPIO24 (BUSY): {e}"))?
            .request(LineRequestFlags::INPUT, 0, "epd-busy")
            .map_err(|e| format!("request GPIO24 (BUSY) as input: {e}"))?,
    )?;
    let dc = CdevPin::new(
        chip.get_line(25)
            .map_err(|e| format!("claim GPIO25 (DC): {e}"))?
            .request(LineRequestFlags::OUTPUT, 0, "epd-dc")
            .map_err(|e| format!("request GPIO25 (DC) as output: {e}"))?,
    )?;
    let rst = CdevPin::new(
        chip.get_line(17)
            .map_err(|e| format!("claim GPIO17 (RST): {e}"))?
            .request(LineRequestFlags::OUTPUT, 1, "epd-rst")
            .map_err(|e| format!("request GPIO17 (RST) as output: {e}"))?,
    )?;
    let pwr = CdevPin::new(
        chip.get_line(18)
            .map_err(|e| format!("claim GPIO18 (PWR): {e}"))?
            .request(LineRequestFlags::OUTPUT, 0, "epd-pwr")
            .map_err(|e| format!("request GPIO18 (PWR) as output: {e}"))?,
    )?;

    let mut delay = Delay;

    // --- EPD init ----------------------------------------------------------
    let mut epd = Epd2in13::new_with_pwr(&mut spi, busy, dc, rst, &mut delay, None, pwr)
        .map_err(|e| format!("EPD init (SSD1680): {e}"))?;

    let mut display = Display2in13::default();
    display.set_rotation(DisplayRotation::Rotate90);

    // 1. Full refresh — start from a clean white panel.
    println!("Clearing panel (full refresh)...");
    display.clear(Color::White).ok();
    epd.update_frame(&mut spi, display.buffer(), &mut delay)?;
    epd.display_frame(&mut spi, &mut delay)?;
    // SSD1680: let the panel settle before the next RAM write. The busy
    // pin drops before the full waveform has fully completed internally.
    delay.delay_ms(500);

    // 2. Establish the partial-refresh base image in both RAM banks.
    //    SSD1680 requires both the "old" and "new" RAM banks to hold the
    //    starting image before partial updates will waveform correctly.
    println!("Establishing partial-refresh base image...");
    display.clear(Color::White).ok();
    draw_ferris(&mut display, 0)?;
    epd.display_part_base_image(&mut spi, display.buffer(), &mut delay)?;
    // display_part_base_image runs a full refresh internally; settle
    // before the first partial step.
    delay.delay_ms(500);

    // 3. Walk loop — Ferris scuttles back and forth via partial refresh.
    println!("Walking Ferris ({} cycles)...", WALK_CYCLES);
    let mut prev_x: i32 = 0;
    for _ in 0..WALK_CYCLES {
        // Left → right
        let mut x = 0;
        while x <= X_MAX {
            partial_step(&mut epd, &mut spi, &mut display, &mut delay, prev_x, x)?;
            prev_x = x;
            x += STEP;
            delay.delay_ms(STEP_DELAY_MS);
        }
        // Right → left
        let mut x = X_MAX;
        while x >= 0 {
            partial_step(&mut epd, &mut spi, &mut display, &mut delay, prev_x, x)?;
            prev_x = x;
            x -= STEP;
            delay.delay_ms(STEP_DELAY_MS);
        }
    }

    // 4. Final full refresh — leave the panel clean.
    //    SSD1680 requires a re-init after display_partial before it will
    //    accept full-refresh commands again; skipping this leaves the
    //    final clear silently dropped.
    println!("Clearing panel (final full refresh)...");
    epd.reinit(&mut spi, &mut delay)?;
    display.clear(Color::White).ok();
    epd.update_frame(&mut spi, display.buffer(), &mut delay)?;
    epd.display_frame(&mut spi, &mut delay)?;

    // 5. Deep sleep.
    println!("Sleeping...");
    epd.sleep(&mut spi, &mut delay)?;

    Ok(())
}

/// Erase Ferris at `prev_x`, draw him at `new_x`, push via partial refresh.
fn partial_step<SPI, BUSY, DC, RST, DELAY, PWR>(
    epd: &mut Epd2in13<SPI, BUSY, DC, RST, DELAY, PWR>,
    spi: &mut SPI,
    display: &mut Display2in13,
    delay: &mut DELAY,
    prev_x: i32,
    new_x: i32,
) -> Result<(), SPI::Error>
where
    SPI: embedded_hal::spi::SpiDevice,
    BUSY: embedded_hal::digital::InputPin,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin,
    DELAY: DelayNs,
    PWR: embedded_hal::digital::OutputPin,
{
    // Erase the previous sprite footprint.
    Rectangle::new(Point::new(prev_x, FERRIS_Y), Size::new(FERRIS_W, FERRIS_H))
        .into_styled(PrimitiveStyle::with_fill(Color::White))
        .draw(display)
        .ok();
    // Redraw at the new position (ok to ignore: Display2in13's error is Infallible).
    draw_ferris(display, new_x).ok();
    epd.display_partial(spi, display.buffer(), delay)
}

/// Blit the Ferris sprite at `(x, FERRIS_Y)`.
fn draw_ferris(display: &mut Display2in13, x: i32) -> Result<(), core::convert::Infallible> {
    // The raw asset is 1-bit packed (MSB first). Per src/color.rs the on-wire
    // encoding is 0 = White, 1 = Black, so we load it as BinaryColor (Off/On)
    // and convert at draw time via the Display2in13's DrawTarget impl.
    let raw: ImageRaw<BinaryColor> = ImageRaw::new(FERRIS_BYTES, FERRIS_W);
    let image = Image::new(&raw, Point::new(x, FERRIS_Y));
    image
        .draw(&mut display.color_converted::<BinaryColor>())
        .ok();
    Ok(())
}
