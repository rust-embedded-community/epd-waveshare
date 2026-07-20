//! A simple Driver for the Waveshare 2.7" B Tri-Color E-Ink Display via SPI
//!
//! [Documentation](https://www.waveshare.com/wiki/2.7inch_e-Paper_HAT_(B))

use embedded_hal::{delay::*, digital::*, spi::SpiDevice};

use crate::interface::DisplayInterface;
use crate::traits::{
    InternalWiAdditions, RefreshLut, WaveshareDisplay, WaveshareThreeColorDisplay,
};

// The Lookup Tables for the Display
mod constants;
use crate::epd2in7b::constants::*;

/// Width of the display
pub const WIDTH: u32 = 176;
/// Height of the display
pub const HEIGHT: u32 = 264;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const IS_BUSY_LOW: bool = true;
const SINGLE_BYTE_WRITE: bool = true;

use crate::color::Color;

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Full size buffer for use with the 2in7B EPD
/// TODO this should be a TriColor, but let's keep it as is at first
#[cfg(feature = "graphics")]
pub type Display2in7b = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Epd2in7b driver
pub struct Epd2in7b<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: Color,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in7b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // reset the device
        self.interface.reset(delay, 10_000, 2_000);

        // power on
        self.command(spi, Command::PowerOn)?;
        delay.delay_us(5000);
        self.wait_until_idle(spi, delay)?;

        // set panel settings, 0xbf is bw, 0xaf is multi-color
        self.interface
            .cmd_with_data(spi, Command::PanelSetting, &[0xaf])?;

        // pll control
        self.interface
            .cmd_with_data(spi, Command::PllControl, &[0x3a])?;

        // set the power settings
        self.interface.cmd_with_data(
            spi,
            Command::PowerSetting,
            &[0x03, 0x00, 0x2b, 0x2b, 0x09],
        )?;

        // start the booster
        self.interface
            .cmd_with_data(spi, Command::BoosterSoftStart, &[0x07, 0x07, 0x17])?;

        // power optimization
        self.interface
            .cmd_with_data(spi, Command::PowerOptimization, &[0x60, 0xa5])?;
        self.interface
            .cmd_with_data(spi, Command::PowerOptimization, &[0x89, 0xa5])?;
        self.interface
            .cmd_with_data(spi, Command::PowerOptimization, &[0x90, 0x00])?;
        self.interface
            .cmd_with_data(spi, Command::PowerOptimization, &[0x93, 0x2a])?;
        self.interface
            .cmd_with_data(spi, Command::PowerOptimization, &[0x73, 0x41])?;

        self.interface
            .cmd_with_data(spi, Command::VcmDcSetting, &[0x12])?;

        self.interface
            .cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x87])?;

        self.set_lut(spi, delay, None)?;

        self.interface
            .cmd_with_data(spi, Command::PartialDisplayRefresh, &[0x00])?;

        self.wait_until_idle(spi, delay)?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in7b<SPI, BUSY, DC, RST, DELAY>
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

        let mut epd = Epd2in7b { interface, color };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.interface
            .cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0xf7])?;

        self.command(spi, Command::PowerOff)?;
        self.wait_until_idle(spi, delay)?;
        self.interface
            .cmd_with_data(spi, Command::DeepSleep, &[0xA5])?;
        Ok(())
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::DataStartTransmission1)?;
        self.send_buffer_helper(spi, buffer)?;

        // Clear chromatic layer since we won't be using it here
        self.interface.cmd(spi, Command::DataStartTransmission2)?;
        self.interface
            .data_x_times(spi, !self.color.get_byte_value(), WIDTH / 8 * HEIGHT)?;

        self.interface.cmd(spi, Command::DataStop)?;
        Ok(())
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
    ) -> Result<(), SPI::Error> {
        self.interface
            .cmd(spi, Command::PartialDataStartTransmission1)?;

        self.send_data(spi, &[(x >> 8) as u8])?;
        self.send_data(spi, &[(x & 0xf8) as u8])?;
        self.send_data(spi, &[(y >> 8) as u8])?;
        self.send_data(spi, &[(y & 0xff) as u8])?;
        self.send_data(spi, &[(width >> 8) as u8])?;
        self.send_data(spi, &[(width & 0xf8) as u8])?;
        self.send_data(spi, &[(height >> 8) as u8])?;
        self.send_data(spi, &[(height & 0xff) as u8])?;
        self.wait_until_idle(spi, delay)?;

        self.send_buffer_helper(spi, buffer)?;

        self.interface.cmd(spi, Command::DataStop)
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
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
        self.command(spi, Command::DisplayRefresh)?;
        Ok(())
    }

    fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;

        let color_value = self.color.get_byte_value();
        self.interface.cmd(spi, Command::DataStartTransmission1)?;
        self.interface
            .data_x_times(spi, color_value, WIDTH / 8 * HEIGHT)?;

        self.interface.cmd(spi, Command::DataStop)?;

        self.interface.cmd(spi, Command::DataStartTransmission2)?;
        self.interface
            .data_x_times(spi, color_value, WIDTH / 8 * HEIGHT)?;
        self.interface.cmd(spi, Command::DataStop)?;
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

    fn set_lut(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::LutForVcom, &LUT_VCOM_DC)?;
        self.cmd_with_data(spi, Command::LutWhiteToWhite, &LUT_WW)?;
        self.cmd_with_data(spi, Command::LutBlackToWhite, &LUT_BW)?;
        self.cmd_with_data(spi, Command::LutWhiteToBlack, &LUT_BB)?;
        self.cmd_with_data(spi, Command::LutBlackToBlack, &LUT_WB)?;
        Ok(())
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareThreeColorDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in7b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn update_color_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        black: &[u8],
        chromatic: &[u8],
    ) -> Result<(), SPI::Error> {
        self.update_achromatic_frame(spi, delay, black)?;
        self.update_chromatic_frame(spi, delay, chromatic)
    }

    /// Update only the black/white data of the display.
    ///
    /// Finish by calling `update_chromatic_frame`.
    fn update_achromatic_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        achromatic: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::DataStartTransmission1)?;

        self.send_buffer_helper(spi, achromatic)?;

        self.interface.cmd(spi, Command::DataStop)
    }

    /// Update only chromatic data of the display.
    ///
    /// This data takes precedence over the black/white data.
    fn update_chromatic_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        chromatic: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::DataStartTransmission2)?;

        self.send_buffer_helper(spi, chromatic)?;

        self.interface.cmd(spi, Command::DataStop)?;
        self.wait_until_idle(spi, delay)?;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd2in7b<SPI, BUSY, DC, RST, DELAY>
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

    fn send_buffer_helper(&mut self, spi: &mut SPI, buffer: &[u8]) -> Result<(), SPI::Error> {
        // Based on the waveshare implementation, all data for color values is flipped. This helper
        // method makes that transmission easier
        for b in buffer.iter() {
            self.send_data(spi, &[!b])?;
        }
        Ok(())
    }

    fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data)
    }

    /// Refresh display for partial frame
    pub fn display_partial_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        self.command(spi, Command::PartialDisplayRefresh)?;
        self.send_data(spi, &[(x >> 8) as u8])?;
        self.send_data(spi, &[(x & 0xf8) as u8])?;
        self.send_data(spi, &[(y >> 8) as u8])?;
        self.send_data(spi, &[(y & 0xff) as u8])?;
        self.send_data(spi, &[(width >> 8) as u8])?;
        self.send_data(spi, &[(width & 0xf8) as u8])?;
        self.send_data(spi, &[(height >> 8) as u8])?;
        self.send_data(spi, &[(height & 0xff) as u8])?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }

    /// Update black/achromatic frame
    #[allow(clippy::too_many_arguments)]
    pub fn update_partial_achromatic_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        achromatic: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        self.interface
            .cmd(spi, Command::PartialDataStartTransmission1)?;
        self.send_data(spi, &[(x >> 8) as u8])?;
        self.send_data(spi, &[(x & 0xf8) as u8])?;
        self.send_data(spi, &[(y >> 8) as u8])?;
        self.send_data(spi, &[(y & 0xff) as u8])?;
        self.send_data(spi, &[(width >> 8) as u8])?;
        self.send_data(spi, &[(width & 0xf8) as u8])?;
        self.send_data(spi, &[(height >> 8) as u8])?;
        self.send_data(spi, &[(height & 0xff) as u8])?;
        self.wait_until_idle(spi, delay)?;

        for b in achromatic.iter() {
            // Flipping based on waveshare implementation
            self.send_data(spi, &[!b])?;
        }

        Ok(())
    }

    /// Update partial chromatic/red frame
    #[allow(clippy::too_many_arguments)]
    pub fn update_partial_chromatic_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        chromatic: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        self.interface
            .cmd(spi, Command::PartialDataStartTransmission2)?;
        self.send_data(spi, &[(x >> 8) as u8])?;
        self.send_data(spi, &[(x & 0xf8) as u8])?;
        self.send_data(spi, &[(y >> 8) as u8])?;
        self.send_data(spi, &[(y & 0xff) as u8])?;
        self.send_data(spi, &[(width >> 8) as u8])?;
        self.send_data(spi, &[(width & 0xf8) as u8])?;
        self.send_data(spi, &[(height >> 8) as u8])?;
        self.send_data(spi, &[(height & 0xff) as u8])?;
        self.wait_until_idle(spi, delay)?;

        for b in chromatic.iter() {
            // Flipping based on waveshare implementation
            self.send_data(spi, &[!b])?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::convert::Infallible;
    use embedded_hal::{
        delay::DelayNs,
        digital::{ErrorType as DigitalErrorType, InputPin, OutputPin},
        spi::{ErrorType as SpiErrorType, Operation, SpiDevice},
    };
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        vec::Vec,
    };

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SpiPhase {
        Command,
        Data,
    }

    struct RecordedWrite {
        phase: SpiPhase,
        bytes: Vec<u8>,
    }

    type Writes = Rc<RefCell<Vec<RecordedWrite>>>;

    struct RecordingSpi {
        dc_is_data: Rc<Cell<bool>>,
        writes: Writes,
    }

    impl SpiErrorType for RecordingSpi {
        type Error = Infallible;
    }

    impl SpiDevice for RecordingSpi {
        fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
            for operation in operations {
                match operation {
                    Operation::Write(bytes) => self.writes.borrow_mut().push(RecordedWrite {
                        phase: if self.dc_is_data.get() {
                            SpiPhase::Data
                        } else {
                            SpiPhase::Command
                        },
                        bytes: bytes.to_vec(),
                    }),
                    _ => unreachable!("display initialization only writes to SPI"),
                }
            }
            Ok(())
        }
    }

    struct Output {
        state: Rc<Cell<bool>>,
    }

    impl DigitalErrorType for Output {
        type Error = Infallible;
    }

    impl OutputPin for Output {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.state.set(false);
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.state.set(true);
            Ok(())
        }
    }

    struct IdleBusy;

    impl DigitalErrorType for IdleBusy {
        type Error = Infallible;
    }

    impl InputPin for IdleBusy {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(true)
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(false)
        }
    }

    struct NoopDelay;

    impl DelayNs for NoopDelay {
        fn delay_ns(&mut self, _ns: u32) {}
    }

    fn data_prefix_after_command(writes: &[RecordedWrite], command: u8) -> Vec<u8> {
        writes
            .iter()
            .skip_while(|write| {
                write.phase == SpiPhase::Data || write.bytes.as_slice() != [command]
            })
            .skip(1)
            .take_while(|write| write.phase == SpiPhase::Data)
            .flat_map(|write| write.bytes.iter().copied())
            .take(7)
            .collect()
    }

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 176);
        assert_eq!(HEIGHT, 264);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }

    #[test]
    fn initialization_uses_waveshare_v1_white_and_black_luts() {
        let dc_is_data = Rc::new(Cell::new(false));
        let writes = Rc::new(RefCell::new(Vec::new()));
        let mut spi = RecordingSpi {
            dc_is_data: dc_is_data.clone(),
            writes: writes.clone(),
        };
        let dc = Output { state: dc_is_data };
        let rst = Output {
            state: Rc::new(Cell::new(false)),
        };
        let mut delay = NoopDelay;

        Epd2in7b::new(&mut spi, IdleBusy, dc, rst, &mut delay, None).unwrap();

        let writes = writes.borrow();
        assert_eq!(
            data_prefix_after_command(&writes, Command::LutWhiteToBlack as u8),
            [0x90, 0x1a, 0x1a, 0x00, 0x00, 0x01, 0x40]
        );
        assert_eq!(
            data_prefix_after_command(&writes, Command::LutBlackToBlack as u8),
            [0x90, 0x1a, 0x1a, 0x00, 0x00, 0x01, 0x20]
        );
    }
}
