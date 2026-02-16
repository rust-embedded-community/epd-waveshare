//! A simple Driver for the Waveshare 2.7inch v2 e-Paper HAT Display via SPI
//!
//! 4 Gray support and partial refresh is not fully implemented yet.
//!
//! # References
//!
//! - [Datasheet](https://www.waveshare.com/wiki/2.7inch_e-Paper_HAT_Manual)
//! - [Waveshare C driver](https://github.com/waveshareteam/e-Paper/blob/master/RaspberryPi_JetsonNano/c/lib/e-Paper/EPD_2in7_V2.c)
//! - [Waveshare Python driver](https://github.com/waveshareteam/e-Paper/blob/master/RaspberryPi_JetsonNano/python/lib/waveshare_epd/epd2in7_V2.py)

use embedded_hal::digital::{InputPin, OutputPin};

use crate::{
    buffer_len,
    color::Color,
    interface::{DelayNs, DisplayInterface, SpiDevice},
    traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay},
    type_a::command::Command,
};

/// Width of the display
pub const WIDTH: u32 = 176;
/// Height of the display
pub const HEIGHT: u32 = 264;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;

const IS_BUSY_LOW: bool = false;
const SINGLE_BYTE_WRITE: bool = true;

/// Full size buffer for use with the 2in7B EPD
/// TODO this should be a TriColor, but let's keep it as is at first
#[cfg(feature = "graphics")]
pub type Display2in7 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Epd2in7b driver
pub struct Epd2in7<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: Color,
    refresh: RefreshLut,
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
        self.interface.reset(delay, 200_000, 2_000).await;

        self.wait_until_idle(spi, delay).await?;
        self.command(spi, Command::SwReset).await?;
        self.wait_until_idle(spi, delay).await?;

        self.use_full_frame(spi, delay).await?;

        self.interface
            .cmd_with_data(spi, Command::DataEntryModeSetting, &[0x03])
            .await?;

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

        let mut epd = Epd2in7 {
            interface,
            color,
            refresh: RefreshLut::Full,
        };

        epd.init(spi, delay).await?;

        Ok(epd)
    }

    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay).await
    }

    async fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.interface
            .cmd_with_data(spi, Command::DeepSleepMode, &[0x01])
            .await?;
        Ok(())
    }

    async fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.use_full_frame(spi, delay).await?;
        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)
            .await?;
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
        self.wait_until_idle(spi, delay).await?;
        self.set_ram_area(spi, x, y, x + width, y + height).await?;
        self.set_ram_counter(spi, delay, x, y).await?;

        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)
            .await?;

        Ok(())
    }

    async fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        if self.refresh == RefreshLut::Full {
            self.interface
                .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xF7])
                .await?;
        } else if self.refresh == RefreshLut::Quick {
            self.interface
                .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xC7])
                .await?;
        }

        self.interface.cmd(spi, Command::MasterActivation).await?;
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
        self.use_full_frame(spi, delay).await?;

        let color = self.color.get_byte_value();

        self.interface.cmd(spi, Command::WriteRam).await?;
        self.interface
            .data_x_times(spi, color, WIDTH / 8 * HEIGHT)
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
        _spi: &mut SPI,
        _delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        if let Some(refresh_lut) = refresh_rate {
            self.refresh = refresh_lut;
        }
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

    async fn set_ram_area(
        &mut self,
        spi: &mut SPI,
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
    ) -> Result<(), SPI::Error> {
        assert!(start_x < end_x);
        assert!(start_y < end_y);

        self.interface
            .cmd_with_data(
                spi,
                Command::SetRamXAddressStartEndPosition,
                &[(start_x >> 3) as u8, (end_x >> 3) as u8],
            )
            .await?;

        self.interface
            .cmd_with_data(
                spi,
                Command::SetRamYAddressStartEndPosition,
                &[
                    (start_y & 0xFF) as u8,
                    ((start_y >> 8) & 0x01) as u8,
                    (end_y & 0xFF) as u8,
                    ((end_y >> 8) & 0x01) as u8,
                ],
            )
            .await?;
        Ok(())
    }

    async fn set_ram_counter(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        x: u32,
        y: u32,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.interface
            .cmd_with_data(spi, Command::SetRamXAddressCounter, &[(x & 0xFF) as u8])
            .await?;

        self.interface
            .cmd_with_data(
                spi,
                Command::SetRamYAddressCounter,
                &[(y & 0xFF) as u8, ((y >> 8) & 0x01) as u8],
            )
            .await?;
        Ok(())
    }

    async fn use_full_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // choose full frame/ram
        self.set_ram_area(spi, 0, 0, WIDTH - 1, HEIGHT - 1).await?;

        // start from the beginning
        self.set_ram_counter(spi, delay, 0, 0).await
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
