//! A Driver for the Waveshare 2.13" E-Ink Display V4 via SPI (SSD1680 controller)
//!
//! # References
//!
//! - [Waveshare product page](https://www.waveshare.com/wiki/2.13inch_e-Paper_HAT_(V4))
//! - [Waveshare Python driver](https://github.com/waveshare/e-Paper/blob/master/RaspberryPi_JetsonNano/python/lib/waveshare_epd/epd2in13_V4.py)
//!
//! # Power pin and constructors
//!
//! The V4 HAT has a power control pin (GPIO18) that must be driven HIGH before
//! the display will respond. Two constructors are available:
//!
//! - [`Epd2in13::new_with_pwr`] — accepts a power pin, drives it HIGH during
//!   init. Use this when your board has the PWR pin wired (the common case for
//!   the Waveshare HAT).
//! - [`WaveshareDisplay::new`] — no power pin, uses the [`NoPwrPin`] no-op
//!   default. Use this on custom boards where power is always on.
//!
//! The [`WaveshareDisplay`] trait is implemented with a `PWR: OutputPin + Default`
//! bound so that `new()` can construct the pin type from nothing. Real GPIO pin
//! types typically do not implement `Default`, so `new_with_pwr()` users call
//! the equivalent inherent methods (`update_frame`, `display_frame`, `sleep`,
//! etc.) directly rather than going through the trait.
//!
//! To fully power down the display after sleep, call [`Epd2in13::power_off`]
//! which drives the PWR pin LOW (matching the Python driver's `module_exit()`).
//!
//! # Example
//!
//! ```rust,ignore
//! use epd_waveshare::epd2in13_v4::{Display2in13, Epd2in13};
//! use epd_waveshare::prelude::*;
//!
//! // Setup SPI, GPIO, and delay via linux-embedded-hal (omitted)
//!
//! let mut epd = Epd2in13::new_with_pwr(
//!     &mut spi, busy, dc, rst, &mut delay, None, pwr,
//! )?;
//!
//! let mut display = Display2in13::default();
//! display.clear(Color::White).ok();
//! // ... draw with embedded-graphics ...
//!
//! epd.update_frame(&mut spi, display.buffer(), &mut delay)?;
//! epd.display_frame(&mut spi, &mut delay)?;
//! epd.sleep(&mut spi, &mut delay)?;
//! ```

/// Width of the display in pixels
pub const WIDTH: u32 = 122;

/// Height of the display in pixels
pub const HEIGHT: u32 = 250;

/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const IS_BUSY_LOW: bool = false;
const SINGLE_BYTE_WRITE: bool = true;

use embedded_hal::{
    delay::DelayNs,
    digital::{ErrorType, InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::buffer_len;
use crate::color::Color;
use crate::interface::DisplayInterface;
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

pub(crate) mod command;
use self::command::Command;

pub(crate) mod constants;

/// Full size buffer for use with the 2.13" V4 EPD
#[cfg(feature = "graphics")]
pub type Display2in13 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// No-op output pin used when no power pin is provided.
#[derive(Default)]
pub struct NoPwrPin;

impl ErrorType for NoPwrPin {
    type Error = core::convert::Infallible;
}

impl OutputPin for NoPwrPin {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Epd2in13 V4 driver (SSD1680)
///
/// The optional `PWR` type parameter supports the V4 HAT's power pin (GPIO18).
/// Use [`Epd2in13::new_with_pwr`] to supply one, or [`WaveshareDisplay::new`] without.
pub struct Epd2in13<SPI, BUSY, DC, RST, DELAY, PWR = NoPwrPin> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    background_color: Color,
    /// Optional power control pin (PWR_PIN / GPIO18 on V4 HAT)
    pwr_pin: PWR,
}

impl<SPI, BUSY, DC, RST, DELAY, PWR> Epd2in13<SPI, BUSY, DC, RST, DELAY, PWR>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
    PWR: OutputPin,
{
    /// Create a new driver with a power control pin.
    ///
    /// The V4 HAT requires GPIO18 (PWR_PIN) to be driven HIGH before the
    /// display will respond. This constructor drives the pin HIGH, then
    /// performs the normal init sequence.
    pub fn new_with_pwr(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
        mut pwr_pin: PWR,
    ) -> Result<Self, SPI::Error> {
        let _ = pwr_pin.set_high();

        let interface = DisplayInterface::new(busy, dc, rst, delay_us);

        let mut epd = Epd2in13 {
            interface,
            background_color: DEFAULT_BACKGROUND_COLOR,
            pwr_pin,
        };

        epd.init(spi, delay)?;
        Ok(epd)
    }

    fn set_ram_area(
        &mut self,
        spi: &mut SPI,
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(
            spi,
            Command::SetRamXAddressStartEndPosition,
            &[(start_x >> 3) as u8, (end_x >> 3) as u8],
        )?;

        self.interface.cmd_with_data(
            spi,
            Command::SetRamYAddressStartEndPosition,
            &[
                start_y as u8,
                (start_y >> 8) as u8,
                end_y as u8,
                (end_y >> 8) as u8,
            ],
        )
    }

    fn set_ram_counter(&mut self, spi: &mut SPI, x: u32, y: u32) -> Result<(), SPI::Error> {
        // Python SetCursor: sends x & 0xFF directly (no shift, no wait_until_idle)
        self.interface
            .cmd_with_data(spi, Command::SetRamXAddressCounter, &[(x >> 3) as u8])?;

        self.interface.cmd_with_data(
            spi,
            Command::SetRamYAddressCounter,
            &[y as u8, (y >> 8) as u8],
        )
    }

    fn turn_on_display(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // 0xF7: full refresh master activation sequence (normal)
        self.interface
            .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xF7])?;
        self.interface.cmd(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)
    }

    fn turn_on_display_fast(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // 0xC7: full refresh master activation sequence (fast)
        self.interface
            .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xC7])?;
        self.interface.cmd(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)
    }

    fn turn_on_display_partial(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        // 0xFF: partial refresh master activation sequence
        self.interface
            .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xFF])?;
        self.interface.cmd(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)
    }

    fn use_full_frame(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        self.set_ram_area(spi, 0, 0, WIDTH - 1, HEIGHT - 1)?;
        self.set_ram_counter(spi, 0, 0)
    }

    /// Initialize the display for fast refresh mode.
    ///
    /// After calling this, use `display_fast()` or `update_and_display_fast_frame()`.
    pub fn init_fast(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // SSD1680: 20 ms initial HIGH, 2 ms LOW pulse — matches Waveshare
        // Python reference driver (epd2in13_V4.py reset sequence)
        self.interface.reset(delay, 20_000, 2_000);

        self.interface.cmd(spi, Command::SwReset)?;
        self.wait_until_idle(spi, delay)?;

        // Temperature sensor control: Python sends 0x18 and 0x80 both as commands
        self.interface.cmd(spi, Command::TemperatureSensorControl)?;
        self.interface.cmd(spi, Command::ReadBuiltInTempSensor)?;

        // Data entry mode: X incr, Y incr
        self.interface
            .cmd_with_data(spi, Command::DataEntryModeSetting, &[0x03])?;

        self.set_ram_area(spi, 0, 0, WIDTH - 1, HEIGHT - 1)?;
        self.set_ram_counter(spi, 0, 0)?;

        // Load temperature and set display mode for fast
        self.interface
            .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xB1])?;
        self.interface.cmd(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)?;

        // Write temperature register
        self.interface
            .cmd_with_data(spi, Command::WriteTempRegister, &[0x64, 0x00])?;

        self.interface
            .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0x91])?;
        self.interface.cmd(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)?;

        Ok(())
    }

    /// Display an image buffer using fast refresh.
    pub fn display_fast(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)?;
        self.turn_on_display_fast(spi, delay)
    }

    /// Update and display a frame using fast refresh.
    pub fn update_and_display_fast_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.use_full_frame(spi)?;
        self.display_fast(spi, buffer, delay)
    }

    /// Display partial update of the frame.
    ///
    /// This performs a soft reset and reconfigures for partial refresh
    /// before writing the buffer.
    pub fn display_partial(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        // Soft reset: RST LOW 1ms then HIGH — no trailing delay.
        // Using soft_reset() instead of reset() which adds 200ms that
        // would cause the controller to perform a full reset.
        self.interface.soft_reset(delay, 1_000);

        self.interface
            .cmd_with_data(spi, Command::BorderWaveformControl, &[0x80])?;

        self.interface
            .cmd_with_data(spi, Command::DriverOutputControl, &[0xF9, 0x00, 0x00])?;

        self.interface
            .cmd_with_data(spi, Command::DataEntryModeSetting, &[0x03])?;

        self.set_ram_area(spi, 0, 0, WIDTH - 1, HEIGHT - 1)?;
        self.set_ram_counter(spi, 0, 0)?;

        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)?;

        self.turn_on_display_partial(spi, delay)
    }

    /// Transmit a full frame to the display RAM.
    pub fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        assert!(buffer.len() == buffer_len(WIDTH as usize, HEIGHT as usize));
        self.use_full_frame(spi)?;
        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)?;
        Ok(())
    }

    /// Display the frame data from RAM.
    pub fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.turn_on_display(spi, delay)
    }

    /// Update and display a frame in one call.
    pub fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.update_frame(spi, buffer, delay)?;
        self.display_frame(spi, delay)
    }

    /// Clear the display RAM with the background color.
    ///
    /// This only writes to RAM. Call `display_frame()` afterwards to
    /// trigger a refresh, matching the behavior of other drivers.
    pub fn clear_frame(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.use_full_frame(spi)?;

        let color = self.background_color.get_byte_value();

        self.interface.cmd(spi, Command::WriteRam)?;
        self.interface.data_x_times(
            spi,
            color,
            buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
        )?;

        Ok(())
    }

    /// Enter deep sleep mode.
    ///
    /// The display retains its image and can be woken with `wake_up()`.
    /// To fully power down, call `power_off()` after this.
    pub fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.interface
            .cmd_with_data(spi, Command::DeepSleepMode, &[0x01])?;
        Ok(())
    }

    /// Drive the power pin LOW, fully powering down the display.
    ///
    /// Matches Python's `module_exit()`. Call after `sleep()` when the
    /// display is no longer needed. A subsequent `wake_up()` will drive
    /// PWR HIGH again during init.
    pub fn power_off(&mut self) {
        let _ = self.pwr_pin.set_low();
    }

    /// Reinitialize the display. Matches Python's `epd.init()`.
    ///
    /// Call this after `clear_frame()` and before writing new frame data,
    /// following the V4 init-clear-init-display pattern.
    pub fn reinit(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    /// Wake up from deep sleep and reinitialize.
    pub fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    /// Wait until the display is idle.
    pub fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }

    /// Set the background color.
    pub fn set_background_color(&mut self, background_color: Color) {
        self.background_color = background_color;
    }

    /// Get the current background color.
    pub fn background_color(&self) -> &Color {
        &self.background_color
    }

    /// Write the buffer to both RAM and RAM2 (base image for partial refresh).
    pub fn display_part_base_image(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.use_full_frame(spi)?;

        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)?;
        self.interface
            .cmd_with_data(spi, Command::WriteRam2, buffer)?;

        self.turn_on_display(spi, delay)
    }
}

impl<SPI, BUSY, DC, RST, DELAY, PWR> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in13<SPI, BUSY, DC, RST, DELAY, PWR>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
    PWR: OutputPin,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Drive power pin HIGH before any SPI communication (V4 requirement)
        let _ = self.pwr_pin.set_high();

        // SSD1680: 20 ms initial HIGH, 2 ms LOW pulse — matches Waveshare
        // Python reference driver (epd2in13_V4.py reset sequence)
        self.interface.reset(delay, 20_000, 2_000);
        self.wait_until_idle(spi, delay)?;

        // Software reset
        self.interface.cmd(spi, Command::SwReset)?;
        self.wait_until_idle(spi, delay)?;

        // Driver output control: set gate lines = HEIGHT - 1 = 0xF9
        self.interface
            .cmd_with_data(spi, Command::DriverOutputControl, &[0xF9, 0x00, 0x00])?;

        // Data entry mode: X incr, Y incr
        self.interface
            .cmd_with_data(spi, Command::DataEntryModeSetting, &[0x03])?;

        // Set RAM window and cursor
        self.set_ram_area(spi, 0, 0, WIDTH - 1, HEIGHT - 1)?;
        self.set_ram_counter(spi, 0, 0)?;

        // Border waveform control
        self.interface
            .cmd_with_data(spi, Command::BorderWaveformControl, &[0x05])?;

        // Display update control 1: 0x00, 0x80 -- critical V4 difference from V2/V3
        self.interface
            .cmd_with_data(spi, Command::DisplayUpdateControl1, &[0x00, 0x80])?;

        // Temperature sensor control: use internal sensor
        self.interface
            .cmd_with_data(spi, Command::TemperatureSensorControl, &[0x80])?;

        self.wait_until_idle(spi, delay)?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY, PWR> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in13<SPI, BUSY, DC, RST, DELAY, PWR>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
    PWR: OutputPin + Default,
{
    type DisplayColor = Color;

    fn new(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
    ) -> Result<Self, SPI::Error> {
        let interface = DisplayInterface::new(busy, dc, rst, delay_us);

        let mut epd = Epd2in13 {
            interface,
            background_color: DEFAULT_BACKGROUND_COLOR,
            pwr_pin: PWR::default(),
        };

        epd.init(spi, delay)?;
        Ok(epd)
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.interface
            .cmd_with_data(spi, Command::DeepSleepMode, &[0x01])?;
        Ok(())
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        assert!(buffer.len() == buffer_len(WIDTH as usize, HEIGHT as usize));
        self.use_full_frame(spi)?;
        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)?;
        Ok(())
    }

    fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        self.set_ram_area(spi, x, y, x + width, y + height)?;
        self.set_ram_counter(spi, x, y)?;
        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)?;
        Ok(())
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.turn_on_display(spi, delay)
    }

    fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.update_frame(spi, buffer, delay)?;
        self.display_frame(spi, delay)
    }

    fn clear_frame(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.use_full_frame(spi)?;

        let color = self.background_color.get_byte_value();

        self.interface.cmd(spi, Command::WriteRam)?;
        self.interface.data_x_times(
            spi,
            color,
            buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
        )?;

        Ok(())
    }

    fn set_background_color(&mut self, background_color: Color) {
        self.background_color = background_color;
    }

    fn background_color(&self) -> &Color {
        &self.background_color
    }

    fn width(&self) -> u32 {
        WIDTH
    }

    fn height(&self) -> u32 {
        HEIGHT
    }

    fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        // V4 uses internal LUT, no custom LUT needed
        Ok(())
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 122);
        assert_eq!(HEIGHT, 250);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
        assert_eq!(buffer_len(WIDTH as usize, HEIGHT as usize), 16 * 250);
    }
}
