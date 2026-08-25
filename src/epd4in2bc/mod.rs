//! A simple Driver for the Waveshare 4.2" (B/C) tri-color E-Ink Display via SPI
//!
//! This covers the original-generation 4.2" 400x300 three-color raw panels, in both the
//! B (black/white/**red**) and C (black/white/**yellow**) pigments. Waveshare ships a single
//! `epd4in2bc` driver for both; there is no separate `epd4in2c`, and the later `epd4in2b_V2`
//! revision is a different controller and is red-only.
//!
//! The panel uses waveforms programmed into OTP, selected by `PanelSetting = 0x0F`. It therefore
//! takes no LUT upload, no power setting, no PLL setting and no resolution setting -- the whole
//! initialisation is three commands.
//!
//! # References
//!
//! - [Product page](https://www.waveshare.com/4.2inch-e-paper-module-c.htm)
//! - [Manual](https://www.waveshare.com/wiki/4.2inch_e-Paper_Module_(C)_Manual)
//! - [Waveshare Python driver](https://github.com/waveshareteam/e-Paper/blob/master/RaspberryPi_JetsonNano/python/lib/waveshare_epd/epd4in2bc.py)
//!
//! # Cautions
//!
//! From the manual, and worth honouring in application code:
//!
//! - A full refresh takes roughly **25 seconds**, and the recommended minimum interval between
//!   refreshes is **180 seconds**.
//! - This panel has **no partial refresh**. [`WaveshareDisplay::update_partial_frame`] is
//!   deliberately unimplemented, and [`crate::traits::QuickRefresh`] is not implemented at all.
//! - **Always call [`WaveshareDisplay::sleep`] once the refresh is done.** Leaving the panel
//!   powered holds it in a high-voltage state, which permanently damages it.
//!
//! # Examples
//!
//!```rust, no_run
//!# use embedded_hal_mock::eh1::*;
//!# fn main() -> Result<(), embedded_hal::spi::ErrorKind> {
//!use embedded_graphics::{
//!    prelude::*, primitives::{Line, PrimitiveStyle},
//!};
//!use epd_waveshare::{epd4in2bc::*, prelude::*};
//!#
//!# let expectations = [];
//!# let mut spi = spi::Mock::new(&expectations);
//!# let expectations = [];
//!# let cs_pin = digital::Mock::new(&expectations);
//!# let busy_in = digital::Mock::new(&expectations);
//!# let dc = digital::Mock::new(&expectations);
//!# let rst = digital::Mock::new(&expectations);
//!# let mut delay = delay::NoopDelay::new();
//!
//!// Setup EPD
//!let mut epd = Epd4in2bc::new(&mut spi, busy_in, dc, rst, &mut delay, None)?;
//!
//!// Use display graphics from embedded-graphics
//!let mut display = Display4in2bc::default();
//!
//!// Draw a chromatic (red or yellow, depending on the panel) line
//!let _ = Line::new(Point::new(0, 120), Point::new(0, 295))
//!    .into_styled(PrimitiveStyle::with_stroke(TriColor::Chromatic, 1))
//!    .draw(&mut display);
//!
//!// Display updated frame
//!epd.update_and_display_frame(&mut spi, display.buffer(), &mut delay)?;
//!
//!// Set the EPD to sleep -- do not skip this
//!epd.sleep(&mut spi, &mut delay)?;
//!# Ok(())
//!# }
//!```

use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::color::TriColor;
use crate::interface::DisplayInterface;
use crate::traits::{
    InternalWiAdditions, RefreshLut, WaveshareDisplay, WaveshareThreeColorDisplay,
};

pub(crate) mod command;
use self::command::Command;

#[cfg(feature = "graphics")]
use crate::buffer_len;

/// Width of the display
pub const WIDTH: u32 = 400;
/// Height of the display
pub const HEIGHT: u32 = 300;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: TriColor = TriColor::White;

const IS_BUSY_LOW: bool = true;
// Bulk SPI writes. The 4.2" b/w driver toggles CS per byte, but this panel does not require it --
// the tri-color `epd2in9b_v4` is bulk too -- and at 30_000 bytes per frame the per-byte transaction
// overhead is worth avoiding.
const SINGLE_BYTE_WRITE: bool = false;
/// Bytes per colour plane. The [`Display4in2bc`] buffer holds two of these back to back.
const NUM_DISPLAY_BITS: u32 = WIDTH / 8 * HEIGHT;

/// Full size buffer for use with the 4in2bc EPD
///
/// `BWRBIT` is `false`, which combined with the chromatic-plane inversion this driver applies on
/// send produces the polarity the panel documents: on both planes a set bit is white, and on the
/// chromatic plane a cleared bit is the chromatic pigment.
#[cfg(feature = "graphics")]
pub type Display4in2bc = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize * 2) },
    TriColor,
>;

/// Epd4in2bc driver
pub struct Epd4in2bc<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: TriColor,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd4in2bc<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Vendor reset: 200ms high, 2ms low, 200ms high.
        self.interface.reset(delay, 200_000, 2_000);

        self.cmd_with_data(spi, Command::BoosterSoftStart, &[0x17, 0x17, 0x17])?;

        self.command(spi, Command::PowerOn)?;
        self.wait_until_idle(spi, delay)?;

        // 0x0F selects the OTP waveform, so no LUT upload follows.
        self.cmd_with_data(spi, Command::PanelSetting, &[0x0F])?;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareThreeColorDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd4in2bc<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn update_color_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        black: &[u8],
        chromatic: &[u8],
    ) -> Result<(), SPI::Error> {
        self.update_achromatic_frame(spi, delay, black)?;
        self.update_chromatic_frame(spi, delay, chromatic)
    }

    /// Update only the black/white data of the display.
    ///
    /// Finish by calling `update_chromatic_frame`.
    fn update_achromatic_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        black: &[u8],
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::DataStartTransmission1, black)?;
        Ok(())
    }

    /// Update only chromatic data of the display.
    ///
    /// This data takes precedence over the black/white data. The bytes are inverted on the way
    /// out, so that a set bit in the caller's buffer means the chromatic pigment -- see
    /// [`Display4in2bc`].
    fn update_chromatic_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        chromatic: &[u8],
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.command(spi, Command::DataStartTransmission2)?;
        self.send_inverted(spi, chromatic)?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd4in2bc<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    type DisplayColor = TriColor;

    fn new(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
    ) -> Result<Self, SPI::Error> {
        let interface = DisplayInterface::new(busy, dc, rst, delay_us);
        let color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = Epd4in2bc { interface, color };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::PowerOff)?;
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::DeepSleep, &[0xA5])?;
        delay.delay_us(2_000_000);
        Ok(())
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn set_background_color(&mut self, color: TriColor) {
        self.color = color;
    }

    fn background_color(&self) -> &TriColor {
        &self.color
    }

    fn width(&self) -> u32 {
        WIDTH
    }

    fn height(&self) -> u32 {
        HEIGHT
    }

    /// Send a full dual-plane buffer, as produced by [`Display4in2bc`].
    ///
    /// The first half is the black/white plane and the second half is the chromatic plane; the
    /// latter is inverted on the way out.
    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        let split = NUM_DISPLAY_BITS as usize;
        debug_assert_eq!(
            buffer.len(),
            2 * split,
            "expected a dual-plane buffer; a single-plane buffer would leave the chromatic plane \
             short and desync the panel"
        );

        self.update_achromatic_frame(spi, delay, &buffer[..split])?;
        self.update_chromatic_frame(spi, delay, &buffer[split..])?;
        Ok(())
    }

    /// Not supported: this panel has no partial refresh mode.
    fn update_partial_frame(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _buffer: &[u8],
        _x: u32,
        _y: u32,
        _width: u32,
        _height: u32,
    ) -> Result<(), SPI::Error> {
        unimplemented!("the 4.2in tri-color panel does not support partial refresh")
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::DisplayRefresh)?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }

    fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.update_frame(spi, buffer, delay)?;
        self.display_frame(spi, delay)?;
        Ok(())
    }

    fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;

        // A set bit is white on both planes, so 0xFF twice clears to white.
        self.command(spi, Command::DataStartTransmission1)?;
        self.interface.data_x_times(spi, 0xFF, NUM_DISPLAY_BITS)?;

        self.command(spi, Command::DataStartTransmission2)?;
        self.interface.data_x_times(spi, 0xFF, NUM_DISPLAY_BITS)?;

        Ok(())
    }

    /// Not supported: the panel runs from its OTP waveform, selected by `PanelSetting = 0x0F`,
    /// and ignores LUTs written to the LUT registers.
    fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        unimplemented!("the 4.2in tri-color panel uses its OTP waveform and has no settable LUT")
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd4in2bc<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn command(&mut self, spi: &mut SPI, command: Command) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, command)
    }

    fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data)
    }

    /// Send `data` with every byte bitwise inverted, without allocating.
    fn send_inverted(&mut self, spi: &mut SPI, data: &[u8]) -> Result<(), SPI::Error> {
        let mut chunk = [0u8; 64];
        for block in data.chunks(chunk.len()) {
            let n = block.len();
            for (dst, src) in chunk[..n].iter_mut().zip(block) {
                *dst = !*src;
            }
            self.interface.data(spi, &chunk[..n])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;
    use std::vec::Vec;

    /// Records every byte handed to the bus, tagged with the DC level at the time, so a test can
    /// assert the exact command/data stream the panel would see.
    #[derive(Default)]
    struct Recorder {
        written: Vec<u8>,
    }

    impl embedded_hal::spi::ErrorType for Recorder {
        type Error = embedded_hal::spi::ErrorKind;
    }

    impl embedded_hal::spi::SpiDevice for Recorder {
        fn transaction(
            &mut self,
            operations: &mut [embedded_hal::spi::Operation<'_, u8>],
        ) -> Result<(), Self::Error> {
            for op in operations {
                if let embedded_hal::spi::Operation::Write(buf) = op {
                    self.written.extend_from_slice(buf);
                }
            }
            Ok(())
        }
    }

    struct FakePin;
    impl embedded_hal::digital::ErrorType for FakePin {
        type Error = core::convert::Infallible;
    }
    impl embedded_hal::digital::OutputPin for FakePin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
        fn set_high(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    /// Never busy, so `wait_until_idle` returns immediately.
    impl embedded_hal::digital::InputPin for FakePin {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(true)
        }
        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(false)
        }
    }

    struct NoDelay;
    impl embedded_hal::delay::DelayNs for NoDelay {
        fn delay_ns(&mut self, _ns: u32) {}
    }

    fn build() -> (
        Recorder,
        Epd4in2bc<Recorder, FakePin, FakePin, FakePin, NoDelay>,
    ) {
        let mut spi = Recorder::default();
        let mut delay = NoDelay;
        let epd = Epd4in2bc::new(&mut spi, FakePin, FakePin, FakePin, &mut delay, None).unwrap();
        spi.written.clear(); // drop the init stream; tests below assert their own calls
        (spi, epd)
    }

    #[test]
    fn init_matches_vendor_sequence() {
        let mut spi = Recorder::default();
        let mut delay = NoDelay;
        let _ = Epd4in2bc::new(&mut spi, FakePin, FakePin, FakePin, &mut delay, None).unwrap();

        // BoosterSoftStart 17 17 17, PowerOn, PanelSetting 0F -- and nothing else.
        assert_eq!(spi.written, [0x06, 0x17, 0x17, 0x17, 0x04, 0x00, 0x0F]);
    }

    #[test]
    fn update_frame_splits_planes_and_inverts_chromatic() {
        let (mut spi, mut epd) = build();
        let mut delay = NoDelay;

        let n = NUM_DISPLAY_BITS as usize;
        let mut buffer = Vec::new();
        buffer.extend(core::iter::repeat(0xAAu8).take(n)); // b/w plane
        buffer.extend(core::iter::repeat(0x0Fu8).take(n)); // chromatic plane

        epd.update_frame(&mut spi, &buffer, &mut delay).unwrap();

        // 0x10 + b/w plane verbatim, then 0x13 + chromatic plane inverted.
        assert_eq!(spi.written.len(), 2 + 2 * n);
        assert_eq!(spi.written[0], 0x10);
        assert!(spi.written[1..=n].iter().all(|&b| b == 0xAA));
        assert_eq!(spi.written[n + 1], 0x13);
        assert!(spi.written[n + 2..].iter().all(|&b| b == 0xF0));
    }

    #[test]
    fn clear_frame_writes_white_to_both_planes() {
        let (mut spi, mut epd) = build();
        let mut delay = NoDelay;

        epd.clear_frame(&mut spi, &mut delay).unwrap();

        let n = NUM_DISPLAY_BITS as usize;
        assert_eq!(spi.written.len(), 2 + 2 * n);
        assert_eq!(spi.written[0], 0x10);
        assert_eq!(spi.written[n + 1], 0x13);
        // A set bit is white on both planes.
        assert!(spi.written[1..=n].iter().all(|&b| b == 0xFF));
        assert!(spi.written[n + 2..].iter().all(|&b| b == 0xFF));
    }

    /// End-to-end polarity check: draw through `Display4in2bc` and assert the bytes that reach the
    /// panel carry the polarity the manual documents -- on both planes a set bit is white, and on
    /// the chromatic plane a cleared bit is the chromatic pigment.
    #[cfg(feature = "graphics")]
    #[test]
    fn drawn_colors_reach_panel_with_vendor_polarity() {
        use embedded_graphics::prelude::*;

        let (mut spi, mut epd) = build();
        let mut delay = NoDelay;

        let mut display = Display4in2bc::default();
        display.clear(TriColor::White).unwrap();
        // Row 0: black at x=0, white at x=1 (from the clear), chromatic at x=2.
        display.set_pixel(Pixel(Point::new(0, 0), TriColor::Black));
        display.set_pixel(Pixel(Point::new(2, 0), TriColor::Chromatic));

        epd.update_frame(&mut spi, display.buffer(), &mut delay)
            .unwrap();

        let n = NUM_DISPLAY_BITS as usize;
        let bw_byte0 = spi.written[1];
        let chromatic_byte0 = spi.written[n + 2];

        // b/w plane: bit7 (x=0) cleared = black, every other pixel left white.
        assert_eq!(bw_byte0, 0b0111_1111, "b/w plane polarity");
        // chromatic plane as sent: bit5 (x=2) cleared = chromatic, everything else white.
        assert_eq!(chromatic_byte0, 0b1101_1111, "chromatic plane polarity");
    }

    #[test]
    fn sleep_powers_off_then_deep_sleeps() {
        let (mut spi, mut epd) = build();
        let mut delay = NoDelay;

        epd.sleep(&mut spi, &mut delay).unwrap();

        assert_eq!(spi.written, [0x02, 0x07, 0xA5]);
    }

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 400);
        assert_eq!(HEIGHT, 300);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, TriColor::White);
    }

    #[test]
    fn buffer_is_dual_plane() {
        assert_eq!(NUM_DISPLAY_BITS, 15_000);
        assert_eq!(
            buffer_len(WIDTH as usize, HEIGHT as usize * 2),
            2 * NUM_DISPLAY_BITS as usize
        );
    }
}
