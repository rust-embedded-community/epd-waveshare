//! A simple Driver for the Waveshare 2.9" B (v4) Tri-Color E-Ink Display via SPI
//!
//! [Documentation](https://www.waveshare.com/wiki/2.9inch_e-Paper_Module_(B)_Manual)
//!
//! [Reference code](https://github.com/waveshareteam/e-Paper/blob/master/RaspberryPi_JetsonNano/c/lib/e-Paper/EPD_2in9b_V4.c)

use crate::{
    buffer_len,
    color::TriColor,
    interface::{DelayNs, DisplayInterface, SpiDevice},
    traits::{InternalWiAdditions, WaveshareDisplay, WaveshareThreeColorDisplay},
};
use embedded_hal::digital::{InputPin, OutputPin};

pub(crate) mod command;
use self::command::Command;

const SINGLE_BYTE_WRITE: bool = false;

/// Default Background Color (white)
pub const DEFAULT_BACKGROUND_COLOR: TriColor = TriColor::White;
/// Width of Epd2in9b in pixels
pub const WIDTH: u32 = 128;
/// HEIGHT of Epd2in9b in pixels
pub const HEIGHT: u32 = 296;

const IS_BUSY_LOW: bool = false;

#[cfg(feature = "graphics")]
/// Full size buffer for use with the 2.9" black/red EPD
pub type Display2in9b = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    true,
    { buffer_len(WIDTH as usize, HEIGHT as usize * 2) },
    TriColor,
>;

/// Epd2in9b (v4) driver
pub struct Epd2in9b<SPI, BUSY, DC, RST, DELAY> {
    /// SPI
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Color
    background_color: TriColor,
}

#[allow(dead_code)]
enum DisplayMode {
    Default,
    Partial,
    Fast, // TODO: Add support in future
    Base,
}

#[maybe_async::maybe_async]
impl<SPI, BUSY, DC, RST, DELAY> Epd2in9b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    /// set the base image before partially update
    ///
    /// <https://github.com/waveshareteam/e-Paper/blob/bc23f8ee814486edb6a364c802847224e079e523/RaspberryPi_JetsonNano/c/examples/EPD_2in9b_V4_test.c#L130>
    pub async fn update_and_display_frame_base(
        &mut self,
        spi: &mut SPI,
        black: &[u8],
        chromatic: Option<&[u8]>,
        delay: &mut DELAY,
    ) -> Result<(), <SPI>::Error> {
        self.update_frame(spi, black, delay).await?;
        if let Some(chromatic) = chromatic {
            self.update_chromatic_frame(spi, delay, chromatic).await?;
        }

        self.turn_on_display(spi, delay, DisplayMode::Base).await?;

        self.command(spi, Command::WriteRedData).await?;
        self.send_data(spi, black).await?;

        Ok(())
    }

    /// display frame partially
    ///
    /// To perform partial update, it need to call update_and_display_frame_base
    /// than call update_partial_frame before call display_frame_partial
    pub async fn display_frame_partial(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<(), <SPI>::Error> {
        self.turn_on_display(spi, delay, DisplayMode::Partial)
            .await?;
        Ok(())
    }

    async fn command(&mut self, spi: &mut SPI, command: Command) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, command).await
    }

    async fn send_data(&mut self, spi: &mut SPI, data: &[u8]) -> Result<(), SPI::Error> {
        self.interface.data(spi, data).await
    }

    async fn turn_on_display(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        mode: DisplayMode,
    ) -> Result<(), SPI::Error> {
        self.command(spi, Command::TurnOnDisplay).await?;

        let data = match mode {
            DisplayMode::Default => 0xf7,
            DisplayMode::Partial => 0x1c,
            DisplayMode::Fast => 0xc7,
            DisplayMode::Base => 0xf4,
        };

        self.send_data(spi, &[data]).await?;
        self.command(spi, Command::ActivateDisplayUpdateSequence)
            .await?;
        self.wait_until_idle(spi, delay).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in9b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        let w = self.width();
        let h = self.height();

        self.interface.reset(delay, 200_000, 2_000).await;

        self.wait_until_idle(spi, delay).await?;
        self.command(spi, Command::SwReset).await?;
        self.wait_until_idle(spi, delay).await?;

        self.command(spi, Command::DriverOutputControl).await?;
        self.send_data(spi, &[((h - 1) % 256) as u8]).await?;
        self.send_data(spi, &[((h - 1) / 256) as u8]).await?;
        self.send_data(spi, &[0]).await?;

        self.command(spi, Command::DataEntryMode).await?;
        self.send_data(spi, &[0x03]).await?;

        self.command(spi, Command::RamXPosition).await?;
        self.send_data(spi, &[0]).await?;
        self.send_data(spi, &[(w / 8 - 1) as u8]).await?;

        self.command(spi, Command::RamYPosition).await?;
        self.send_data(spi, &[0]).await?;
        self.send_data(spi, &[0]).await?;
        self.send_data(spi, &[((h - 1) % 256) as u8]).await?;
        self.send_data(spi, &[((h - 1) / 256) as u8]).await?;

        self.command(spi, Command::BorderWavefrom).await?;
        self.send_data(spi, &[0x05]).await?;

        self.command(spi, Command::DisplayUpdateControl).await?;
        self.send_data(spi, &[0x00]).await?;
        self.send_data(spi, &[0x80]).await?;

        self.command(spi, Command::ReadBuiltInTemperatureSensor)
            .await?;
        self.send_data(spi, &[0x80]).await?;

        self.command(spi, Command::RamXAddressCount).await?;
        self.send_data(spi, &[0x00]).await?; // set RAM x address count to 0
        self.command(spi, Command::RamYAddressCount).await?; // set RAM y address count to 0X199
        self.send_data(spi, &[0x00]).await?;
        self.send_data(spi, &[0x00]).await?;

        self.wait_until_idle(spi, delay).await?;

        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareThreeColorDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in9b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn update_color_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        black: &[u8],
        chromatic: &[u8],
    ) -> Result<(), <SPI>::Error> {
        self.update_achromatic_frame(spi, delay, black).await?;
        self.update_chromatic_frame(spi, delay, chromatic).await?;
        Ok(())
    }

    async fn update_achromatic_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        black: &[u8],
    ) -> Result<(), <SPI>::Error> {
        self.command(spi, Command::WriteBlackData).await?;
        self.send_data(spi, black).await?;
        Ok(())
    }

    async fn update_chromatic_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        chromatic: &[u8],
    ) -> Result<(), <SPI>::Error> {
        self.command(spi, Command::WriteRedData).await?;
        self.send_data(spi, chromatic).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in9b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    type DisplayColor = TriColor;

    async fn new(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
    ) -> Result<Self, <SPI>::Error>
    where
        Self: Sized,
    {
        let interface = DisplayInterface::new(busy, dc, rst, delay_us);
        let background_color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = Epd2in9b {
            interface,
            background_color,
        };

        epd.init(spi, delay).await?;

        Ok(epd)
    }

    async fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.command(spi, Command::DeepSleep).await?;
        self.send_data(spi, &[1]).await?;
        delay.delay_ms(100).await;

        Ok(())
    }

    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.init(spi, delay).await?;
        Ok(())
    }

    fn set_background_color(&mut self, color: Self::DisplayColor) {
        self.background_color = color
    }

    fn background_color(&self) -> &Self::DisplayColor {
        &self.background_color
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
        _delay: &mut DELAY,
    ) -> Result<(), <SPI>::Error> {
        self.command(spi, Command::WriteBlackData).await?;
        self.send_data(spi, buffer).await?;

        self.command(spi, Command::WriteRedData).await?;
        self.interface
            .data_x_times(spi, 0x00, WIDTH / 8 * HEIGHT)
            .await?;
        Ok(())
    }

    async fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), <SPI>::Error> {
        assert!(width % 8 == 0, "width must multiple of 8");
        let mut x_start = x;
        let mut x_end = x + width;

        let y_start = y;
        let mut y_end = y + height;

        if (x_start % 8 + x_end % 8 == 8 && x_start % 8 > x_end % 8)
            || x_start % 8 + x_end % 8 == 0
            || (x_end - x_start) % 8 == 0
        {
            x_start /= 8;
            x_end /= 8;
        } else {
            x_start /= 8;
            x_end = if x_end % 8 == 0 {
                x_end / 8
            } else {
                x_end / 8 + 1
            };
        }

        x_end -= 1;
        y_end -= 1;

        let x_start = x_start as u8;
        let x_end = x_end as u8;

        let y_start_1 = y_start as u8;
        let y_start_2 = (y_start >> 8) as u8;

        let y_end_1 = y_end as u8;
        let y_end_2 = (y_end >> 8) as u8;

        self.command(spi, Command::RamXPosition).await?;
        self.send_data(spi, &[x_start, x_end]).await?;
        self.command(spi, Command::RamYPosition).await?;
        self.send_data(spi, &[y_start_1, y_start_2]).await?;
        self.send_data(spi, &[y_end_1, y_end_2]).await?;

        self.command(spi, Command::RamXAddressCount).await?;
        self.send_data(spi, &[x_start]).await?;
        self.command(spi, Command::RamYAddressCount).await?;
        self.send_data(spi, &[y_start_1, y_start_2]).await?;

        self.command(spi, Command::WriteBlackData).await?;
        self.send_data(spi, buffer).await?;

        Ok(())
    }

    async fn display_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<(), <SPI>::Error> {
        self.turn_on_display(spi, delay, DisplayMode::Default)
            .await?;

        Ok(())
    }

    async fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), <SPI>::Error> {
        self.update_frame(spi, buffer, delay).await?;
        self.display_frame(spi, delay).await?;

        Ok(())
    }

    async fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        const SIZE: u32 = WIDTH / 8 * HEIGHT;

        self.command(spi, Command::WriteBlackData).await?;
        self.interface.data_x_times(spi, 0xff, SIZE).await?;

        self.command(spi, Command::WriteRedData).await?;
        self.interface.data_x_times(spi, 0, SIZE).await?;

        self.display_frame(spi, delay).await?;
        Ok(())
    }

    async fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _refresh_rate: Option<crate::traits::RefreshLut>,
    ) -> Result<(), <SPI>::Error> {
        Ok(())
    }

    async fn wait_until_idle(
        &mut self,
        _spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<(), <SPI>::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW).await;
        Ok(())
    }
}
