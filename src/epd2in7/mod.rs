//! A simple Driver for the Waveshare 2.7" E-Ink Display via SPI
//!
//! [Documentation](https://www.waveshare.com/wiki/2.7inch_e-Paper_HAT)

use embedded_hal::digital::{InputPin, OutputPin};

use crate::interface::{DelayNs, DisplayInterface, SpiDevice};
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
    false,
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

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in7<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // reset the device
        self.interface.reset(delay, 10_000, 2_000).await;

        // power setting
        self.cmd_with_data(spi, Command::PowerSetting, &[0x03, 0x00, 0x2b, 0x2b, 0x09])
            .await?;
        // booster soft start
        self.cmd_with_data(spi, Command::BoosterSoftStart, &[0x07, 0x07, 0x17])
            .await?;
        // power optimization
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x60, 0xa5])
            .await?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x89, 0xa5])
            .await?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x90, 0x00])
            .await?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x93, 0x2a])
            .await?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0xa0, 0xa5])
            .await?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0xa1, 0x00])
            .await?;
        self.cmd_with_data(spi, Command::PowerOptimization, &[0x73, 0x41])
            .await?;
        // partial display refresh
        self.cmd_with_data(spi, Command::PartialDisplayRefresh, &[0x00])
            .await?;
        // power on
        self.command(spi, Command::PowerOn).await?;
        delay.delay_us(5000).await;
        self.wait_until_idle(spi, delay).await?;
        // panel setting
        self.cmd_with_data(spi, Command::PanelSetting, &[0xaf])
            .await?;
        // pll control
        self.cmd_with_data(spi, Command::PllControl, &[0x3a])
            .await?;
        // vcom and data interval setting
        self.cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x57])
            .await?;
        // cvm dc setting register
        self.cmd_with_data(spi, Command::VcmDcSetting, &[0x12])
            .await?;
        self.set_lut(spi, delay, None).await?;
        self.wait_until_idle(spi, delay).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
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
    async fn new(
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

        epd.init(spi, delay).await?;

        Ok(epd)
    }

    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay).await
    }

    async fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.interface
            .cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0xf7])
            .await?;

        self.command(spi, Command::PowerOff).await?;
        self.wait_until_idle(spi, delay).await?;
        self.interface
            .cmd_with_data(spi, Command::DeepSleep, &[0xA5])
            .await?;
        Ok(())
    }

    async fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.interface
            .cmd(spi, Command::DataStartTransmission1)
            .await?;
        self.interface
            .data_x_times(spi, self.color.get_byte_value(), WIDTH * HEIGHT / 8)
            .await?;
        self.interface
            .cmd(spi, Command::DataStartTransmission2)
            .await?;
        self.send_data(spi, buffer).await?;
        Ok(())
    }

    async fn update_partial_frame(
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
            .cmd(spi, Command::PartialDataStartTransmission1)
            .await?;

        self.send_data(spi, &[(x >> 8) as u8]).await?;
        self.send_data(spi, &[(x & 0xf8) as u8]).await?;
        self.send_data(spi, &[(y >> 8) as u8]).await?;
        self.send_data(spi, &[(y & 0xff) as u8]).await?;
        self.send_data(spi, &[(width >> 8) as u8]).await?;
        self.send_data(spi, &[(width & 0xf8) as u8]).await?;
        self.send_data(spi, &[(height >> 8) as u8]).await?;
        self.send_data(spi, &[(height & 0xff) as u8]).await?;
        self.wait_until_idle(spi, delay).await?;
        self.send_data(spi, buffer).await
    }

    async fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::DisplayRefresh).await?;
        self.wait_until_idle(spi, delay).await?;
        Ok(())
    }

    async fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.update_frame(spi, buffer, delay).await?;
        self.command(spi, Command::DisplayRefresh).await?;
        Ok(())
    }

    async fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;

        let color_value = self.color.get_byte_value();
        self.interface
            .cmd(spi, Command::DataStartTransmission1)
            .await?;
        self.interface
            .data_x_times(spi, color_value, WIDTH * HEIGHT / 8)
            .await?;
        self.interface
            .cmd(spi, Command::DataStartTransmission2)
            .await?;
        self.interface
            .data_x_times(spi, color_value, WIDTH * HEIGHT / 8)
            .await?;
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

    async fn set_lut(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.cmd_with_data(spi, Command::LutForVcom, &LUT_VCOM_DC)
            .await?;
        self.cmd_with_data(spi, Command::LutWhiteToWhite, &LUT_WW)
            .await?;
        self.cmd_with_data(spi, Command::LutBlackToWhite, &LUT_BW)
            .await?;
        self.cmd_with_data(spi, Command::LutWhiteToBlack, &LUT_WB)
            .await?;
        self.cmd_with_data(spi, Command::LutBlackToBlack, &LUT_BB)
            .await?;
        Ok(())
    }

    async fn wait_until_idle(
        &mut self,
        _spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW).await;
        Ok(())
    }
}

#[maybe_async::maybe_async]
impl<SPI, BUSY, DC, RST, DELAY> Epd2in7<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn command(&mut self, spi: &mut SPI, command: Command) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, command).await
    }

    async fn send_data(&mut self, spi: &mut SPI, data: &[u8]) -> Result<(), SPI::Error> {
        self.interface.data(spi, data).await
    }

    async fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data).await
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
