//! Carcinisation demo for the Waveshare 3.52" E-Paper HAT (UC8253).
//!
//! Carcinisation is the convergent evolution of non-crab crustaceans into
//! crab-like forms — it has happened independently at least five times in
//! the decapod lineage. This example is a visual pun on the observation
//! that large software rewrites tend to converge toward Rust: a lobster
//! dissolves frame-by-frame into Ferris the Rustacean, then the screen
//! resolves to the word "carcinisation" with an arrow pointing at the
//! result. The blending is a deterministic pseudo-random cross-dissolve
//! across 8 partial-refresh frames (Quick / DU LUT), bracketed by two
//! full refreshes (GC LUT).
//!
//! # GPIO backend
//!
//! Uses `gpio_cdev` rather than `sysfs_gpio`: sysfs is deprecated on
//! Raspberry Pi OS Bookworm and the Pi 5 requires `gpio_cdev` due to
//! BCM offset changes on `gpiochip0`. The rest of the upstream examples
//! still use `sysfs_gpio`; this example targets newer RPi OS releases.
//!
//! # Wiring (Waveshare 3.52" HAT, BCM numbering)
//!
//! | Signal | BCM |
//! |--------|-----|
//! | RST    | 17  |
//! | DC     | 25  |
//! | BUSY   | 24  |
//! | SPI    | CE0 on /dev/spidev0.0, 10 MHz, mode 0 |

use embedded_graphics::{
    image::{Image, ImageRaw},
    mono_font::{ascii::FONT_10X20, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use embedded_hal::delay::DelayNs;
use epd_waveshare::{
    epd3in52::{Display3in52, Epd3in52},
    graphics::DisplayRotation,
    prelude::*,
};
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{self, SpidevOptions},
    CdevPin, Delay, SpidevDevice,
};

// --- Sprite geometry --------------------------------------------------------
const SPRITE_W: u32 = 96;
const SPRITE_H: u32 = 96;
/// Horizontal position of the sprite on the 360-wide landscape canvas.
/// `(360 - 96) / 2 = 132`.
const SPRITE_X: i32 = 132;
/// Vertical position during the transformation sequence (vertically centred).
/// `(240 - 96) / 2 = 72`.
const SPRITE_Y_ANIM: i32 = 72;
/// Vertical position in the final frame — same as the animation position
/// so the sprite does not jump between phases. Caption text sits above
/// and below the sprite.
const SPRITE_Y_FINAL: i32 = 72;

// --- Timing ----------------------------------------------------------------
/// Number of cross-dissolve frames.
const FRAMES: u32 = 8;
/// Inter-frame delay during the cross-dissolve.
const FRAME_DELAY_MS: u32 = 400;
/// How long to hold the initial lobster before the dissolve starts.
const SHOW_DELAY_MS: u32 = 2000;

// --- Asset bytes -----------------------------------------------------------
const LOBSTER: &[u8] = include_bytes!("./assets/lobster_96x96.raw");
const FERRIS: &[u8] = include_bytes!("./assets/ferris_96x96.raw");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- SPI setup ---------------------------------------------------------
    let mut spi =
        SpidevDevice::open("/dev/spidev0.0").map_err(|e| format!("open /dev/spidev0.0: {e}"))?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(10_000_000)
        .mode(spidev::SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)
        .map_err(|e| format!("configure /dev/spidev0.0: {e}"))?;

    // --- GPIO setup (gpio_cdev) --------------------------------------------
    let mut chip = Chip::new("/dev/gpiochip0").map_err(|e| format!("open /dev/gpiochip0: {e}"))?;
    let busy = CdevPin::new(
        chip.get_line(24)
            .map_err(|e| format!("claim GPIO24 (BUSY): {e}"))?
            .request(LineRequestFlags::INPUT, 0, "epd3in52-busy")
            .map_err(|e| format!("request GPIO24 (BUSY) as input: {e}"))?,
    )?;
    let dc = CdevPin::new(
        chip.get_line(25)
            .map_err(|e| format!("claim GPIO25 (DC): {e}"))?
            .request(LineRequestFlags::OUTPUT, 0, "epd3in52-dc")
            .map_err(|e| format!("request GPIO25 (DC) as output: {e}"))?,
    )?;
    let rst = CdevPin::new(
        chip.get_line(17)
            .map_err(|e| format!("claim GPIO17 (RST): {e}"))?
            .request(LineRequestFlags::OUTPUT, 1, "epd3in52-rst")
            .map_err(|e| format!("request GPIO17 (RST) as output: {e}"))?,
    )?;

    let mut delay = Delay;

    // --- EPD init ----------------------------------------------------------
    let mut epd = Epd3in52::new(&mut spi, busy, dc, rst, &mut delay, None)
        .map_err(|e| format!("EPD init (UC8253): {e}"))?;

    // ------------------------------------------------------------------
    // Phase 1 — show the lobster with a full refresh (GC LUT).
    // ------------------------------------------------------------------
    println!("Phase 1: showing lobster (full refresh)...");
    epd.set_lut(&mut spi, &mut delay, Some(RefreshLut::Full))?;

    let lobster_frame = render_sprite_frame(LOBSTER, SPRITE_Y_ANIM);
    epd.update_frame(&mut spi, &lobster_frame, &mut delay)?;
    epd.display_frame(&mut spi, &mut delay)?;
    delay.delay_ms(SHOW_DELAY_MS);

    // ------------------------------------------------------------------
    // Phase 2 — cross-dissolve lobster → Ferris with the Quick (DU) LUT.
    // Each frame is a fresh full-buffer render with a blended sprite.
    // ------------------------------------------------------------------
    println!("Phase 2: carcinisation ({FRAMES} frames, Quick LUT)...");
    epd.set_lut(&mut spi, &mut delay, Some(RefreshLut::Quick))?;

    for frame in 0..FRAMES {
        let buf = blend_sprites(LOBSTER, FERRIS, frame);
        epd.update_frame(&mut spi, &buf, &mut delay)?;
        epd.display_frame(&mut spi, &mut delay)?;
        delay.delay_ms(FRAME_DELAY_MS);
    }

    // ------------------------------------------------------------------
    // Phase 3 — final full refresh: Ferris + caption, then sleep.
    // ------------------------------------------------------------------
    println!("Phase 3: final frame + caption (full refresh)...");
    epd.set_lut(&mut spi, &mut delay, Some(RefreshLut::Full))?;

    let final_frame = render_final_frame();
    epd.update_frame(&mut spi, &final_frame, &mut delay)?;
    epd.display_frame(&mut spi, &mut delay)?;

    println!("Sleeping...");
    epd.sleep(&mut spi, &mut delay)?;

    Ok(())
}

/// Draw "Carcinisation" at the top of the canvas, centred on x=180.
/// Used by every phase so the caption is present from the very first frame.
fn draw_top_caption(display: &mut Display3in52) {
    let character_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();
    let centred = TextStyleBuilder::new()
        .baseline(Baseline::Top)
        .alignment(Alignment::Center)
        .build();
    Text::with_text_style(
        "Carcinisation",
        Point::new(180, 8),
        character_style,
        centred,
    )
    .draw(display)
    .ok();
}

/// Build a full 360×240 landscape display buffer with a 96×96 sprite pasted
/// at `(SPRITE_X, sprite_y)` on a white background, plus the "Carcinisation"
/// caption at the top. The sprite bytes are a 1-bit-packed (MSB-first) 96×96
/// image where 0 = white and 1 = black.
fn render_sprite_frame(sprite: &[u8], sprite_y: i32) -> Vec<u8> {
    let mut display = Display3in52::default();
    display.set_rotation(DisplayRotation::Rotate90);
    display.clear(Color::White).ok();

    let raw: ImageRaw<BinaryColor> = ImageRaw::new(sprite, SPRITE_W);
    let image = Image::new(&raw, Point::new(SPRITE_X, sprite_y));
    image
        .draw(&mut display.color_converted::<BinaryColor>())
        .ok();

    draw_top_caption(&mut display);

    display.buffer().to_vec()
}

/// Deterministic pseudo-random cross-dissolve between two 96×96 sprites.
///
/// Produces a full 360×240 display buffer with the blended sprite pasted
/// at `(SPRITE_X, SPRITE_Y_ANIM)` on a white background.
///
/// # Blending rule
///
/// For each pixel `(x, y)` in the 96×96 sprite area:
///
/// - `threshold = frame / (FRAMES - 1)` — 0.0 at frame 0, 1.0 at frame 7.
/// - A deterministic "random" value is derived from the pixel position and
///   the frame index: `(x * 31 + y * 17 + frame * 7) % 100`.
/// - If the random value is below `threshold * 100`, the pixel takes the
///   Ferris value; otherwise it takes the lobster value.
///
/// This is a monotonic dissolve: at frame 0 every pixel is lobster, at
/// frame 7 every pixel is Ferris, and the fraction of ferris pixels grows
/// smoothly with the frame index.
fn blend_sprites(lobster: &[u8], ferris: &[u8], frame: u32) -> Vec<u8> {
    let threshold = frame as f32 / (FRAMES - 1) as f32;
    let flip_threshold = (threshold * 100.0) as u32;

    // 96 × 96 / 8 = 1152 bytes, MSB-packed rows.
    let row_stride = (SPRITE_W / 8) as usize;
    let mut blended = vec![0xFFu8; row_stride * SPRITE_H as usize];

    for y in 0..SPRITE_H as usize {
        for x in 0..SPRITE_W as usize {
            let byte_idx = y * row_stride + (x / 8);
            let bit_idx = 7 - (x % 8);
            let mask = 1u8 << bit_idx;

            let lobster_bit = (lobster[byte_idx] & mask) != 0;
            let ferris_bit = (ferris[byte_idx] & mask) != 0;

            // Deterministic pseudo-random in [0, 100).
            let rnd = ((x as u32) * 31 + (y as u32) * 17 + frame * 7) % 100;
            let pixel_black = if rnd < flip_threshold {
                ferris_bit
            } else {
                lobster_bit
            };

            if pixel_black {
                blended[byte_idx] |= mask;
            } else {
                blended[byte_idx] &= !mask;
            }
        }
    }

    // Paint the blended sprite into a full display buffer with the
    // "Carcinisation" caption at the top.
    let mut display = Display3in52::default();
    display.set_rotation(DisplayRotation::Rotate90);
    display.clear(Color::White).ok();

    let raw: ImageRaw<BinaryColor> = ImageRaw::new(&blended, SPRITE_W);
    let image = Image::new(&raw, Point::new(SPRITE_X, SPRITE_Y_ANIM));
    image
        .draw(&mut display.color_converted::<BinaryColor>())
        .ok();

    draw_top_caption(&mut display);

    display.buffer().to_vec()
}

/// Compose the final frame: "Carcinisation" at the top (present since
/// the first animation frame), Ferris centred, and "Rustacean" below —
/// which only appears here, as the punch line.
fn render_final_frame() -> Vec<u8> {
    let mut display = Display3in52::default();
    display.set_rotation(DisplayRotation::Rotate90);
    display.clear(Color::White).ok();

    // Ferris, vertically centred on the 240 px canvas (y=72..168).
    let raw: ImageRaw<BinaryColor> = ImageRaw::new(FERRIS, SPRITE_W);
    let image = Image::new(&raw, Point::new(SPRITE_X, SPRITE_Y_FINAL));
    image
        .draw(&mut display.color_converted::<BinaryColor>())
        .ok();

    // "Carcinisation" (top) reused across all phases.
    draw_top_caption(&mut display);

    // "Rustacean" — punch line, only drawn on the final frame, at y=184
    // (sprite ends at y=168, gap of 16 px, caption occupies y=184..204).
    let character_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();
    let centred = TextStyleBuilder::new()
        .baseline(Baseline::Top)
        .alignment(Alignment::Center)
        .build();
    Text::with_text_style("Rustacean", Point::new(180, 184), character_style, centred)
        .draw(&mut display)
        .ok();

    display.buffer().to_vec()
}
