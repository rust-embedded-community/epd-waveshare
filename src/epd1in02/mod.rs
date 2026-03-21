//! A simple Driver for the Waveshare 1.02" E-Ink Display via SPI
//!
//! - [Datasheet](https://www.waveshare.com/product/1.02inch-e-paper.htm)
//!
//! The display controller IC is UC8175

use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::color::Color;
use crate::interface::DisplayInterface;
use crate::prelude::WaveshareDisplay;
use crate::traits::{InternalWiAdditions, QuickRefresh, RefreshLut};

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

pub(crate) mod constants;
use self::constants::{
    LUT_FULL_UPDATE_BLACK, LUT_FULL_UPDATE_WHITE, LUT_PARTIAL_UPDATE_BLACK,
    LUT_PARTIAL_UPDATE_WHITE,
};

/// Full size buffer for use with the 1in02 EPD
#[cfg(feature = "graphics")]
pub type Display1in02 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Width of the display
pub const WIDTH: u32 = 80;
/// Height of the display
pub const HEIGHT: u32 = 128;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
/// BUSY is low active
const IS_BUSY_LOW: bool = true;
/// Number of bytes to contain values of all display pixels
const NUMBER_OF_BYTES: u32 = WIDTH * HEIGHT / 8;
const SINGLE_BYTE_WRITE: bool = true;

/// Epd1in02 driver
///
pub struct Epd1in02<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: Color,
    is_turned_on: bool,
    refresh_mode: RefreshLut,
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd1in02<SPI, BUSY, DC, RST, DELAY>
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
    ) -> Result<Self, SPI::Error> {
        let interface = DisplayInterface::new(busy, dc, rst, delay_us);
        let color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = Epd1in02 {
            interface,
            color,
            is_turned_on: false,
            refresh_mode: RefreshLut::Full,
        };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.turn_off(spi, delay)?;
        self.cmd_with_data(spi, Command::DeepSleep, &[0xA5])?;

        // display registers are set to default value
        self.refresh_mode = RefreshLut::Full;

        Ok(())
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
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

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;

        self.set_full_mode(spi, delay)?;

        let color_value = self.background_color().get_byte_value();

        self.command(spi, Command::DataStartTransmission1)?;
        self.interface
            .data_x_times(spi, color_value, NUMBER_OF_BYTES)?;

        self.cmd_with_data(spi, Command::DataStartTransmission2, buffer)?;
        Ok(())
    }

    // Implemented as quick partial update
    // as it requires old frame update
    fn update_partial_frame(
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

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.turn_on_if_turned_off(spi, delay)?;

        self.command(spi, Command::DisplayRefresh)?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }

    fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.update_frame(spi, buffer, delay)?;
        self.display_frame(spi, delay)?;
        Ok(())
    }

    fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.set_full_mode(spi, delay)?;

        let color_value = self.background_color().get_byte_value();

        self.command(spi, Command::DataStartTransmission1)?;
        self.interface
            .data_x_times(spi, !color_value, NUMBER_OF_BYTES)?;

        self.command(spi, Command::DataStartTransmission2)?;
        self.interface
            .data_x_times(spi, color_value, NUMBER_OF_BYTES)?;

        Ok(())
    }

    fn set_lut(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        let (white_lut, black_lut) = match refresh_rate {
            Some(RefreshLut::Full) => (&LUT_FULL_UPDATE_WHITE, &LUT_FULL_UPDATE_BLACK),
            Some(RefreshLut::Quick | RefreshLut::PartialRefresh) => {
                (&LUT_PARTIAL_UPDATE_WHITE, &LUT_PARTIAL_UPDATE_BLACK)
            }
            None => return Ok(()),
        };

        self.cmd_with_data(spi, Command::SetWhiteLut, white_lut)?;
        self.cmd_with_data(spi, Command::SetBlackLut, black_lut)?;
        Ok(())
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> QuickRefresh<SPI, BUSY, DC, RST, DELAY>
    for Epd1in02<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    /// To be followed immediately by update_new_frame
    fn update_old_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.set_partial_mode(spi, delay)?;
        self.set_partial_window(spi, delay, 0, 0, WIDTH, HEIGHT)?;

        self.cmd_with_data(spi, Command::DataStartTransmission1, buffer)?;
        Ok(())
    }

    /// To be used immediately after update_old_frame
    fn update_new_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(spi, Command::DataStartTransmission2, buffer)?;
        Ok(())
    }

    fn display_new_frame(&mut self, _spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        unimplemented!()
    }

    fn update_and_display_new_frame(
        &mut self,
        _spi: &mut SPI,
        _buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        unimplemented!()
    }

    /// To be followed immediately by update_partial_new_frame
    /// isn't faster then full update
    fn update_partial_old_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        if !is_buffer_size_ok(buffer, width, height) {
            panic!("Image buffer size is not correct")
        }

        self.set_partial_mode(spi, delay)?;
        self.set_partial_window(spi, delay, x, y, width, height)?;

        self.cmd_with_data(spi, Command::DataStartTransmission1, buffer)?;
        Ok(())
    }

    /// To be used immediately after update_partial_old_frame
    fn update_partial_new_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        buffer: &[u8],
        _x: u32,
        _y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        if !is_buffer_size_ok(buffer, width, height) {
            panic!("Image buffer size is not correct")
        }

        self.cmd_with_data(spi, Command::DataStartTransmission2, buffer)?;
        Ok(())
    }

    /// Isn't faster then full clear
    fn clear_partial_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        // set full LUT as quick LUT requires old image
        self.set_full_mode(spi, delay)?;
        self.command(spi, Command::PartialIn)?;
        self.set_partial_window(spi, delay, x, y, width, height)?;

        let color_value = self.background_color().get_byte_value();
        let number_of_bytes = buffer_len(width as usize, height as usize) as u32;

        self.command(spi, Command::DataStartTransmission1)?;
        self.interface
            .data_x_times(spi, !color_value, number_of_bytes)?;

        self.command(spi, Command::DataStartTransmission2)?;
        self.interface
            .data_x_times(spi, color_value, number_of_bytes)?;

        self.command(spi, Command::PartialOut)?;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd1in02<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Reset the device
        self.interface.reset(delay, 20_000, 2000);

        // Set the panel settings: LUT from register
        self.cmd_with_data(spi, Command::PanelSetting, &[0x6F])?;

        // Set the power settings: VGH=16V, VGL=-16V, VDH=11V, VDL=-11V
        self.cmd_with_data(spi, Command::PowerSetting, &[0x03, 0x00, 0x2b, 0x2b])?;

        // Set the charge pump settings: 50ms, Strength 4, 8kHz
        self.cmd_with_data(spi, Command::ChargePumpSetting, &[0x3F])?;

        // Set LUT option: no All-Gate-ON
        self.cmd_with_data(spi, Command::LutOption, &[0x00, 0x00])?;

        // Set the clock frequency: 50 Hz
        self.cmd_with_data(spi, Command::PllControl, &[0x17])?;

        // Set Vcom and data interval: default
        // set the border color the same as background color
        let value = match self.background_color() {
            Color::Black => 0x57,
            Color::White => 0x97,
        };
        self.cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[value])?;

        // Set the non-overlapping period of Gate and Source: 24us
        self.cmd_with_data(spi, Command::TconSetting, &[0x22])?;

        // Set the real resolution
        self.send_resolution(spi)?;

        // Set Vcom DC value: -1 V
        self.cmd_with_data(spi, Command::VcomDcSetting, &[0x12])?;

        // Set pover saving settings
        self.cmd_with_data(spi, Command::PowerSaving, &[0x33])?;

        self.set_lut(spi, delay, Some(self.refresh_mode))?;

        self.wait_until_idle(spi, delay)?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd1in02<SPI, BUSY, DC, RST, DELAY>
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

    fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data)
    }

    fn send_resolution(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        let w = self.width();
        let h = self.height();

        self.command(spi, Command::TconResolution)?;
        self.send_data(spi, &[h as u8])?;
        self.send_data(spi, &[w as u8])
    }

    /// PowerOn command
    fn turn_on_if_turned_off(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        if !self.is_turned_on {
            self.command(spi, Command::PowerOn)?;
            self.wait_until_idle(spi, delay)?;
            self.is_turned_on = true;
        }
        Ok(())
    }

    fn turn_off(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::PowerOff)?;
        self.wait_until_idle(spi, delay)?;
        self.is_turned_on = false;
        Ok(())
    }

    fn set_full_mode(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        if self.refresh_mode != RefreshLut::Full {
            self.command(spi, Command::PartialOut)?;
            self.set_lut(spi, delay, Some(RefreshLut::Full))?;
            self.refresh_mode = RefreshLut::Full;
        }
        Ok(())
    }

    fn set_partial_mode(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        if self.refresh_mode != RefreshLut::Quick {
            self.command(spi, Command::PartialIn)?;
            self.set_lut(spi, delay, Some(RefreshLut::Quick))?;
            self.refresh_mode = RefreshLut::Quick;
        }
        Ok(())
    }

    fn set_partial_window(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        if !is_window_size_ok(x, y, width, height) {
            panic!("Partial update window size is not correct")
        }

        self.cmd_with_data(
            spi,
            Command::PartialWindow,
            &[
                x as u8,
                (x + width - 1) as u8,
                y as u8,
                (y + height - 1) as u8,
                0x00,
            ],
        )?;

        Ok(())
    }
}

fn is_window_size_ok(x: u32, y: u32, width: u32, height: u32) -> bool {
    // partial update window is inside the screen
    x + width <= WIDTH && y + height <= HEIGHT
    // 3 less significant bits are ignored
    && x % 8 == 0 && width % 8 == 0
}

fn is_buffer_size_ok(buffer: &[u8], width: u32, height: u32) -> bool {
    buffer_len(width as usize, height as usize) == buffer.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 80);
        assert_eq!(HEIGHT, 128);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }

    #[test]
    fn inside_of_screen() {
        assert!(is_window_size_ok(0, 0, 80, 128));
    }

    #[test]
    fn x_too_big() {
        assert!(!is_window_size_ok(8, 8, 80, 1));
    }

    #[test]
    fn y_too_big() {
        assert!(!is_window_size_ok(8, 8, 8, 121));
    }

    #[test]
    fn x_is_not_multiple_of_8() {
        assert!(!is_window_size_ok(1, 0, 72, 128));
    }

    #[test]
    fn width_is_not_multiple_of_8() {
        assert!(!is_window_size_ok(0, 0, 79, 128));
    }

    #[test]
    fn buffer_size_incorrect() {
        let buf = [0u8; 10];
        assert!(!is_buffer_size_ok(&buf, 10, 10));
    }

    #[test]
    fn buffer_size_correct() {
        let buf = [0u8; 10];
        assert!(is_buffer_size_ok(&buf, 8, 10));
    }
}
