//! A simple Driver for the Waveshare 1.54" E-Ink Display via SPI
//!
//! GDEH0154D67

/// Width of the display
pub const WIDTH: u32 = 200;
/// Height of the display
pub const HEIGHT: u32 = 200;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const IS_BUSY_LOW: bool = false;
const SINGLE_BYTE_WRITE: bool = true;

use embedded_hal::digital::*;

use crate::type_a::command::Command;

mod constants;
use crate::epd1in54_v2::constants::{LUT_FULL_UPDATE, LUT_PARTIAL_UPDATE};

use crate::color::Color;

use crate::traits::{RefreshLut, WaveshareDisplay};

use crate::interface::{DelayNs, DisplayInterface, SpiDevice};

#[cfg(feature = "graphics")]
pub use crate::epd1in54::Display1in54;

/// Epd1in54 driver
pub struct Epd1in54<SPI, BUSY, DC, RST, DELAY> {
    /// SPI
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Color
    background_color: Color,

    /// Refresh LUT
    refresh: RefreshLut,
}

#[maybe_async::maybe_async]
impl<SPI, BUSY, DC, RST, DELAY> Epd1in54<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.reset(delay, 10_000, 10_000).await;
        self.wait_until_idle(spi, delay).await?;
        self.interface.cmd(spi, Command::SwReset).await?;
        self.wait_until_idle(spi, delay).await?;

        // 3 Databytes:
        // A[7:0]
        // 0.. A[8]
        // 0.. B[2:0]
        // Default Values: A = Height of Screen (0x127), B = 0x00 (GD, SM and TB=0?)
        self.interface
            .cmd_with_data(
                spi,
                Command::DriverOutputControl,
                &[(HEIGHT - 1) as u8, 0x0, 0x00],
            )
            .await?;

        self.interface
            .cmd_with_data(spi, Command::DataEntryModeSetting, &[0x3])
            .await?;

        self.set_ram_area(spi, delay, 0, 0, WIDTH - 1, HEIGHT - 1)
            .await?;

        self.interface
            .cmd_with_data(
                spi,
                Command::TemperatureSensorSelection,
                &[0x80], // 0x80: internal temperature sensor
            )
            .await?;

        self.interface
            .cmd_with_data(spi, Command::TemperatureSensorControl, &[0xB1, 0x20])
            .await?;

        self.set_ram_counter(spi, delay, 0, 0).await?;

        //Initialize the lookup table with a refresh waveform
        self.set_lut(spi, delay, None).await?;

        self.wait_until_idle(spi, delay).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd1in54<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    type DisplayColor = Color;
    fn width(&self) -> u32 {
        WIDTH
    }

    fn height(&self) -> u32 {
        HEIGHT
    }

    async fn new(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
    ) -> Result<Self, SPI::Error> {
        let interface = DisplayInterface::new(busy, dc, rst, delay_us);

        let mut epd = Epd1in54 {
            interface,
            background_color: DEFAULT_BACKGROUND_COLOR,
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

    //TODO: update description: last 3 bits will be ignored for width and x_pos
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
        self.set_ram_area(spi, delay, x, y, x + width, y + height)
            .await?;
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
                .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xC7])
                .await?;
        } else if self.refresh == RefreshLut::Quick {
            self.interface
                .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xCF])
                .await?;
        }

        self.interface.cmd(spi, Command::MasterActivation).await?;
        // MASTER Activation should not be interupted to avoid currption of panel images
        // therefore a terminate command is send
        self.interface.cmd(spi, Command::Nop).await?;
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

        // clear the ram with the background color
        let color = self.background_color.get_byte_value();

        self.interface.cmd(spi, Command::WriteRam).await?;
        self.interface
            .data_x_times(spi, color, WIDTH / 8 * HEIGHT)
            .await?;
        self.interface.cmd(spi, Command::WriteRam2).await?;
        self.interface
            .data_x_times(spi, color, WIDTH / 8 * HEIGHT)
            .await?;
        Ok(())
    }

    fn set_background_color(&mut self, background_color: Color) {
        self.background_color = background_color;
    }

    fn background_color(&self) -> &Color {
        &self.background_color
    }

    async fn set_lut(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        if let Some(refresh_lut) = refresh_rate {
            self.refresh = refresh_lut;
        }
        match self.refresh {
            RefreshLut::Full => self.set_lut_helper(spi, delay, &LUT_FULL_UPDATE),
            RefreshLut::Quick => self.set_lut_helper(spi, delay, &LUT_PARTIAL_UPDATE),
        }
        .await?;

        // Additional configuration required only for partial updates
        if self.refresh == RefreshLut::Quick {
            self.interface
                .cmd_with_data(
                    spi,
                    Command::WriteOtpSelection,
                    &[0x0, 0x0, 0x0, 0x0, 0x0, 0x40, 0x0, 0x0, 0x0, 0x0],
                )
                .await?;
            self.interface
                .cmd_with_data(spi, Command::BorderWaveformControl, &[0x80])
                .await?;
            self.interface
                .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xc0])
                .await?;
            self.interface.cmd(spi, Command::MasterActivation).await?;
            // MASTER Activation should not be interupted to avoid currption of panel images
            // therefore a terminate command is send
            self.interface.cmd(spi, Command::Nop).await?;
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
impl<SPI, BUSY, DC, RST, DELAY> Epd1in54<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    pub(crate) async fn use_full_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        // choose full frame/ram
        self.set_ram_area(spi, delay, 0, 0, WIDTH - 1, HEIGHT - 1)
            .await?;

        // start from the beginning
        self.set_ram_counter(spi, delay, 0, 0).await
    }

    pub(crate) async fn set_ram_area(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        assert!(start_x < end_x);
        assert!(start_y < end_y);

        // x is positioned in bytes, so the last 3 bits which show the position inside a byte in the ram
        // aren't relevant
        self.interface
            .cmd_with_data(
                spi,
                Command::SetRamXAddressStartEndPosition,
                &[(start_x >> 3) as u8, (end_x >> 3) as u8],
            )
            .await?;

        // 2 Databytes: A[7:0] & 0..A[8] for each - start and end
        self.interface
            .cmd_with_data(
                spi,
                Command::SetRamYAddressStartEndPosition,
                &[
                    start_y as u8,
                    (start_y >> 8) as u8,
                    end_y as u8,
                    (end_y >> 8) as u8,
                ],
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn set_ram_counter(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        x: u32,
        y: u32,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        // x is positioned in bytes, so the last 3 bits which show the position inside a byte in the ram
        // aren't relevant
        self.interface
            .cmd_with_data(spi, Command::SetRamXAddressCounter, &[(x >> 3) as u8])
            .await?;

        // 2 Databytes: A[7:0] & 0..A[8]
        self.interface
            .cmd_with_data(
                spi,
                Command::SetRamYAddressCounter,
                &[y as u8, (y >> 8) as u8],
            )
            .await?;
        Ok(())
    }

    async fn set_lut_helper(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        assert!(buffer.len() == 159);

        self.interface
            .cmd_with_data(spi, Command::WriteLutRegister, &buffer[0..153])
            .await?;

        self.interface
            .cmd_with_data(spi, Command::WriteLutRegisterEnd, &[buffer[153]])
            .await?;

        self.wait_until_idle(spi, delay).await?;

        self.interface
            .cmd_with_data(spi, Command::GateDrivingVoltage, &[buffer[154]])
            .await?;

        self.interface
            .cmd_with_data(
                spi,
                Command::SourceDrivingVoltage,
                &[buffer[155], buffer[156], buffer[157]],
            )
            .await?;
        self.interface
            .cmd_with_data(spi, Command::WriteVcomRegister, &[buffer[158]])
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 200);
        assert_eq!(HEIGHT, 200);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }
}
