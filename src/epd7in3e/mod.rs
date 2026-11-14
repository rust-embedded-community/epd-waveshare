//! A simple Driver for the Waveshare 7.3inch e-Paper HAT (F) Display via SPI
//!
//! # References
//!
//! - [Datasheet](https://www.waveshare.com/wiki/7.3inch_e-Paper_HAT_(E))
//! - [Waveshare C driver](https://github.com/waveshareteam/e-Paper/blob/af0c7dc2b04220b04dae75838cff47f9046d49b5/RaspberryPi_JetsonNano/c/lib/e-Paper/EPD_7in3e.c)
//! - [Waveshare Python driver](https://github.com/waveshareteam/e-Paper/blob/af0c7dc2b04220b04dae75838cff47f9046d49b5/RaspberryPi_JetsonNano/python/lib/waveshare_epd/epd7in3e.py)

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

/// Full size buffer for use with the 7in3e EPD
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
        self.interface.reset(delay, 20_000, 20_000);
        self.wait_busy_low(delay);
        delay.delay_ms(30);

        self.cmd_with_data(spi, Command::CMDH, &[0x49, 0x55, 0x20, 0x08, 0x09, 0x18])?;
        self.cmd_with_data(spi, Command::Ox01, &[0x3f])?;
        self.cmd_with_data(spi, Command::Ox00, &[0x5f, 0x69])?;
        self.cmd_with_data(spi, Command::Ox03, &[0x00, 0x54, 0x00, 0x44])?;
        self.cmd_with_data(spi, Command::Ox05, &[0x40, 0x1f, 0x1f, 0x2c])?;
        self.cmd_with_data(spi, Command::Ox06, &[0x6f, 0x1f, 0x17, 0x49])?;
        self.cmd_with_data(spi, Command::Ox08, &[0x6f, 0x1f, 0x1f, 0x22])?;
        self.cmd_with_data(spi, Command::Ox30, &[0x03])?;
        self.cmd_with_data(spi, Command::Ox50, &[0x3f])?;
        self.cmd_with_data(spi, Command::Ox60, &[0x02, 0x00])?;
        self.cmd_with_data(spi, Command::Ox61, &[0x03, 0x20, 0x01, 0xe0])?;
        self.cmd_with_data(spi, Command::Ox84, &[0x01])?;
        self.cmd_with_data(spi, Command::OxE3, &[0x2f])?;

        self.command(spi, Command::PowerOn)?;
        self.wait_busy_low(delay);

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

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.cmd_with_data(spi, Command::PowerOff, &[0x00])?;
        self.wait_busy_low(delay);

        self.cmd_with_data(spi, Command::DeepSleep, &[0xa5])?;
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
        self.cmd_with_data(spi, Command::DataStartTransmission, buffer)
    }

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
        assert!(width > 0 && height > 0);
        assert!(matches!(x.checked_add(width), Some(end) if end <= WIDTH));
        assert!(matches!(y.checked_add(height), Some(end) if end <= HEIGHT));
        assert_eq!(buffer.len(), (((width + 1) / 2) * height) as usize);

        let x_end = x + width - 1;
        let y_end = y + height - 1;

        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(
            spi,
            Command::PartialWindow,
            &[
                (x >> 8) as u8,
                x as u8,
                (x_end >> 8) as u8,
                x_end as u8,
                (y >> 8) as u8,
                y as u8,
                (y_end >> 8) as u8,
                y_end as u8,
                0x01,
            ],
        )?;
        self.cmd_with_data(spi, Command::DataStartTransmission, &[0x00])?;
        self.interface.data(spi, buffer)
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), <SPI>::Error> {
        self.command(spi, Command::PowerOn)?;
        self.wait_busy_low(delay);

        self.cmd_with_data(spi, Command::Ox06, &[0x6f, 0x1f, 0x17, 0x49])?;

        self.cmd_with_data(spi, Command::DataFresh, &[0x00])?;
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
        Ok(())
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
            HexColor::Yellow,
            HexColor::Red,
            HexColor::Blue,
            HexColor::Green,
            HexColor::White,
        ];

        self.command(spi, Command::DataStartTransmission)?;
        for color in color_6.iter() {
            for _ in 0..20_000 {
                self.interface
                    .data(spi, &[HexColor::colors_byte(*color, *color)])?;
            }
        }

        self.display_frame(spi, delay)
    }
}
