//! Driver for the Waveshare 4.2" V2 E-Ink Display via SPI
//!
//! [Documentation](https://www.waveshare.com/wiki/4.2inch_e-Paper_Module_Manual)
//!
//! [Reference code](https://github.com/waveshareteam/e-Paper/blob/master/RaspberryPi_JetsonNano/c/lib/e-Paper/EPD_4in2_V2.c)

use crate::color::Color;
use crate::traits::RefreshLut;
use crate::{
    buffer_len,
    interface::DisplayInterface,
    traits::{InternalWiAdditions, WaveshareDisplay},
};
use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

pub(crate) mod command;
use self::command::Command;

const SINGLE_BYTE_WRITE: bool = true;

/// Default background color.
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
/// Display width in pixels.
pub const WIDTH: u32 = 400;
/// Display height in pixels.
pub const HEIGHT: u32 = 300;

const IS_BUSY_LOW: bool = false;

#[cfg(feature = "graphics")]
/// Full-size graphics buffer for the 4.2-inch EPD.
pub type Display4in2 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Driver for the Waveshare 4.2-inch V2 EPD.
pub struct Epd4in2<SPI, BUSY, DC, RST, DELAY> {
    /// SPI and control-pin interface.
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background color.
    background_color: Color,
    /// Refresh mode: full or quick.
    refresh: RefreshLut,
    /// Set after writing a partial frame so the next display activation uses
    /// the SSD1683 partial update control value (`0xFF`).
    partial_pending: bool,
}

enum DisplayMode {
    Full,
    Partial,
    Fast,
}

impl<SPI, BUSY, DC, RST, DELAY> Epd4in2<SPI, BUSY, DC, RST, DELAY>
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

    /// Changes the refresh mode and reinitializes the controller when necessary.
    pub fn set_refresh(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        refresh: RefreshLut,
    ) -> Result<(), SPI::Error> {
        if self.refresh != refresh {
            self.refresh = refresh;
            self.init(spi, delay)?;
        }
        Ok(())
    }

    fn set_full_ram_area(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        self.command(spi, Command::DataEntryModeSetting)?;
        self.send_data(spi, &[0x03])?;

        self.command(spi, Command::SetRamXAddressStartEnd)?;
        self.send_data(spi, &[0x00, ((WIDTH - 1) >> 3) as u8])?;

        self.command(spi, Command::SetRamYAddressStartEnd)?;
        self.send_data(
            spi,
            &[0x00, 0x00, (HEIGHT - 1) as u8, ((HEIGHT - 1) >> 8) as u8],
        )?;

        self.command(spi, Command::SetRamXAddressCounter)?;
        self.send_data(spi, &[0x00])?;

        self.command(spi, Command::SetRamYAddressCounter)?;
        self.send_data(spi, &[0x00, 0x00])
    }

    fn use_full_frame(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        self.command(spi, Command::DisplayUpdateControl1)?;
        self.send_data(spi, &[0x40, 0x00])?;

        self.command(spi, Command::BorderWaveformControl)?;
        self.send_data(spi, &[0x05])?;

        self.set_full_ram_area(spi)
    }

    fn use_fast_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::DisplayUpdateControl1)?;
        self.send_data(spi, &[0x40, 0x00])?;

        self.command(spi, Command::BorderWaveformControl)?;
        self.send_data(spi, &[0x05])?;

        self.command(spi, Command::WriteTemperatureRegister)?;
        self.send_data(spi, &[0x6e])?; // Waveshare 1.5-second fast mode

        self.command(spi, Command::DisplayUpdateControl2)?;
        self.send_data(spi, &[0x91])?;
        self.command(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)?;

        self.set_full_ram_area(spi)
    }

    fn turn_on_display(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        mode: DisplayMode,
    ) -> Result<(), SPI::Error> {
        self.command(spi, Command::DisplayUpdateControl2)?;

        let data = match mode {
            DisplayMode::Full => 0xf7,
            DisplayMode::Partial => 0xff,
            DisplayMode::Fast => 0xc7,
        };

        self.send_data(spi, &[data])?;
        self.command(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd4in2<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.reset(delay, 100_000, 2_000);
        self.wait_until_idle(spi, delay)?;
        self.command(spi, Command::SwReset)?;
        self.wait_until_idle(spi, delay)?;

        match self.refresh {
            RefreshLut::Full => self.use_full_frame(spi)?,
            RefreshLut::Quick => self.use_fast_frame(spi, delay)?,
        }

        self.wait_until_idle(spi, delay)?;
        self.partial_pending = false;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd4in2<SPI, BUSY, DC, RST, DELAY>
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
        let interface = DisplayInterface::new(busy, dc, rst, delay_us);
        let background_color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = Epd4in2 {
            interface,
            background_color,
            refresh: RefreshLut::Full,
            partial_pending: false,
        };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.command(spi, Command::DeepSleepMode)?;
        self.send_data(spi, &[1])?;
        delay.delay_ms(200);

        Ok(())
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.init(spi, delay)?;
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

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        assert_eq!(buffer.len(), buffer_len(WIDTH as usize, HEIGHT as usize));

        self.wait_until_idle(spi, delay)?;
        self.use_full_frame(spi)?;

        self.command(spi, Command::WriteRamBw)?;
        self.send_data(spi, buffer)?;

        self.command(spi, Command::WriteRamRed)?;
        self.send_data(spi, buffer)?;
        self.partial_pending = false;

        Ok(())
    }

    // The Waveshare reference driver does not define partial updates after fast initialization.
    fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), <SPI>::Error> {
        assert_eq!(
            self.refresh,
            RefreshLut::Full,
            "partial refresh requires RefreshLut::Full"
        );
        assert!(width > 0 && height > 0, "partial area must not be empty");
        assert!(x % 8 == 0, "x must be a multiple of 8");
        assert!(width % 8 == 0, "width must be a multiple of 8");
        assert!(
            width <= WIDTH && x <= WIDTH - width,
            "partial area exceeds display width"
        );
        assert!(
            height <= HEIGHT && y <= HEIGHT - height,
            "partial area exceeds display height"
        );
        assert_eq!(buffer.len(), (width / 8 * height) as usize);

        self.command(spi, Command::BorderWaveformControl)?;
        self.send_data(spi, &[0x80])?;

        self.command(spi, Command::DisplayUpdateControl1)?;
        self.send_data(spi, &[0x00, 0x00])?;

        self.command(spi, Command::BorderWaveformControl)?;
        self.send_data(spi, &[0x80])?;

        let x_start = (x >> 3) as u8;
        let x_end = ((x + width - 1) >> 3) as u8;
        let y_end = y + height - 1;

        self.command(spi, Command::SetRamXAddressStartEnd)?;
        self.send_data(spi, &[x_start, x_end])?;

        self.command(spi, Command::SetRamYAddressStartEnd)?;
        self.send_data(
            spi,
            &[y as u8, (y >> 8) as u8, y_end as u8, (y_end >> 8) as u8],
        )?;

        self.command(spi, Command::SetRamXAddressCounter)?;
        self.send_data(spi, &[x_start])?;

        self.command(spi, Command::SetRamYAddressCounter)?;
        self.send_data(spi, &[y as u8, (y >> 8) as u8])?;

        self.command(spi, Command::WriteRamBw)?;
        self.send_data(spi, buffer)?;
        self.partial_pending = true;

        Ok(())
    }

    // A pending partial update takes precedence over the configured refresh mode.
    // update_partial_frame ensures it can only be pending in Full mode.
    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        let mode = if self.partial_pending {
            DisplayMode::Partial
        } else {
            match self.refresh {
                RefreshLut::Full => DisplayMode::Full,
                RefreshLut::Quick => DisplayMode::Fast,
            }
        };

        self.turn_on_display(spi, delay, mode)?;
        self.partial_pending = false;
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
        const SIZE: u32 = WIDTH / 8 * HEIGHT;

        self.wait_until_idle(spi, delay)?;
        self.use_full_frame(spi)?;

        self.command(spi, Command::WriteRamBw)?;
        self.interface.data_x_times(spi, 0xff, SIZE)?;

        self.command(spi, Command::WriteRamRed)?;
        self.interface.data_x_times(spi, 0xff, SIZE)?;

        self.partial_pending = false;
        self.display_frame(spi, delay)?;
        Ok(())
    }

    fn set_lut(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), <SPI>::Error> {
        if let Some(refresh) = refresh_rate {
            self.set_refresh(spi, delay, refresh)
        } else {
            self.init(spi, delay)
        }
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 400);
        assert_eq!(HEIGHT, 300);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }
}
