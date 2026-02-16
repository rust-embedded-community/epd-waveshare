//! A simple Driver for the Waveshare 3.7" E-Ink Display via SPI
//!
//!
//! Build with the help of documentation/code from [Waveshare](https://www.waveshare.com/wiki/3.7inch_e-Paper_HAT),
use embedded_hal::digital::{InputPin, OutputPin};

pub(crate) mod command;
mod constants;

use self::command::Command;
use self::constants::*;

use crate::buffer_len;
use crate::color::Color;
use crate::interface::{DelayNs, DisplayInterface, SpiDevice};
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

/// Width of the display.
pub const WIDTH: u32 = 280;

/// Height of the display
pub const HEIGHT: u32 = 480;

/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;

const IS_BUSY_LOW: bool = false;

const SINGLE_BYTE_WRITE: bool = true;

/// Display with Fullsize buffer for use with the 3in7 EPD
#[cfg(feature = "graphics")]
pub type Display3in7 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// EPD3in7 driver
pub struct EPD3in7<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    background_color: Color,
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for EPD3in7<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // reset the device
        self.interface.reset(delay, 30, 10).await;

        self.interface.cmd(spi, Command::SwReset).await?;
        delay.delay_us(300000u32).await;

        self.interface
            .cmd_with_data(spi, Command::AutoWriteRedRamRegularPattern, &[0xF7])
            .await?;
        self.interface.wait_until_idle(delay, IS_BUSY_LOW).await;
        self.interface
            .cmd_with_data(spi, Command::AutoWriteBwRamRegularPattern, &[0xF7])
            .await?;
        self.interface.wait_until_idle(delay, IS_BUSY_LOW).await;

        self.interface
            .cmd_with_data(spi, Command::GateSetting, &[0xDF, 0x01, 0x00])
            .await?;
        self.interface
            .cmd_with_data(spi, Command::GateVoltage, &[0x00])
            .await?;
        self.interface
            .cmd_with_data(spi, Command::GateVoltageSource, &[0x41, 0xA8, 0x32])
            .await?;

        self.interface
            .cmd_with_data(spi, Command::DataEntrySequence, &[0x03])
            .await?;

        self.interface
            .cmd_with_data(spi, Command::BorderWaveformControl, &[0x03])
            .await?;

        self.interface
            .cmd_with_data(
                spi,
                Command::BoosterSoftStartControl,
                &[0xAE, 0xC7, 0xC3, 0xC0, 0xC0],
            )
            .await?;

        self.interface
            .cmd_with_data(spi, Command::TemperatureSensorSelection, &[0x80])
            .await?;

        self.interface
            .cmd_with_data(spi, Command::WriteVcomRegister, &[0x44])
            .await?;

        self.interface
            .cmd_with_data(
                spi,
                Command::DisplayOption,
                &[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x4F, 0xFF, 0xFF, 0xFF, 0xFF],
            )
            .await?;

        self.interface
            .cmd_with_data(
                spi,
                Command::SetRamXAddressStartEndPosition,
                &[0x00, 0x00, 0x17, 0x01],
            )
            .await?;
        self.interface
            .cmd_with_data(
                spi,
                Command::SetRamYAddressStartEndPosition,
                &[0x00, 0x00, 0xDF, 0x01],
            )
            .await?;

        self.interface
            .cmd_with_data(spi, Command::DisplayUpdateSequenceSetting, &[0xCF])
            .await?;

        self.set_lut(spi, delay, Some(RefreshLut::Full)).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for EPD3in7<SPI, BUSY, DC, RST, DELAY>
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
        let mut epd = EPD3in7 {
            interface: DisplayInterface::new(busy, dc, rst, delay_us),
            background_color: DEFAULT_BACKGROUND_COLOR,
        };

        epd.init(spi, delay).await?;
        Ok(epd)
    }

    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay).await
    }

    async fn sleep(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface
            .cmd_with_data(spi, Command::Sleep, &[0xF7])
            .await?;
        self.interface.cmd(spi, Command::PowerOff).await?;
        self.interface
            .cmd_with_data(spi, Command::Sleep2, &[0xA5])
            .await?;
        Ok(())
    }

    fn set_background_color(&mut self, color: Self::DisplayColor) {
        self.background_color = color;
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
    ) -> Result<(), SPI::Error> {
        assert!(buffer.len() == buffer_len(WIDTH as usize, HEIGHT as usize));
        self.interface
            .cmd_with_data(spi, Command::SetRamXAddressCounter, &[0x00, 0x00])
            .await?;
        self.interface
            .cmd_with_data(spi, Command::SetRamYAddressCounter, &[0x00, 0x00])
            .await?;

        self.interface
            .cmd_with_data(spi, Command::WriteRam, buffer)
            .await?;

        Ok(())
    }

    #[allow(unused)]
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
        todo!()
    }

    async fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        //self.interface
        //    .cmd_with_data(spi, Command::WRITE_LUT_REGISTER, &LUT_1GRAY_GC)?;
        self.interface
            .cmd(spi, Command::DisplayUpdateSequence)
            .await?;
        self.interface.wait_until_idle(delay, IS_BUSY_LOW).await;
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

    async fn clear_frame(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface
            .cmd_with_data(spi, Command::SetRamXAddressCounter, &[0x00, 0x00])
            .await?;
        self.interface
            .cmd_with_data(spi, Command::SetRamYAddressCounter, &[0x00, 0x00])
            .await?;

        let color = self.background_color.get_byte_value();
        self.interface.cmd(spi, Command::WriteRam).await?;
        self.interface
            .data_x_times(spi, color, WIDTH * HEIGHT)
            .await?;

        Ok(())
    }

    async fn set_lut(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        let buffer = match refresh_rate {
            Some(RefreshLut::Full) | None => &LUT_1GRAY_GC,
            Some(RefreshLut::Quick) => &LUT_1GRAY_DU,
        };

        self.interface
            .cmd_with_data(spi, Command::WriteLutRegister, buffer)
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
