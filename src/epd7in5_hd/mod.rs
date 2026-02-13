//! A simple Driver for the Waveshare 7.5" E-Ink Display (HD) via SPI
//!
//! Color values for this driver are inverted compared to the [EPD 7in5 V2 driver](crate::epd7in5_v2)
//! *EPD 7in5 HD:* White = 1/0xFF, Black = 0/0x00
//! *EPD 7in5 V2:* White = 0/0x00, Black = 1/0xFF
//!
//! # References
//!
//! - [Datasheet](https://www.waveshare.com/w/upload/2/27/7inch_HD_e-Paper_Specification.pdf)
//! - [Waveshare Python driver](https://github.com/waveshare/e-Paper/blob/master/RaspberryPi_JetsonNano/python/lib/waveshare_epd/epd7in5_HD.py)
//!
use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::color::Color;
use crate::interface::DisplayInterface;
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Full size buffer for use with the 7in5 HD EPD
#[cfg(feature = "graphics")]
pub type Display7in5 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    { crate::graphics::DisplayMode::BwrBitOff as u8 },
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Width of the display
pub const WIDTH: u32 = 880;
/// Height of the display
pub const HEIGHT: u32 = 528;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White; // Inverted for HD as compared to 7in5 v2 (HD: 0xFF = White)
const IS_BUSY_LOW: bool = false;
const SINGLE_BYTE_WRITE: bool = false;

/// EPD7in5 (HD) driver
///
pub struct Epd7in5<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: Color,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd7in5<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Reset the device
        self.interface.reset(delay, 10_000, 2_000);

        // HD procedure as described here:
        // https://github.com/waveshare/e-Paper/blob/master/RaspberryPi_JetsonNano/python/lib/waveshare_epd/epd7in5_HD.py
        // and as per specs:
        // https://www.waveshare.com/w/upload/2/27/7inch_HD_e-Paper_Specification.pdf

        self.wait_until_idle(spi, delay)?;
        self.command(spi, Command::SwReset)?;
        self.wait_until_idle(spi, delay)?;

        self.cmd_with_data(spi, Command::AutoWriteRed, &[0xF7])?;
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::AutoWriteBw, &[0xF7])?;
        self.wait_until_idle(spi, delay)?;

        self.cmd_with_data(spi, Command::SoftStart, &[0xAE, 0xC7, 0xC3, 0xC0, 0x40])?;

        self.cmd_with_data(spi, Command::DriverOutputControl, &[0xAF, 0x02, 0x01])?;

        self.cmd_with_data(spi, Command::DataEntry, &[0x01])?;

        self.cmd_with_data(spi, Command::SetRamXStartEnd, &[0x00, 0x00, 0x6F, 0x03])?;
        self.cmd_with_data(spi, Command::SetRamYStartEnd, &[0xAF, 0x02, 0x00, 0x00])?;

        self.cmd_with_data(spi, Command::VbdControl, &[0x05])?;

        self.cmd_with_data(spi, Command::TemperatureSensorControl, &[0x80])?;

        self.cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xB1])?;

        self.command(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)?;

        self.cmd_with_data(spi, Command::SetRamXAc, &[0x00, 0x00])?;
        self.cmd_with_data(spi, Command::SetRamYAc, &[0x00, 0x00])?;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd7in5<SPI, BUSY, DC, RST, DELAY>
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
        let color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = Epd7in5 { interface, color };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::DeepSleep, &[0x01])?;
        Ok(())
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::SetRamYAc, &[0x00, 0x00])?;
        self.cmd_with_data(spi, Command::WriteRamBw, buffer)?;
        self.cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xF7])?;
        Ok(())
    }

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
        unimplemented!();
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::MasterActivation)?;
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
        let pixel_count = WIDTH / 8 * HEIGHT;
        let background_color_byte = self.color.get_byte_value();

        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::SetRamYAc, &[0x00, 0x00])?;

        for cmd in &[Command::WriteRamBw, Command::WriteRamRed] {
            self.command(spi, *cmd)?;
            self.interface
                .data_x_times(spi, background_color_byte, pixel_count)?;
        }

        self.cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xF7])?;
        self.command(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }

    fn set_background_color(&mut self, color: Color) {
        self.color = color;
    }

    fn background_color(&self) -> &Color {
        &self.color
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
        unimplemented!();
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd7in5<SPI, BUSY, DC, RST, DELAY>
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 880);
        assert_eq!(HEIGHT, 528);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }
}
