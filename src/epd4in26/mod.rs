//! A driver for the Waveshare 4.26" grayscale E-Ink display via SPI.
//!
//! Based on the reference code from [Waveshare](https://www.waveshare.com/4.26inch-e-Paper.htm).
//!
//! The SSD1677 controller in this display has two banks of RAM, one for the current frame and one
//! for the previous. The controller uses the previous frame as a reference to compute waveforms
//! to bring e-ink particles from the current to the desired state.
//!
//! A pair of update_frame() and display_frame() calls will perform the equivalent of
//! EPD_4in26_Display_Base() in Waveshare's C driver: send the current frame to both RAM banks
//! and perform an update with the complete waveform to get rid of all ghosting. This is the slowest
//! and highest quality sequence, and will visibly flash the screen black before showing the
//! desired image. It takes 1.9 seconds on my display.
//!
//! While this display also supports faster full screen updates, I haven't bothered with the
//! QuickRefresh trait to support them because they're only marginally faster than the default
//! (1.8 secs) and they still flash the screen to black and back. If you want faster updates,
//! I recommend a "partial" update of the whole screen (see below).
//!
//! This driver also supports partial refreshes, which is a bit of a misnomer. The controller
//! has partial RAM addressing (through set_windows() and set_cursor()) but the
//! screen will be refreshed entirely. This is fast (~0.9 seconds) and doesn't flash to black.
//! Effectively, the "partial" feature saves just a few milliseconds of SPI transfer time,
//! and it might be easier to use the full buffer for update_partial_frame() too. Please note
//! that you should call update_partial_frame() again after a display_frame() with a partial
//! update otherwise the new contents will disappear. GxEPD2 does the same.
//!
//! # Examples
//!
//!```rust, no_run
//!# use embedded_hal_mock::eh1::*;
//!# fn main() -> Result<(), embedded_hal::spi::ErrorKind> {
//!use embedded_graphics::{
//!    pixelcolor::BinaryColor::On as Black, prelude::*, primitives::{Line, PrimitiveStyle},
//!};
//!use epd_waveshare::{epd4in26::*, prelude::*};
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
//!let mut epd = Epd4in26::new(&mut spi, busy_in, dc, rst, &mut delay, None)?;
//!let rec_buffer = [0x00; 8];  // 8x8 black pixels
//!epd.update_partial_frame(&mut spi, &mut delay, &rec_buffer, 16, 24, 8, 8)?;
//!epd.display_frame(&mut spi, &mut delay)?;
//!// Need to do this again to prevent the new content to disappear on the next
//!// update.
//!epd.update_partial_frame(&mut spi, &mut delay, &rec_buffer, 16, 24, 8, 8)?;
//!
//!epd.sleep(&mut spi, &mut delay)?;
//!# Ok(())
//!# }
//!```

use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::{
    buffer_len,
    color::Color,
    interface::DisplayInterface,
    traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay},
};

pub(crate) mod command;

use self::command::Command;

/// Display width in pixels.
pub const WIDTH: u32 = 800;
/// Display height in pixels.
pub const HEIGHT: u32 = 480;
/// Default background color.
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;

const IS_BUSY_LOW: bool = false;
const SINGLE_BYTE_WRITE: bool = true;

/// Full size buffer for use with the 4in26 EPD
#[cfg(feature = "graphics")]
pub type Display4in26 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Each of these DisplayModes corresponds to a different waveform sequence
/// selected using Command::DisplayUpdateControl. See update_display() for details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayMode {
    Full,
    Fast,
    Partial,
}

impl From<RefreshLut> for DisplayMode {
    fn from(value: RefreshLut) -> Self {
        match value {
            RefreshLut::Full => DisplayMode::Full,
            RefreshLut::Quick => DisplayMode::Fast,
        }
    }
}

/// EPD4in26 driver.
pub struct Epd4in26<SPI, BUSY, DC, RST, DELAY> {
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    color: Color,
    refresh: RefreshLut,
    pending_display: Option<DisplayMode>,
}

impl<SPI, BUSY, DC, RST, DELAY> Epd4in26<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn send_rows(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        width_px: u32,
        height_px: u32,
    ) -> Result<(), SPI::Error> {
        // Send buffer row-by-row to account for the y-decrement mode.
        let bytes_per_row = width_px / 8;
        for row in 0..height_px {
            let start = row * bytes_per_row;
            let end = (row + 1) * bytes_per_row;
            self.interface
                .data(spi, &buffer[start as usize..end as usize])?;
        }
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd4in26<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.reset(delay, 100_000, 2_000);
        delay.delay_us(100_000);

        self.wait_until_idle(spi, delay)?;
        self.command(spi, Command::SwReset)?;
        self.wait_until_idle(spi, delay)?;

        self.cmd_with_data(spi, Command::TemperatureSensorSelection, &[0x80])?;
        self.cmd_with_data(
            spi,
            Command::BoosterSoftStart,
            &[0xAE, 0xC7, 0xC3, 0xC0, 0x80],
        )?;

        let h = self.height();
        self.cmd_with_data(
            spi,
            Command::DriverOutputControl,
            &[((h - 1) % 256) as u8, ((h - 1) / 256) as u8, 0x02],
        )?;

        self.cmd_with_data(spi, Command::BorderWaveformControl, &[0x01])?;

        // X increment and Y decrement to match the display layout.
        self.cmd_with_data(spi, Command::DataEntryMode, &[0x01])?;

        self.set_full_window(spi)?;
        self.set_cursor(spi, 0, 0)?;

        self.wait_until_idle(spi, delay)?;

        self.pending_display = None;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd4in26<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
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

        let mut epd = Epd4in26 {
            interface,
            color: DEFAULT_BACKGROUND_COLOR,
            refresh: RefreshLut::Full,
            pending_display: None,
        };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::DeepSleep, &[0x03])?;
        delay.delay_us(100_000);
        Ok(())
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn set_background_color(&mut self, color: Self::DisplayColor) {
        self.color = color;
    }

    fn background_color(&self) -> &Self::DisplayColor {
        &self.color
    }

    fn width(&self) -> u32 {
        WIDTH
    }

    fn height(&self) -> u32 {
        HEIGHT
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        debug_assert_eq!(buffer.len(), buffer_len(WIDTH as usize, HEIGHT as usize));

        self.wait_until_idle(spi, delay)?;
        self.set_full_window(spi)?;
        self.set_cursor(spi, 0, 0)?;

        self.command(spi, Command::WriteRam)?;
        self.send_rows(spi, buffer, WIDTH, HEIGHT)?;

        self.command(spi, Command::WriteRam2)?;
        self.send_rows(spi, buffer, WIDTH, HEIGHT)?;

        self.pending_display = Some(DisplayMode::from(self.refresh));

        Ok(())
    }

    fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        debug_assert_eq!(buffer.len(), buffer_len(width as usize, height as usize));
        // Pixels are addressable in groups of 8 only.
        debug_assert_eq!(width % 8, 0);

        self.wait_until_idle(spi, delay)?;

        self.set_windows(spi, x, y, x + width - 1, y + height - 1)?;
        self.set_cursor(spi, x, y)?;

        self.command(spi, Command::WriteRam)?;
        self.send_rows(spi, buffer, width, height)?;

        self.pending_display = Some(DisplayMode::Partial);
        Ok(())
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        let mode = self
            .pending_display
            .take()
            .unwrap_or_else(|| DisplayMode::from(self.refresh));

        self.turn_on_display(spi, delay, mode)?;

        Ok(())
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

    fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.set_full_window(spi)?;
        self.set_cursor(spi, 0, 0)?;

        let color = self.color.get_byte_value();
        let size = WIDTH / 8 * HEIGHT;

        self.command(spi, Command::WriteRam)?;
        self.interface.data_x_times(spi, color, size)?;

        self.command(spi, Command::WriteRam2)?;
        self.interface.data_x_times(spi, color, size)?;

        self.pending_display = Some(DisplayMode::Full);
        Ok(())
    }

    fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        if let Some(rate) = refresh_rate {
            self.refresh = rate;
        }
        Ok(())
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd4in26<SPI, BUSY, DC, RST, DELAY>
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

    fn set_full_window(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        self.set_windows(spi, 0, 0, WIDTH - 1, HEIGHT - 1)
    }

    fn map_y(&self, y: u32) -> u32 {
        // Map from canvas coordinates to controller coordinates accounting for the reversed Y axis, see:
        // https://github.com/ZinggJM/GxEPD2/blob/9c6775b9c5a51743ab0d8aa36760a51d38ddf32b/src/gdeq/GxEPD2_426_GDEQ0426T82.cpp#L301
        HEIGHT - y - 1
    }

    fn set_windows(
        &mut self,
        spi: &mut SPI,
        x_start: u32,
        y_start: u32,
        x_end: u32,
        y_end: u32,
    ) -> Result<(), SPI::Error> {
        let x_data = [
            (x_start & 0xFF) as u8,
            ((x_start >> 8) & 0x03) as u8,
            (x_end & 0xFF) as u8,
            ((x_end >> 8) & 0x03) as u8,
        ];
        self.cmd_with_data(spi, Command::SetRamXAddressStartEnd, &x_data)?;

        // Map from canvas to controller coordinates accounting for the reversed Y axis (y decrement mode in 0x01).
        let (y_start, y_end) = (self.map_y(y_start), self.map_y(y_end));

        let y_data = [
            (y_start & 0xFF) as u8,
            ((y_start >> 8) & 0x03) as u8,
            (y_end & 0xFF) as u8,
            ((y_end >> 8) & 0x03) as u8,
        ];
        self.cmd_with_data(spi, Command::SetRamYAddressStartEnd, &y_data)
    }

    fn set_cursor(&mut self, spi: &mut SPI, x: u32, y: u32) -> Result<(), SPI::Error> {
        let x_data = [(x & 0xFF) as u8, ((x >> 8) & 0x03) as u8];
        self.cmd_with_data(spi, Command::SetRamXAddressCounter, &x_data)?;

        // Map from canvas to controller coordinates accounting for the reversed Y axis (y decrement mode in 0x01).
        let y = self.map_y(y);

        let y_data = [(y & 0xFF) as u8, ((y >> 8) & 0x03) as u8];
        self.cmd_with_data(spi, Command::SetRamYAddressCounter, &y_data)
    }

    fn turn_on_display(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        mode: DisplayMode,
    ) -> Result<(), SPI::Error> {
        // This is a magic trick in the Waveshare driver. It seems that it
        // overrides the temperature reading so that the controller selects
        // a faster waveform. We only do this for fast refreshes.
        if mode == DisplayMode::Fast {
            self.cmd_with_data(spi, Command::TemperatureSensorWrite, &[0x5a])?;
        }

        // These magic constants encode which steps of the pixel update process
        // should be executed. Skipping some (eg calibration) steps speeds up
        // the refresh but introduces ghosting.
        // Here, we decide which steps to run based on how the image data was
        // sent to the controller's RAM (see crate doc).
        let update_data = match mode {
            DisplayMode::Full => 0xF7,
            DisplayMode::Fast => 0xC7,
            DisplayMode::Partial => 0xFF,
        };

        self.cmd_with_data(spi, Command::DisplayUpdateControl, &[update_data])?;
        // This triggers the actual refresh.
        self.command(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)?;

        self.pending_display = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 800);
        assert_eq!(HEIGHT, 480);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }
}
