//! A simple Drlever for the Waveshare 2.13" B V4 E-Ink Display via SPI
//! More information on this display can be found at the [Waveshare Wiki](https://www.waveshare.com/wiki/Pico-ePaper-2.13-B)
//! This driver was build and tested for 250x122, 2.13inch E-Ink display HAT for Raspberry Pi, three-color, SPI interface
//!
//! # Example for the 2.13" B V4 E-Ink Display
//!
//!```rust, no_run
//!# use embedded_hal_mock::eh1::*;
//!# fn main() -> Result<(), MockError> {
//!use embedded_graphics::{prelude::*, primitives::{Line, PrimitiveStyle, PrimitiveStyleBuilder}};
//!use epd_waveshare::{epd2in13b_v4::*, prelude::*};
//!#
//!# let expectations = [];
//!# let mut spi = spi::Mock::new(&expectations);
//!# let expectations = [];
//!# let cs_pin = digital::Mock::new(&expectations);
//!# let busy_in = digital::Mock::new(&expectations);
//!# let dc = digital::Mock::new(&expectations);
//!# let rst = digital::Mock::new(&expectations);
//!# let mut delay = delay::NoopDelay::new();
//!
//!// Setup EPD
//!let mut epd = Epd2in13b::new(&mut spi, busy_in, dc, rst, &mut delay, None).unwrap();
//!
//!// Use display graphics from embedded-graphics
//!// This display is for the black/white/chromatic pixels
//!let mut tricolor_display = Display2in13b::default();
//!
//!// Use embedded graphics for drawing a black line
//!let _ = Line::new(Point::new(0, 120), Point::new(0, 200))
//!    .into_styled(PrimitiveStyle::with_stroke(TriColor::Black, 1))
//!    .draw(&mut tricolor_display);
//!
//!// We use `chromatic` but it will be shown as red/yellow
//!let _ = Line::new(Point::new(15, 120), Point::new(15, 200))
//!    .into_styled(PrimitiveStyle::with_stroke(TriColor::Chromatic, 1))
//!    .draw(&mut tricolor_display);
//!
//!// Display updated frame
//!epd.update_color_frame(
//!    &mut spi,
//!    &mut delay,
//!    &tricolor_display.bw_buffer(),
//!    &tricolor_display.chromatic_buffer()
//!).unwrap();
//!epd.display_frame(&mut spi, &mut delay).unwrap();
//!
//!// Set the EPD to sleep
//!epd.sleep(&mut spi, &mut delay).unwrap();
//!# Ok(())
//!# }
//!```
use core::convert::Infallible;

use embedded_graphics_core::prelude::DrawTarget;
// Original Waveforms from Waveshare
use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::color::TriColor;
use crate::interface::DisplayInterface;
use crate::traits::{
    InternalWiAdditions, RefreshLut, WaveshareDisplay, WaveshareThreeColorDisplay,
};
use crate::{buffer_len, color::Color};

pub(crate) mod command;
use self::command::{
    BorderWaveForm, BorderWaveFormFixLevel, BorderWaveFormGs, BorderWaveFormVbd, Command,
    DataEntryModeDir, DataEntryModeIncr, DeepSleepMode, DisplayUpdateControl, DriverOutput,
    RamOption,
};

const SINGLE_BYTE_WRITE: bool = true;

/// Full size buffer for use with the 2.13" v4 EPD
#[cfg(feature = "graphics")]
pub type Display2in13b = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) * 2 },
    TriColor,
>;

#[cfg(feature = "graphics")]
/// buffered buffer
pub type BufferMonoDisplay2in13b = crate::graphics::Display<
    WIDTH,
    { HEIGHT / BUFFER },
    false,
    { buffer_len(WIDTH as usize, (HEIGHT / BUFFER) as usize) },
    Color,
>;

#[cfg(feature = "graphics")]
/// buffered buffer
pub type BufferChromaticDisplay2in13b = crate::graphics::Display<
    WIDTH,
    { HEIGHT / BUFFER },
    false,
    { buffer_len(WIDTH as usize, (HEIGHT / BUFFER) as usize) },
    TriColor,
>;

/// buffers
pub const BUFFER: u32 = 4;

/// Width of the display.
pub const WIDTH: u32 = 122;

/// Height of the display
pub const HEIGHT: u32 = 250;

/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: TriColor = TriColor::White;
const IS_BUSY_LOW: bool = false;

/// Epd2in13b (V4) driver
pub struct Epd2in13b<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,

    /// Background Color
    background_color: TriColor,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in13b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // HW reset
        self.interface.reset(delay, 10_000, 10_000);

        self.wait_until_idle(spi, delay)?;
        self.interface.cmd(spi, Command::SwReset)?;
        self.wait_until_idle(spi, delay)?;

        self.set_driver_output(
            spi,
            DriverOutput {
                scan_is_linear: true,
                scan_g0_is_first: true,
                scan_dir_incr: true,
                width: (HEIGHT - 1) as u16,
            },
        )?;

        self.set_data_entry_mode(spi, DataEntryModeIncr::XIncrYIncr, DataEntryModeDir::XDir)?;

        // Use simple X/Y auto increase
        self.set_ram_area(spi, 0, 0, WIDTH - 1, HEIGHT - 1)?;
        self.set_ram_address_counters(spi, delay, 0, 0)?;

        self.set_border_waveform(
            spi,
            command::BorderWaveForm {
                vbd: BorderWaveFormVbd::Gs,
                fix_level: BorderWaveFormFixLevel::Vss,
                gs_trans: BorderWaveFormGs::Lut3,
            },
        )?;

        self.cmd_with_data(spi, Command::WriteVcomRegister, &[0x36])?;
        self.cmd_with_data(spi, Command::GateDrivingVoltageCtrl, &[0x17])?;
        self.cmd_with_data(spi, Command::SourceDrivingVoltageCtrl, &[0x41, 0x00, 0x32])?;

        self.set_display_update_control(
            spi,
            command::DisplayUpdateControl {
                red_ram_option: RamOption::Normal,
                bw_ram_option: RamOption::Normal,
                source_output_mode: true,
            },
        )?;

        self.wait_until_idle(spi, delay)?;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd2in13b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    /// Transmit data to the SRAM of the EPD with the provided generators.
    ///
    /// Updates both the black and the secondary color layers
    /// Useful for rendering directly from progmem buffers.
    ///
    /// Example:
    /// ```rust no_run
    /// progmem! {
    ///     static progmem BLACK: [u8; 4000] = *include_bytes!("black.gray");
    ///     static progmem RED: [u8; 4000] = *include_bytes!("red.gray");
    /// }
    /// epd.update_color_frame_with(
    ///     &mut spi,
    ///     &mut delay,
    ///     |i| BLACK.load_at(i),
    ///     |i| RED.load_at(i),
    ///     BLACK.len(),
    ///     RED.len(),
    /// )?;
    /// ```
    pub fn update_color_frame_with(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        black: impl Fn(usize) -> u8,
        chromatic: impl Fn(usize) -> u8,
        black_len: usize,
        chromatic_len: usize,
    ) -> Result<(), SPI::Error> {
        self.update_achromatic_frame_with(spi, delay, black, black_len)?;
        self.update_chromatic_frame_with(spi, delay, chromatic, chromatic_len)
    }

    /// Update only the black/white data of the display using a generator
    ///
    /// This must be finished by calling `update_chromatic_frame`.
    pub fn update_achromatic_frame_with(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        black: impl Fn(usize) -> u8,
        len: usize,
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::WriteRam)?;
        self.interface.data_with(spi, black, len)?;
        Ok(())
    }

    /// Update only the chromatic data of the display.
    ///
    /// This should be preceded by a call to `update_achromatic_frame`.
    /// This data takes precedence over the black/white data.
    pub fn update_chromatic_frame_with(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        chromatic: impl Fn(usize) -> u8,
        len: usize,
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::WriteRamRed)?;
        self.interface.data_with(spi, chromatic, len)?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareThreeColorDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in13b<SPI, BUSY, DC, RST, DELAY>
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

    fn update_achromatic_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        black: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::WriteRam)?;
        self.interface.data(spi, black)?;
        Ok(())
    }

    fn update_chromatic_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        chromatic: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::WriteRamRed)?;
        self.interface.data(spi, chromatic)?;
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in13b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    type DisplayColor = TriColor;
    fn new(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
    ) -> Result<Self, SPI::Error> {
        let mut epd = Epd2in13b {
            interface: DisplayInterface::new(busy, dc, rst, delay_us),
            background_color: DEFAULT_BACKGROUND_COLOR,
        };

        epd.init(spi, delay)?;
        Ok(epd)
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn sleep(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.set_sleep_mode(spi, DeepSleepMode::Normal)?;
        Ok(())
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        assert!(buffer.len() == buffer_len(WIDTH as usize, HEIGHT as usize));
        self.cmd_with_data(spi, Command::WriteRam, buffer)?;

        self.command(spi, Command::WriteRamRed)?;
        self.interface.data_x_times(
            spi,
            TriColor::Black.get_byte_value(),
            buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
        )?;
        Ok(())
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
    ) -> Result<(), SPI::Error> {
        unimplemented!();
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::MasterActivation)?;
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

    fn clear_frame(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.clear_achromatic_frame(spi)?;
        self.clear_chromatic_frame(spi)
    }

    fn set_background_color(&mut self, background_color: TriColor) {
        self.background_color = background_color;
    }

    fn background_color(&self) -> &TriColor {
        &self.background_color
    }

    fn width(&self) -> u32 {
        WIDTH
    }

    fn height(&self) -> u32 {
        HEIGHT
    }

    fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        unimplemented!()
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

#[derive(Copy, Clone)]
/// a type safe chunk reperesentation for BufferMonoDisplay
pub enum Chunk {
    /// the first chunk of the display
    Buf1,
    /// the second chunk of the display
    Buf2,
    /// the third chunk of the display
    Buf3,
    /// the fourth chunk of the display
    Buf4,
}

impl Chunk {
    /// converts chunk to a zero-indexed `u32`
    pub fn to_zero_indexed(&self) -> u32 {
        match self {
            Chunk::Buf1 => 0,
            Chunk::Buf2 => 1,
            Chunk::Buf3 => 2,
            Chunk::Buf4 => 3,
        }
    }
    /// converts from zero-indexed `u32` to a `Chunk`
    pub const fn from_zero_indexed(i: u32) -> Self {
        match i {
            0 => Chunk::Buf1,
            1 => Chunk::Buf2,
            2 => Chunk::Buf3,
            3 => Chunk::Buf4,
            _ => panic!("please check buffer"),
        }
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd2in13b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    /// Due to memory limitations on the arduino boards, this function allows the user to separate the 122x250 board into four 122x62.5(rounding to 63) subgrids.
    ///
    /// for usage on `mono_buffers` and colored_buffers`, please refer to the documentation of `update_achromatic_buffered` and `update_chromatic_buffered`
    pub fn update_frame_buffered(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        mono_buffers: impl FnMut(&mut BufferMonoDisplay2in13b, Chunk) -> Result<Option<()>, Infallible>,
        colored_buffers: impl FnMut(
            &mut BufferMonoDisplay2in13b,
            Chunk,
        ) -> Result<Option<()>, Infallible>,
    ) -> Result<(), SPI::Error> {
        self.update_achromatic_buffered(spi, delay, mono_buffers)?;
        self.update_chromatic_buffered(spi, delay, colored_buffers)
    }

    /// Due to memory limitations on the arduino boards, this function allows the user to separate the 122x250 board into four 122x62.5(rounding to 63) subgrids.
    ///
    /// IMPORTANT: this must be followed by `update_chromatic_buffered`, even if you're trying to only display purely mono content, otherwise the display won't be updated.
    ///
    /// `buffers`: A function that that should populate the content of each section of the display.
    ///     - Takes a mutable reference to `BuffermonoDisplay2in13b` and a buffer index(0-3)
    ///     - Returns `Result<Option<(), Infalliable>>`
    ///         * `Ok(Some(()))` indicates successful execution.
    ///         * `Ok(None)` indicates the buffer should be left unmodified, leaving it uncolored.
    ///         * `Err(Infalliable)` is here purely for allowing `?` with `embedded-graphics` draw operations.
    pub fn update_achromatic_buffered(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        mut buffers: impl FnMut(&mut BufferMonoDisplay2in13b, Chunk) -> Result<Option<()>, Infallible>,
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::WriteRam)?;
        for i in 0..BUFFER {
            let mut buffer = BufferMonoDisplay2in13b::default();
            if buffers(&mut buffer, Chunk::from_zero_indexed(i))
                .unwrap()
                .is_none()
            {
                buffer.clear(Color::White).unwrap();
            }
            let data = buffer.buffer();
            self.interface.data(spi, data)?;
        }
        Ok(())
    }

    /// Due to memory limitations on the arduino boards, this function allows the user to separate the 122x250 board into four 122x62.5(rounding to 63) subgrids.
    ///
    /// IMPORTANT: this function must be called after `update_achromatic_buffered`, even if you're trying to only display purely mono content, otherwise the display won't be updated.
    ///
    /// The usage of color within `BufferMonoDisplay2in13b` is a misnomer. `Color::White` stands for colored(red), while `Color::Black` stands for uncolored(white).
    ///
    /// `buffers`: A function that that should populate the content of each section of the display.
    ///     - Takes a mutable reference to `BufferMonoDisplay2in13b` and a buffer index(0-3)
    ///     - Returns `Result<Option<(), Infalliable>>`
    ///         * `Ok(Some(()))` indicates successful execution
    ///         * `Ok(None)` indicates the buffer should be left unmodified, leaving it uncolored.
    ///         * `Err(Infalliable)` is here purely for allowing `?` with `embedded-graphics` draw operations.
    pub fn update_chromatic_buffered(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        mut buffers: impl FnMut(&mut BufferMonoDisplay2in13b, Chunk) -> Result<Option<()>, Infallible>,
    ) -> Result<(), SPI::Error> {
        self.command(spi, Command::WriteRamRed)?;
        let mut buffer = BufferMonoDisplay2in13b::default();
        for i in 0..BUFFER {
            if buffers(&mut buffer, Chunk::from_zero_indexed(i))
                .unwrap()
                .is_none()
            {
                buffer.clear(Color::Black).unwrap();
            }
            let data = buffer.buffer();
            self.interface.data(spi, data)?;
        }
        self.interface.cmd(spi, Command::MasterActivation)?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }

    fn set_display_update_control(
        &mut self,
        spi: &mut SPI,
        display_update_control: DisplayUpdateControl,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(
            spi,
            Command::DisplayUpdateControl1,
            &display_update_control.to_bytes(),
        )
    }

    fn set_border_waveform(
        &mut self,
        spi: &mut SPI,
        borderwaveform: BorderWaveForm,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(
            spi,
            Command::BorderWaveformControl,
            &[borderwaveform.to_u8()],
        )
    }

    /// Triggers the deep sleep mode
    fn set_sleep_mode(&mut self, spi: &mut SPI, mode: DeepSleepMode) -> Result<(), SPI::Error> {
        self.cmd_with_data(spi, Command::DeepSleepMode, &[mode as u8])
    }

    fn set_driver_output(&mut self, spi: &mut SPI, output: DriverOutput) -> Result<(), SPI::Error> {
        self.cmd_with_data(spi, Command::DriverOutputControl, &output.to_bytes())
    }

    /// Sets the data entry mode (ie. how X and Y positions changes when writing
    /// data to RAM)
    fn set_data_entry_mode(
        &mut self,
        spi: &mut SPI,
        counter_incr_mode: DataEntryModeIncr,
        counter_direction: DataEntryModeDir,
    ) -> Result<(), SPI::Error> {
        let mode = counter_incr_mode as u8 | counter_direction as u8;
        self.cmd_with_data(spi, Command::DataEntryModeSetting, &[mode])
    }

    /// Sets both X and Y pixels ranges
    fn set_ram_area(
        &mut self,
        spi: &mut SPI,
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(
            spi,
            Command::SetRamXAddressStartEndPosition,
            &[(start_x >> 3) as u8, (end_x >> 3) as u8],
        )?;

        self.cmd_with_data(
            spi,
            Command::SetRamYAddressStartEndPosition,
            &[
                start_y as u8,
                (start_y >> 8) as u8,
                end_y as u8,
                (end_y >> 8) as u8,
            ],
        )
    }

    /// Sets both X and Y pixels counters when writing data to RAM
    fn set_ram_address_counters(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        x: u32,
        y: u32,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::SetRamXAddressCounter, &[(x >> 3) as u8])?;

        self.cmd_with_data(
            spi,
            Command::SetRamYAddressCounter,
            &[y as u8, (y >> 8) as u8],
        )?;
        Ok(())
    }

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

    fn clear_achromatic_frame(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        match self.background_color {
            TriColor::White => {
                self.command(spi, Command::WriteRam)?;
                self.interface.data_x_times(
                    spi,
                    0xFF,
                    buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                )?;
            }
            TriColor::Chromatic => {
                self.command(spi, Command::WriteRam)?;
                self.interface.data_x_times(
                    spi,
                    0xFF,
                    buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                )?;
            }
            TriColor::Black => {
                self.command(spi, Command::WriteRam)?;
                self.interface.data_x_times(
                    spi,
                    0x00,
                    buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                )?;
            }
        }

        Ok(())
    }

    fn clear_chromatic_frame(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        match self.background_color {
            TriColor::White => {
                self.command(spi, Command::WriteRam)?;
                self.interface.data_x_times(
                    spi,
                    0x00,
                    buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                )?;
            }
            TriColor::Chromatic => {
                self.command(spi, Command::WriteRam)?;
                self.interface.data_x_times(
                    spi,
                    0xFF,
                    buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                )?;
            }
            TriColor::Black => {
                self.command(spi, Command::WriteRam)?;
                self.interface.data_x_times(
                    spi,
                    0x00,
                    buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                )?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 122);
        assert_eq!(HEIGHT, 250);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, TriColor::White);
    }
}
