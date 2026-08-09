//! A simple Driver for the Waveshare 7.3inch e-Paper HAT (E) Display via SPI
//!
//! # References
//!
//! - [Wiki](https://www.waveshare.com/wiki/7.3inch_e-Paper_HAT_(E))
//! - [Waveshare C driver](https://github.com/waveshareteam/e-Paper/blob/master/Arduino_R4/src/e-Paper/EPD_7in3e.cpp)
//! - [Waveshare Python driver](https://github.com/waveshareteam/e-Paper/blob/master/RaspberryPi_JetsonNano/python/lib/waveshare_epd/epd7in3e.py)
//! - [Display datasheet](https://files.seeedstudio.com/wiki/Other_Display/7.3-E6-color/7.3_E6_eInk_%20Display_%20module_%20Datasheet.pdf)
//! - [Controller datasheet](https://cursedhardware.github.io/epd-driver-ic/UC8159c.pdf) (unconfirmed!)

use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::{
    buffer_len,
    color::HexColor,
    interface::DisplayInterface,
    traits::{InternalWiAdditions, WaveshareDisplay},
};

use self::command::Command;

mod command;

/// Full size buffer for use with the 7in3f EPD
#[cfg(feature = "graphics")]
pub type Display7in3e = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize * 4) },
    HexColor,
>;

/// Width of the display
pub const WIDTH: u32 = 800;
/// Height of the display
pub const HEIGHT: u32 = 480;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: HexColor = HexColor::White;
/// Default mode of writing data (single byte vs blockwise)
const SINGLE_BYTE_WRITE: bool = true;

/// Epd57n3f driver
pub struct Epd7in3e<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: HexColor,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd7in3e<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.interface.reset(delay, 20_000, 2_000);
        self.wait_busy_low(delay);
        delay.delay_ms(30);

        self.cmd_with_data(spi, Command::CMDH, &[0x49, 0x55, 0x20, 0x08, 0x09, 0x18])?;
        self.cmd_with_data(spi, Command::Ox01, &[0x3F])?;
        self.cmd_with_data(spi, Command::Ox00, &[0x5F, 0x69])?;
        self.cmd_with_data(spi, Command::Ox03, &[0x00, 0x54, 0x00, 0x44])?;
        self.cmd_with_data(spi, Command::Ox05, &[0x40, 0x1F, 0x1F, 0x2C])?;
        self.cmd_with_data(spi, Command::Ox06, &[0x6F, 0x1F, 0x17, 0x49])?;
        self.cmd_with_data(spi, Command::Ox08, &[0x6F, 0x1F, 0x1F, 0x22])?;
        self.cmd_with_data(spi, Command::Ox30, &[0x03])?;
        self.cmd_with_data(spi, Command::Ox50, &[0x3F])?;
        self.cmd_with_data(spi, Command::Ox60, &[0x02, 0x00])?;
        self.cmd_with_data(spi, Command::Ox61, &[0x03, 0x20, 0x01, 0xE0])?;
        self.cmd_with_data(spi, Command::Ox84, &[0x01])?;
        self.cmd_with_data(spi, Command::OxE3, &[0x2F])?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd7in3e<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    type DisplayColor = HexColor;

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
        let color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = Epd7in3e { interface, color };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn sleep(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.cmd_with_data(spi, Command::DeepSleep, &[0xA5])
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
        self.cmd_with_data(spi, Command::DataStartTransmission, buffer)
    }

    fn update_partial_frame(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _buffer: &[u8],
        _x: u32,
        _y: u32,
        _width: u32,
        _height: u32,
    ) -> Result<(), <SPI>::Error> {
        unimplemented!()
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.command(spi, Command::PowerOn)?;
        self.wait_busy_low(delay);

        self.cmd_with_data(spi, Command::DataRefresh, &[0x00])?;
        self.wait_busy_low(delay);

        self.cmd_with_data(spi, Command::PowerOff, &[0x00])?;
        self.wait_busy_low(delay);

        Ok(())
    }

    fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), <SPI>::Error> {
        self.update_frame(spi, buffer, delay)?;
        self.display_frame(spi, delay)
    }

    fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        let bg = HexColor::colors_byte(self.color, self.color);

        self.wait_busy_low(delay);
        self.command(spi, Command::DataStartTransmission)?;
        self.interface.data_x_times(spi, bg, WIDTH * HEIGHT / 2)?;

        self.display_frame(spi, delay)
    }

    fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _refresh_rate: Option<crate::traits::RefreshLut>,
    ) -> Result<(), <SPI>::Error> {
        unimplemented!()
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.wait_busy_low(delay);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd7in3e<SPI, BUSY, DC, RST, DELAY>
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

    fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data)
    }

    fn wait_busy_low(&mut self, delay: &mut DELAY) {
        self.interface.wait_until_idle(delay, true);
    }

    /// Show 6 blocks of color, used for quick testing
    pub fn show_6block(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        let color_6 = [
            HexColor::Black,
            HexColor::White,
            HexColor::Yellow,
            HexColor::Red,
            HexColor::Blue,
            HexColor::Green,
        ];

        self.command(spi, Command::DataStartTransmission)?;
        for color in color_6 {
            for _y in 0..80 {
                for _x in 0..400 {
                    self.interface
                        .data(spi, &[HexColor::colors_byte(color, color)])?;
                }
            }
        }

        self.display_frame(spi, delay)
    }
}
