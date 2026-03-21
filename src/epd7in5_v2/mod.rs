//! A simple Driver for the Waveshare 7.5" E-Ink Display (V2) via SPI
//!
//! # References
//!
//! - [Datasheet](https://www.waveshare.com/wiki/7.5inch_e-Paper_HAT)
//! - [Waveshare C driver](https://github.com/waveshare/e-Paper/blob/702def0/RaspberryPi%26JetsonNano/c/lib/e-Paper/EPD_7in5_V2.c)
//! - [Waveshare Python driver](https://github.com/waveshare/e-Paper/blob/702def0/RaspberryPi%26JetsonNano/python/lib/waveshare_epd/epd7in5_V2.py)
//!
//! Important note for V2:
//! Revision V2 has been released on 2019.11, the resolution is upgraded to 800×480, from 640×384 of V1.
//! The hardware and interface of V2 are compatible with V1, however, the related software should be updated.

use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::color::Color;
use crate::interface::DisplayInterface;
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Full size buffer for use with the 7in5 v2 EPD
#[cfg(feature = "graphics")]
pub type Display7in5 = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Width of the display
pub const WIDTH: u32 = 800;
/// Height of the display
pub const HEIGHT: u32 = 480;
/// Default Background Color
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;
const IS_BUSY_LOW: bool = true;
const SINGLE_BYTE_WRITE: bool = false;

/// Epd7in5 (V2) driver
pub struct Epd7in5<SPI, BUSY, DC, RST, DELAY> {
    /// Connection Interface
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Background Color
    color: Color,
    /// LUT refresh mode
    refresh: RefreshLut,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd7in5<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Reset the device
        self.interface.reset(delay, 10_000, 2_000);

        // V2 procedure as described here:
        // https://github.com/waveshare/e-Paper/blob/master/RaspberryPi%26JetsonNano/python/lib/waveshare_epd/epd7in5bc_V2.py
        // and as per specs:
        // https://www.waveshare.com/w/upload/6/60/7.5inch_e-Paper_V2_Specification.pdf

        // Only edits settings that deviate from defaults in OTP

        self.cmd_with_data(spi, Command::BoosterSoftStart, &[0x17, 0x17, 0x28, 0x17])?;
        self.cmd(spi, Command::PowerOn)?;
        delay.delay_ms(100);
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::PanelSetting, &[0x1F])?; // Sets black and white as opposed to black, white and red.
        self.cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x29, 0x07])?; // Sets NEW/OLD buffer behavior and polarity
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd7in5<SPI, BUSY, DC, RST, DELAY>
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

        let mut epd = Epd7in5 {
            interface,
            color,
            refresh: RefreshLut::default(),
        };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.cmd(spi, Command::PowerOff)?;
        self.wait_until_idle(spi, delay)?;
        self.cmd_with_data(spi, Command::DeepSleep, &[0xA5])?;
        Ok(())
    }

    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(spi, Command::DataStartTransmission2, buffer)?;
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
        if width == 0 || height == 0 {
            return Ok(());
        }

        if x % 8 != 0 || width % 8 != 0 {
            panic!("x and width should be byte-aligned (multiples of 8).")
        }

        let expected_size = buffer_len(width as usize, height as usize);
        let actual_size = buffer.len();
        if actual_size != expected_size {
            panic!("Buffer is incorrect size. Expected: {expected_size}. Actual: {actual_size}.")
        }

        let x_end = x + width - 1;

        let hrst_upper = (x >> 8) as u8;
        let hrst_lower = x as u8;
        let hred_upper = (x_end >> 8) as u8;
        let hred_lower = x_end as u8;

        let y_end = y + height - 1;

        let vrst_upper = (y >> 8) as u8;
        let vrst_lower = y as u8;
        let vred_upper = (y_end >> 8) as u8;
        let vred_lower = y_end as u8;

        let pt_scan = 0x01; // Gates scan both inside and outside of the partial window. (default)

        self.cmd(spi, Command::PartialIn)?;
        self.cmd_with_data(
            spi,
            Command::PartialWindow,
            &[
                hrst_upper, hrst_lower, hred_upper, hred_lower, vrst_upper, vrst_lower, vred_upper,
                vred_lower, pt_scan,
            ],
        )?;

        self.update_frame(spi, buffer, delay)?;

        self.cmd(spi, Command::PartialOut)?;
        Ok(())
    }

    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.cmd(spi, Command::DisplayRefresh)?;
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
        self.cmd(spi, Command::DisplayRefresh)?;
        self.wait_until_idle(spi, delay)?;
        Ok(())
    }

    fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.cmd(spi, Command::DataStartTransmission2)?;
        self.interface.data_x_times(spi, 0xFF, WIDTH / 8 * HEIGHT)?;

        self.cmd(spi, Command::DisplayRefresh)?;
        self.wait_until_idle(spi, delay)?;
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
        _delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        let refresh_rate = if let Some(refresh_rate) = refresh_rate {
            refresh_rate
        } else {
            RefreshLut::Full
        };

        if self.refresh == refresh_rate {
            return Ok(());
        }

        if self.refresh == RefreshLut::Quick {
            // Return booster power settings to default
            self.cmd_with_data(spi, Command::BoosterSoftStart, &[0x17, 0x17, 0x28, 0x17])?;
        }

        // NOT DOCUMENTED IN OFFICIAL SPEC: Override temperature-based LUT selection for fast refresh mode
        // The cascade temperature setting (0xE5) accepts out-of-range values (beyond the 49°C max)
        // which the manufacturer uses as custom LUT indices in OTP memory.
        // This is used in official demo's and libraries, but is not documented behavior.
        match refresh_rate {
            RefreshLut::Full => {
                // This disables custom LUT indices and uses normal temperature-based operation
                self.cmd_with_data(spi, Command::CascadeSetting, &[0x00])?;
            }
            RefreshLut::Quick => {
                // Booster power settings for quick LUT
                self.cmd_with_data(spi, Command::BoosterSoftStart, &[0x27, 0x27, 0x18, 0x17])?;
                // This selects a speed-optimized waveform: fewer voltage transitions mean faster updates
                // (~2s vs ~4s) at the cost of increased ghosting.
                self.cmd_with_data(spi, Command::CascadeSetting, &[0x02])?;
                self.cmd_with_data(spi, Command::ForceTemperature, &[0x5A])?;
            }
            RefreshLut::PartialRefresh => {
                // This waveform applies gentle voltage transitions that update only the changed
                // pixels without the full-screen flicker normally required to clear ghosting.
                // Will accumulate ghosting over many cycles - requires occasional full refresh to
                // maintain image quality.
                self.cmd_with_data(spi, Command::CascadeSetting, &[0x02])?;
                self.cmd_with_data(spi, Command::ForceTemperature, &[0x6E])?;
            }
        }

        self.refresh = refresh_rate;

        Ok(())
    }

    fn wait_until_idle(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface
            .wait_until_idle_with_cmd(spi, delay, IS_BUSY_LOW, Command::GetStatus)
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd7in5<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn cmd(&mut self, spi: &mut SPI, command: Command) -> Result<(), SPI::Error> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epd_size() {
        assert_eq!(WIDTH, 800);
        assert_eq!(HEIGHT, 480);
        assert_eq!(DEFAULT_BACKGROUND_COLOR, Color::White);
    }
}
