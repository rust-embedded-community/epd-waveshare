//! A simple Driver for the Waveshare 6.65 inch (F) E-Ink Display via SPI
//!
//! # References
//!
//! - [Datasheet](https://www.waveshare.com/wiki/5.65inch_e-Paper_Module_(F))
//! - [Waveshare C driver](https://github.com/waveshare/e-Paper/blob/master/RaspberryPi%26JetsonNano/c/lib/e-Paper/EPD_5in65f.c)
//! - [Waveshare Python driver](https://github.com/waveshare/e-Paper/blob/master/RaspberryPi%26JetsonNano/python/lib/waveshare_epd/epd5in65f.py)

use embedded_hal::digital::{InputPin, OutputPin};

use crate::color::OctColor;
use crate::interface::{DelayNs, DisplayInterface, SpiDevice};
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Full size buffer for use with the 5in65f EPD
#[cfg(feature = "graphics")]
pub type Display5in65f = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize * 4) },
    OctColor,
>;

/// Width of the display
pub const WIDTH: u32 = 600;
/// Height of the display
pub const HEIGHT: u32 = 448;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: OctColor = OctColor::White;
/// Default mode of writing data (single byte vs blockwise)
const SINGLE_BYTE_WRITE: bool = true;

/// Epd5in65f driver
///
pub struct Epd5in65f<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: OctColor,
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd5in65f<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Reset the device
        self.interface.reset(delay, 10_000, 2_000).await;

        self.cmd_with_data(spi, Command::PanelSetting, &[0xEF, 0x08])
            .await?;
        self.cmd_with_data(spi, Command::PowerSetting, &[0x37, 0x00, 0x23, 0x23])
            .await?;
        self.cmd_with_data(spi, Command::PowerOffSequenceSetting, &[0x00])
            .await?;
        self.cmd_with_data(spi, Command::BoosterSoftStart, &[0xC7, 0xC7, 0x1D])
            .await?;
        self.cmd_with_data(spi, Command::PllControl, &[0x3C])
            .await?;
        self.cmd_with_data(spi, Command::TemperatureSensor, &[0x00])
            .await?;
        self.update_vcom(spi).await?;
        self.cmd_with_data(spi, Command::TconSetting, &[0x22])
            .await?;
        self.send_resolution(spi).await?;

        self.cmd_with_data(spi, Command::FlashMode, &[0xAA]).await?;

        delay.delay_us(100_000).await;

        self.update_vcom(spi).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd5in65f<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    type DisplayColor = OctColor;
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

        let mut epd = Epd5in65f { interface, color };

        epd.init(spi, delay).await?;

        Ok(epd)
    }

    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay).await
    }

    async fn sleep(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.cmd_with_data(spi, Command::DeepSleep, &[0xA5]).await?;
        Ok(())
    }

    async fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.update_vcom(spi).await?;
        self.send_resolution(spi).await?;
        self.cmd_with_data(spi, Command::DataStartTransmission1, buffer)
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
        unimplemented!();
    }

    async fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.command(spi, Command::PowerOn).await?;
        self.wait_until_idle(spi, delay).await?;
        self.command(spi, Command::DisplayRefresh).await?;
        self.wait_until_idle(spi, delay).await?;
        self.command(spi, Command::PowerOff).await?;
        self.wait_busy_low(delay).await;
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
        let bg = OctColor::colors_byte(self.color, self.color);
        self.wait_until_idle(spi, delay).await?;
        self.update_vcom(spi).await?;
        self.send_resolution(spi).await?;
        self.command(spi, Command::DataStartTransmission1).await?;
        self.interface
            .data_x_times(spi, bg, WIDTH * HEIGHT / 2)
            .await?;
        self.display_frame(spi, delay).await?;
        Ok(())
    }

    fn set_background_color(&mut self, color: OctColor) {
        self.color = color;
    }

    fn background_color(&self) -> &OctColor {
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
        self.interface.wait_until_idle(delay, true).await;
        Ok(())
    }
}

#[maybe_async::maybe_async]
impl<SPI, BUSY, DC, RST, DELAY> Epd5in65f<SPI, BUSY, DC, RST, DELAY>
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

    async fn wait_busy_low(&mut self, delay: &mut DELAY) {
        self.interface.wait_until_idle(delay, false).await;
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

    async fn update_vcom(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        let bg_color = (self.color.get_nibble() & 0b111) << 5;
        self.cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x17 | bg_color])
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 600);
        assert_eq!(HEIGHT, 448);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, OctColor::White);
    }
}
