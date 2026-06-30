//! A simple Driver for the Waveshare 3.97" E-Ink Display via SPI
//!
//!
//! Build with documentation from [Waveshare](https://www.waveshare.com/wiki/3.97inch_e-Paper_HAT+_Manual),
//! Oriented on their provided source code [github](https://github.com/waveshareteam/e-Paper/tree/master/E-paper_Separate_Program/3in97_e-Paper)
//!
//! Partial refresh and 4-Grey scale is not yet implemented but I included the fast_init Sequence.
//! The lut seems to be baked into the HAT+ so I did not yet see a reason to implement set_lut(),
//! neither can I provide a lut as it doesn't seem to be provided.
//! It is tested on an esp32c5.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal::spi::SpiDevice;

use crate::color::Color;
use crate::interface::DisplayInterface;
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Width of the display
pub const WIDTH: u32 = 800;
/// Height if the display
pub const HEIGHT: u32 = 480;

/// default background color (white)
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const SINGLE_BYTE_WRITE: bool = true;

///Display with full size buffer for the 3in97 epd
#[cfg(feature = "graphics")]
pub type Display3in97 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// 3in97 driver
pub struct Epd3in97<SPI, BUSY, DC, RST, DELAY> {
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    color: Color,
    refresh: RefreshLut,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd3in97<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        // delay in microseconds not miliseconds -> 10^-6
        // so 20 ms is 20_000 micros
        self.interface.reset(delay, 20_000, 2_000);

        // READ BUSY
        self.interface.wait_until_idle(delay, false);
        self.interface.cmd(spi, Command::SwReset)?;
        self.interface.wait_until_idle(delay, false);

        // select internal Temperature sensor Temperature Control
        self.interface
            .cmd_with_data(spi, Command::TemperatureSensorControl, &[0x80])?;

        // Booster Soft-start Control (not really documented)
        self.interface.cmd_with_data(
            spi,
            Command::BoosterSoftStart,
            &[0xAE, 0xC7, 0xC3, 0xC0, 0x80],
        )?;
        // splitting 10-Bit number across 2 Bytes
        self.interface.cmd_with_data(
            spi,
            Command::GateSettings,
            &[((HEIGHT - 1) % 256) as u8, ((HEIGHT - 1) / 256) as u8, 0x02],
        )?;

        self.interface
            .cmd_with_data(spi, Command::BorderWaveform, &[0x01])?;
        self.interface
            .cmd_with_data(spi, Command::DataEntryMode, &[0x01])?;

        self.interface.cmd_with_data(
            spi,
            Command::SetRamXAdressStartEndPosition,
            &[
                0x00,
                0x00,
                ((WIDTH - 1) % 256) as u8,
                ((WIDTH - 1) / 256) as u8,
            ],
        )?;
        self.interface.cmd_with_data(
            spi,
            Command::SetRamYAdressStartEndPosition,
            &[
                ((HEIGHT - 1) % 256) as u8,
                ((HEIGHT - 1) / 256) as u8,
                0x00,
                0x00,
            ],
        )?;
        self.interface
            .cmd_with_data(spi, Command::SetRamXAdressCounter, &[0x00, 0x00])?;
        self.interface
            .cmd_with_data(spi, Command::SetRamYAdressCounter, &[0x00, 0x00])?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd3in97<SPI, BUSY, DC, RST, DELAY>
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
    ) -> Result<Self, <SPI>::Error>
    where
        Self: Sized,
    {
        let mut epd = Epd3in97 {
            interface: DisplayInterface::new(busy, dc, rst, delay_us),
            color: DEFAULT_BACKGROUND_COLOR,
            refresh: RefreshLut::Full,
        };

        epd.init(spi, delay)?;
        Ok(epd)
    }
    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.wait_until_idle(spi, delay);
        self.interface
            .cmd_with_data(spi, Command::DeepSleep, &[0x01])?;
        Ok(())
    }
    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
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
    ) -> Result<(), <SPI>::Error> {
        self.wait_until_idle(spi, delay)?;
        self.interface
            .cmd_with_data(spi, Command::WriteRamBw, buffer)
    }

    #[allow(unused)]
    fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), <SPI>::Error> {
        todo!()
    }
    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.wait_until_idle(spi, delay)?;

        self.interface
            .cmd_with_data(spi, Command::DisplayUpdateControl2, &[0xF7])?;
        self.interface.cmd(spi, Command::DisplayUpdateSequence)?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }
    fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), <SPI>::Error> {
        self.update_frame(spi, buffer, delay)?;
        self.display_frame(spi, delay)?;
        Ok(())
    }
    fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.wait_until_idle(spi, delay);
        let width = WIDTH / 8;
        let height = HEIGHT;

        let color = self.color.get_byte_value();

        self.interface.cmd(spi, Command::WriteRamBw)?;
        self.interface.data_x_times(spi, color, width * height)?;
        Ok(())
    }
    #[allow(unused)]
    fn set_lut(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), <SPI>::Error> {
        todo!()
    }
    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.interface.wait_until_idle(delay, false);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd3in97<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn fast_init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        // delay in microseconds not miliseconds -> 10^-6
        // so 20 ms is 20_000 micros
        self.interface.reset(delay, 20_000, 2_000);

        // READ BUSY
        self.interface.wait_until_idle(delay, false);
        self.interface.cmd(spi, Command::SwReset)?;
        self.interface.wait_until_idle(delay, false);

        //no temperature control

        // Booster Soft-start Control (not really documented)
        // same as init
        self.interface.cmd_with_data(
            spi,
            Command::BoosterSoftStart,
            &[0xAE, 0xC7, 0xC3, 0xC0, 0x80],
        )?;
        // splitting 10-Bit number across 2 Bytes
        self.interface.cmd_with_data(
            spi,
            Command::GateSettings,
            &[((HEIGHT - 1) % 256) as u8, ((HEIGHT - 1) / 256) as u8, 0x02],
        )?;

        self.interface
            .cmd_with_data(spi, Command::DataEntryMode, &[0x01])?;

        self.interface.cmd_with_data(
            spi,
            Command::SetRamXAdressStartEndPosition,
            &[
                0x00,
                0x00,
                ((WIDTH - 1) % 256) as u8,
                ((WIDTH - 1) / 256) as u8,
            ],
        )?;
        self.interface.cmd_with_data(
            spi,
            Command::SetRamYAdressStartEndPosition,
            &[
                ((HEIGHT - 1) % 256) as u8,
                ((HEIGHT - 1) / 256) as u8,
                0x00,
                0x00,
            ],
        )?;
        self.interface
            .cmd_with_data(spi, Command::SetRamXAdressCounter, &[0x00, 0x00])?;
        self.interface
            .cmd_with_data(spi, Command::SetRamYAdressCounter, &[0x00, 0x00])?;

        // border waveform
        self.interface
            .cmd_with_data(spi, Command::BorderWaveform, &[0x01])?;

        //temperature
        self.interface
            .cmd_with_data(spi, Command::TemperatureSensorControl, &[0x80]);

        self.interface
            .cmd_with_data(spi, Command::TemperatureSensorControlWrite, &[0x6A])?;
        Ok(())
    }
}
