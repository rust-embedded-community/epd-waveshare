//! A simple Driver for the Waveshare 5.83" v2 E-Ink Display via SPI
//!
//! # References
//!
//! - [Datasheet](https://www.waveshare.com/5.83inch-e-paper-hat.htm)
//! - [Waveshare C driver](https://github.com/waveshare/e-Paper/blob/master/RaspberryPi_JetsonNano/c/lib/e-Paper/EPD_5in83_V2.c)
//! - [Waveshare Python driver](https://github.com/waveshare/e-Paper/blob/master/RaspberryPi_JetsonNano/python/lib/waveshare_epd/epd5in83_V2.py)

use embedded_hal::digital::{InputPin, OutputPin};

use crate::color::Color;
use crate::interface::{DelayNs, DisplayInterface, SpiDevice};
use crate::prelude::WaveshareDisplay;
use crate::traits::{InternalWiAdditions, RefreshLut};

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Full size buffer for use with the 5in83 v2 EPD
#[cfg(feature = "graphics")]
pub type Display5in83 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Width of the display
pub const WIDTH: u32 = 648;
/// Height of the display
pub const HEIGHT: u32 = 480;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const IS_BUSY_LOW: bool = true;
const NUM_DISPLAY_BITS: u32 = WIDTH * HEIGHT / 8;
const SINGLE_BYTE_WRITE: bool = true;

/// Epd5in83 driver
///
pub struct Epd5in83<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: Color,
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd5in83<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Reset the device
        self.interface.reset(delay, 2000, 50).await;

        // Set the power settings: VGH=20V,VGL=-20V,VDH=15V,VDL=-15V
        self.cmd_with_data(spi, Command::PowerSetting, &[0x07, 0x07, 0x3F, 0x3F])
            .await?;

        // Power on
        self.command(spi, Command::PowerOn).await?;
        delay.delay_us(5000).await;
        self.wait_until_idle(spi, delay).await?;

        // Set the panel settings: BWOTP
        self.cmd_with_data(spi, Command::PanelSetting, &[0x1F])
            .await?;

        // Set the real resolution
        self.send_resolution(spi).await?;

        // Disable dual SPI
        self.cmd_with_data(spi, Command::DualSPI, &[0x00]).await?;

        // Set Vcom and data interval
        self.cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x10, 0x07])
            .await?;

        // Set S2G and G2S non-overlap periods to 12 (default)
        self.cmd_with_data(spi, Command::TconSetting, &[0x22])
            .await?;

        self.wait_until_idle(spi, delay).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd5in83<SPI, BUSY, DC, RST, DELAY>
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

        let mut epd = Epd5in83 { interface, color };

        epd.init(spi, delay).await?;

        Ok(epd)
    }

    async fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.command(spi, Command::PowerOff).await?;
        self.wait_until_idle(spi, delay).await?;
        self.cmd_with_data(spi, Command::DeepSleep, &[0xA5]).await?;
        Ok(())
    }

    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay).await
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

    async fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        let color_value = self.color.get_byte_value();

        self.interface
            .cmd(spi, Command::DataStartTransmission1)
            .await?;
        self.interface
            .data_x_times(spi, color_value, WIDTH / 8 * HEIGHT)
            .await?;

        self.interface
            .cmd_with_data(spi, Command::DataStartTransmission2, buffer)
            .await?;
        Ok(())
    }

    async fn update_partial_frame(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _buffer: &[u8],
        _x: u32,
        _y: u32,
        _width: u32,
        _height: u32,
    ) -> Result<(), SPI::Error> {
        unimplemented!()
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
        self.display_frame(spi, delay).await?;
        Ok(())
    }

    async fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;

        self.command(spi, Command::DataStartTransmission1).await?;
        self.interface
            .data_x_times(spi, 0xFF, NUM_DISPLAY_BITS)
            .await?;

        self.command(spi, Command::DataStartTransmission2).await?;
        self.interface
            .data_x_times(spi, 0x00, NUM_DISPLAY_BITS)
            .await?;

        Ok(())
    }

    async fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        unimplemented!();
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
impl<SPI, BUSY, DC, RST, DELAY> Epd5in83<SPI, BUSY, DC, RST, DELAY>
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

    async fn send_resolution(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        let w = self.width();
        let h = self.height();

        self.command(spi, Command::TconResolution).await?;
        self.send_data(spi, &[(w >> 8) as u8]).await?;
        self.send_data(spi, &[w as u8]).await?;
        self.send_data(spi, &[(h >> 8) as u8]).await?;
        self.send_data(spi, &[h as u8]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 648);
        assert_eq!(HEIGHT, 480);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }
}
