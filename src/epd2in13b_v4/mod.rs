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
// Original Waveforms from Waveshare
use embedded_hal::digital::{InputPin, OutputPin};

use crate::buffer_len;
use crate::color::TriColor;
use crate::interface::{DelayNs, DisplayInterface, SpiDevice};
use crate::traits::{
    InternalWiAdditions, RefreshLut, WaveshareDisplay, WaveshareThreeColorDisplay,
};

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

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in13b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // HW reset
        self.interface.reset(delay, 10_000, 10_000).await;

        self.wait_until_idle(spi, delay).await?;
        self.interface.cmd(spi, Command::SwReset).await?;
        self.wait_until_idle(spi, delay).await?;

        self.set_driver_output(
            spi,
            DriverOutput {
                scan_is_linear: true,
                scan_g0_is_first: true,
                scan_dir_incr: true,
                width: (HEIGHT - 1) as u16,
            },
        )
        .await?;

        self.set_data_entry_mode(spi, DataEntryModeIncr::XIncrYIncr, DataEntryModeDir::XDir)
            .await?;

        // Use simple X/Y auto increase
        self.set_ram_area(spi, 0, 0, WIDTH - 1, HEIGHT - 1).await?;
        self.set_ram_address_counters(spi, delay, 0, 0).await?;

        self.set_border_waveform(
            spi,
            command::BorderWaveForm {
                vbd: BorderWaveFormVbd::Gs,
                fix_level: BorderWaveFormFixLevel::Vss,
                gs_trans: BorderWaveFormGs::Lut3,
            },
        )
        .await?;

        self.cmd_with_data(spi, Command::WriteVcomRegister, &[0x36])
            .await?;
        self.cmd_with_data(spi, Command::GateDrivingVoltageCtrl, &[0x17])
            .await?;
        self.cmd_with_data(spi, Command::SourceDrivingVoltageCtrl, &[0x41, 0x00, 0x32])
            .await?;

        self.set_display_update_control(
            spi,
            command::DisplayUpdateControl {
                red_ram_option: RamOption::Normal,
                bw_ram_option: RamOption::Normal,
                source_output_mode: true,
            },
        )
        .await?;

        self.wait_until_idle(spi, delay).await?;

        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareThreeColorDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in13b<SPI, BUSY, DC, RST, DELAY>
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
    ) -> Result<(), SPI::Error> {
        self.update_achromatic_frame(spi, delay, black).await?;
        self.update_chromatic_frame(spi, delay, chromatic).await
    }

    async fn update_achromatic_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        black: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::WriteRam).await?;
        self.interface.data(spi, black).await?;
        Ok(())
    }

    async fn update_chromatic_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        chromatic: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::WriteRamRed).await?;
        self.interface.data(spi, chromatic).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
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
    async fn new(
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

        epd.init(spi, delay).await?;
        Ok(epd)
    }

    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay).await
    }

    async fn sleep(&mut self, spi: &mut SPI, _delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.set_sleep_mode(spi, DeepSleepMode::Normal).await?;
        Ok(())
    }

    async fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        assert!(buffer.len() == buffer_len(WIDTH as usize, HEIGHT as usize));
        self.cmd_with_data(spi, Command::WriteRam, buffer).await?;

        self.command(spi, Command::WriteRamRed).await?;
        self.interface
            .data_x_times(
                spi,
                TriColor::Black.get_byte_value(),
                buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
            )
            .await?;
        Ok(())
    }

    async fn update_partial_frame(
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

    async fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::MasterActivation).await?;
        self.wait_until_idle(spi, delay).await?;

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
        self.clear_achromatic_frame(spi).await?;
        self.clear_chromatic_frame(spi).await
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

    async fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        unimplemented!()
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
impl<SPI, BUSY, DC, RST, DELAY> Epd2in13b<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn set_display_update_control(
        &mut self,
        spi: &mut SPI,
        display_update_control: DisplayUpdateControl,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(
            spi,
            Command::DisplayUpdateControl1,
            &display_update_control.to_bytes(),
        )
        .await
    }

    async fn set_border_waveform(
        &mut self,
        spi: &mut SPI,
        borderwaveform: BorderWaveForm,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(
            spi,
            Command::BorderWaveformControl,
            &[borderwaveform.to_u8()],
        )
        .await
    }

    /// Triggers the deep sleep mode
    async fn set_sleep_mode(
        &mut self,
        spi: &mut SPI,
        mode: DeepSleepMode,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(spi, Command::DeepSleepMode, &[mode as u8])
            .await
    }

    async fn set_driver_output(
        &mut self,
        spi: &mut SPI,
        output: DriverOutput,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(spi, Command::DriverOutputControl, &output.to_bytes())
            .await
    }

    /// Sets the data entry mode (ie. how X and Y positions changes when writing
    /// data to RAM)
    async fn set_data_entry_mode(
        &mut self,
        spi: &mut SPI,
        counter_incr_mode: DataEntryModeIncr,
        counter_direction: DataEntryModeDir,
    ) -> Result<(), SPI::Error> {
        let mode = counter_incr_mode as u8 | counter_direction as u8;
        self.cmd_with_data(spi, Command::DataEntryModeSetting, &[mode])
            .await
    }

    /// Sets both X and Y pixels ranges
    async fn set_ram_area(
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
        )
        .await?;

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
        .await
    }

    /// Sets both X and Y pixels counters when writing data to RAM
    async fn set_ram_address_counters(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        x: u32,
        y: u32,
    ) -> Result<(), SPI::Error> {
        self.wait_until_idle(spi, delay).await?;
        self.cmd_with_data(spi, Command::SetRamXAddressCounter, &[(x >> 3) as u8])
            .await?;

        self.cmd_with_data(
            spi,
            Command::SetRamYAddressCounter,
            &[y as u8, (y >> 8) as u8],
        )
        .await?;
        Ok(())
    }

    async fn command(&mut self, spi: &mut SPI, command: Command) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, command).await
    }

    async fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data).await
    }

    async fn clear_achromatic_frame(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        match self.background_color {
            TriColor::White => {
                self.command(spi, Command::WriteRam).await?;
                self.interface
                    .data_x_times(
                        spi,
                        0xFF,
                        buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                    )
                    .await?;
            }
            TriColor::Chromatic => {
                self.command(spi, Command::WriteRam).await?;
                self.interface
                    .data_x_times(
                        spi,
                        0xFF,
                        buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                    )
                    .await?;
            }
            TriColor::Black => {
                self.command(spi, Command::WriteRam).await?;
                self.interface
                    .data_x_times(
                        spi,
                        0x00,
                        buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                    )
                    .await?;
            }
        }

        Ok(())
    }

    async fn clear_chromatic_frame(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        match self.background_color {
            TriColor::White => {
                self.command(spi, Command::WriteRam).await?;
                self.interface
                    .data_x_times(
                        spi,
                        0x00,
                        buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                    )
                    .await?;
            }
            TriColor::Chromatic => {
                self.command(spi, Command::WriteRam).await?;
                self.interface
                    .data_x_times(
                        spi,
                        0xFF,
                        buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                    )
                    .await?;
            }
            TriColor::Black => {
                self.command(spi, Command::WriteRam).await?;
                self.interface
                    .data_x_times(
                        spi,
                        0x00,
                        buffer_len(WIDTH as usize, HEIGHT as usize) as u32,
                    )
                    .await?;
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
