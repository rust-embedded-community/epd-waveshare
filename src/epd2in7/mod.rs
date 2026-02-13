//! A simple Driver for the Waveshare 2.7" E-Ink Display via SPI
//!
//! [Documentation](https://www.waveshare.com/wiki/2.7inch_e-Paper_HAT)

use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::interface::DisplayInterface;
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

// The Lookup Tables for the Display
mod constants;
use crate::epd2in7::constants::*;

/// Width of the display
pub const WIDTH: u32 = 176;
/// Height of the display
pub const HEIGHT: u32 = 264;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const IS_BUSY_LOW: bool = true;
const SINGLE_BYTE_WRITE: bool = true;

use crate::color::Color;

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Full size buffer for use with the 2in7 EPD
#[cfg(feature = "graphics")]
pub type Display2in7 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    { crate::graphics::DisplayMode::BwrBitOff as u8 },
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Epd2in7 driver
pub struct Epd2in7<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: Color,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in7<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // reset the device
        self.interface.reset(delay, 10_000, 2_000);

        // power setting
        self.cmd_with_data(spi, Command::PowerSetting, &[0x03, 0x00, 0x2b, 0x2b, 0x09])?;
        // booster soft start
        self.cmd_with_data(spi, Command::BoosterSoftStart, &[0x07, 0x07, 0x17])?;
        // power optimization
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x60, 0xa5])?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x89, 0xa5])?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x90, 0x00])?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x93, 0x2a])?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0xa0, 0xa5])?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0xa1, 0x00])?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x73, 0x41])?;
        // partial display refresh
        self.cmd_with_data(spi, Command::PartialDisplayRefresh, &[0x00])?;
        // power on
        self.command(spi, Command::PowerOn)?;
        delay.delay_us(5000);
        self.wait_until_idle(spi, delay)?;
        // panel setting
        self.cmd_with_data(spi, Command::PanelSetting, &[0xaf])?;
        // pll control
        self.cmd_with_data(spi, Command::PllControl, &[0x3a])?;
        // vcom and data interval setting
        self.cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x57])?;
        // cvm dc setting register
        self.cmd_with_data(spi, Command::VcmDcSetting, &[0x12])?;
        self.set_lut(spi, delay, None)?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in7<SPI, BUSY, DC, RST, DELAY>
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

        let mut epd = Epd2in7 { interface, color };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.interface
            .cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0xf7])?;

        self.command(spi, Command::PowerOff)?;
        self.wait_until_idle(spi, delay)?;
        self.interface
            .cmd_with_data(spi, Command::DeepSleep, &[0xA5])?;
        Ok(())
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::DataStartTransmission1)?;
        self.interface
            .data_x_times(spi, self.color.get_byte_value(), WIDTH * HEIGHT / 8)?;
        self.interface.cmd(spi, Command::DataStartTransmission2)?;
        self.send_data(spi, buffer)?;
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
        self.interface
            .cmd(spi, Command::PartialDataStartTransmission1)?;

        self.send_data(spi, &[(x >> 8) as u8])?;
        self.send_data(spi, &[(x & 0xf8) as u8])?;
        self.send_data(spi, &[(y >> 8) as u8])?;
        self.send_data(spi, &[(y & 0xff) as u8])?;
        self.send_data(spi, &[(width >> 8) as u8])?;
        self.send_data(spi, &[(width & 0xf8) as u8])?;
        self.send_data(spi, &[(height >> 8) as u8])?;
        self.send_data(spi, &[(height & 0xff) as u8])?;
        self.wait_until_idle(spi, delay)?;
        self.send_data(spi, buffer)
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
        self.command(spi, Command::DisplayRefresh)?;
        Ok(())
    }

    fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;

        let color_value = self.color.get_byte_value();
        self.interface.cmd(spi, Command::DataStartTransmission1)?;
        self.interface
            .data_x_times(spi, color_value, WIDTH * HEIGHT / 8)?;
        self.interface.cmd(spi, Command::DataStartTransmission2)?;
        self.interface
            .data_x_times(spi, color_value, WIDTH * HEIGHT / 8)?;
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
        spi: &mut SPI,
        delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::LutForVcom, &LUT_VCOM_DC)?;
        self.cmd_with_data(spi, Command::LutWhiteToWhite, &LUT_WW)?;
        self.cmd_with_data(spi, Command::LutBlackToWhite, &LUT_BW)?;
        self.cmd_with_data(spi, Command::LutWhiteToBlack, &LUT_WB)?;
        self.cmd_with_data(spi, Command::LutBlackToBlack, &LUT_BB)?;
        Ok(())
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd2in7<SPI, BUSY, DC, RST, DELAY>
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

    fn send_data(&mut self, spi: &mut SPI, data: &[u8]) -> Result<(), SPI::Error> {
        self.interface.data(spi, data)
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
        assert_eq!(WIDTH, 176);
        assert_eq!(HEIGHT, 264);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }
}
