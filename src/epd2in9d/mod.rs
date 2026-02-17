//! A simple Driver for the Waveshare 2.9" D E-Ink Display via SPI
//!
//!
//! 参考[Waveshare](https://www.waveshare.net/wiki/2.9inch_e-Paper_HAT_%28D%29)的文档/例程进行构建
//!
//! Specification: <https://www.waveshare.net/w/upload/b/b5/2.9inch_e-Paper_%28D%29_Specification.pdf>

use core::slice::from_raw_parts;

use embedded_hal::{
    delay::DelayNs,
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};

use crate::interface::DisplayInterface;
use crate::traits::{InternalWiAdditions, RefreshLut, WaveshareDisplay};

//The Lookup Tables for the Display
mod constants;
use crate::epd2in9d::constants::*;

/// Width of Epd2in9d in pixels
pub const WIDTH: u32 = 128;
/// Height of Epd2in9d in pixels
pub const HEIGHT: u32 = 296;
/// EPD_ARRAY of Epd2in9d in pixels
/// WIDTH / 8 * HEIGHT
pub const EPD_ARRAY: u32 = 4736;
/// Default Background Color (white)
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::Black;
const IS_BUSY_LOW: bool = true;
const SINGLE_BYTE_WRITE: bool = true;

use crate::color::Color;

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Display with Fullsize buffer for use with the 2in9 EPD D
#[cfg(feature = "graphics")]
pub type Display2in9d = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Epd2in9d driver
///
pub struct Epd2in9d<'a, SPI, BUSY, DC, RST, DELAY> {
    /// SPI
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    /// Color
    // background_color: Color,
    color: Color,
    /// Refresh LUT
    refresh: RefreshLut,
    // Storing old data for partial refreshes
    old_data: &'a [u8],
    // 标记是否局刷的状态
    is_partial_refresh: bool,
}

impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in9d<'_, SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.reset(delay, 10_000, 2_000);

        //panel setting
        //LUT from OTP，KW-BF   KWR-AF	BWROTP 0f	BWOTP 1f
        self.interface
            .cmd_with_data(spi, Command::PanelSetting, &[0x1f, 0x0D])?;

        //resolution setting
        self.interface
            .cmd_with_data(spi, Command::ResolutionSetting, &[0x80, 0x01, 0x28])?;

        self.interface.cmd(spi, Command::PowerOn)?;
        self.wait_until_idle(spi, delay)?;

        //VCOM AND DATA INTERVAL SETTING
        self.interface
            .cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x97])?;

        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in9d<'_, SPI, BUSY, DC, RST, DELAY>
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
        let old_data: &[u8] = &[];
        let is_partial_refresh = false;

        let mut epd = Epd2in9d {
            interface,
            color,
            refresh: RefreshLut::Full,
            old_data,
            is_partial_refresh,
        };

        epd.init(spi, delay)?;

        Ok(epd)
    }

    fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.is_partial_refresh = false;
        self.interface
            .cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0xf7])?;
        self.interface.cmd(spi, Command::PowerOff)?;
        self.wait_until_idle(spi, delay)?;
        delay.delay_us(100_000);
        self.interface
            .cmd_with_data(spi, Command::DeepSleep, &[0xA5])?;

        Ok(())
    }

    fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay)?;
        Ok(())
    }

    fn set_background_color(&mut self, background_color: Color) {
        self.color = background_color;
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

    // Corresponds to the Display function.
    // Used to write the data to be displayed to the screen SRAM.
    fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        if self.is_partial_refresh {
            // Modify local refresh status if full refresh is performed.
            self.is_partial_refresh = false;
        }
        self.wait_until_idle(spi, delay)?;

        self.interface.cmd(spi, Command::DataStartTransmission1)?;
        self.interface.data_x_times(spi, 0xFF, EPD_ARRAY)?;

        self.interface
            .cmd_with_data(spi, Command::DataStartTransmission2, buffer)?;
        self.old_data = unsafe { from_raw_parts(buffer.as_ptr(), buffer.len()) };
        Ok(())
    }

    // 这个是DisplayPart
    // Partial refresh write address and data
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
        if !self.is_partial_refresh {
            // Initialize only on first call
            self.set_part_reg(spi, delay)?;
            self.is_partial_refresh = true;
        }
        self.interface.cmd(spi, Command::PartialIn)?;

        self.interface.cmd(spi, Command::PartialWindow)?;
        self.interface.data(spi, &[(x - x % 8) as u8])?;
        self.interface
            .data(spi, &[(((x - x % 8) + width - 1) - 1) as u8])?;
        self.interface.data(spi, &[(y / 256) as u8])?;
        self.interface.data(spi, &[(y % 256) as u8])?;
        self.interface
            .data(spi, &[((y + height - 1) / 256) as u8])?;
        self.interface
            .data(spi, &[((y + height - 1) % 256 - 1) as u8])?;
        self.interface.data(spi, &[0x28])?;

        self.interface
            .cmd_with_data(spi, Command::DataStartTransmission1, self.old_data)?;

        self.interface
            .cmd_with_data(spi, Command::DataStartTransmission2, buffer)?;
        self.old_data = unsafe { from_raw_parts(buffer.as_ptr(), buffer.len()) };

        Ok(())
    }

    /// actually is the "Turn on Display" sequence
    fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, Command::DisplayRefresh)?;
        delay.delay_us(1_000);
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
        self.interface.cmd(spi, Command::DataStartTransmission1)?;
        self.interface.data_x_times(spi, 0x00, EPD_ARRAY)?;

        self.interface.cmd(spi, Command::DataStartTransmission2)?;
        self.interface.data_x_times(spi, 0xFF, EPD_ARRAY)?;

        self.display_frame(spi, delay)?;

        Ok(())
    }

    fn set_lut(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        if let Some(refresh_lut) = refresh_rate {
            self.refresh = refresh_lut;
        }
        self.set_lut_helper(
            spi, delay, &LUT_VCOM1, &LUT_WW1, &LUT_BW1, &LUT_WB1, &LUT_BB1,
        )?;

        Ok(())
    }

    fn wait_until_idle(&mut self, _spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.interface.wait_until_idle(delay, IS_BUSY_LOW);
        Ok(())
    }
}

impl<SPI, BUSY, DC, RST, DELAY> Epd2in9d<'_, SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    // /// Wake Up Screen
    // ///
    // /// After the screen sleeps, it enters deep sleep mode. If you need to refresh the screen while in deep sleep mode, you must first execute awaken().
    // /// Wake the screen.
    // fn awaken(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
    //     // reset the device
    //     self.interface.reset(delay, 20_000, 2_000);
    //     self.wait_until_idle(spi, delay)?;

    //     // panel setting
    //     // LUT from OTP，KW-BF   KWR-AF	BWROTP 0f	BWOTP 1f
    //     self.interface
    //         .cmd_with_data(spi, Command::PanelSetting, &[0x1f])?;

    //     // resolution setting
    //     self.interface
    //         .cmd_with_data(spi, Command::ResolutionSetting, &[0x80, 0x01, 0x28])?;

    //     // VCOM AND DATA INTERVAL SETTING
    //     self.interface
    //         .cmd(spi, Command::VcomAndDataIntervalSetting)?;
    //     Ok(())
    // }

    fn set_part_reg(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Reset the EPD driver circuit
        //TODO: 这里在微雪的例程中反复刷新了3次，后面有显示问题再进行修改
        self.interface.reset(delay, 10_000, 2_000);

        // Power settings
        //TODO: The data in the document is [0x03,0x00,0x2b,0x2b,0x09].
        self.interface.cmd_with_data(
            spi,
            Command::PowerSetting,
            &[0x03, 0x00, 0x2b, 0x2b, 0x03],
        )?;

        // Soft start
        self.interface
            .cmd_with_data(spi, Command::BoosterSoftStart, &[0x17, 0x17, 0x17])?;

        // Panel settings
        self.interface
            .cmd_with_data(spi, Command::PanelSetting, &[0xbf, 0x0D])?;

        // Setting the refresh rate
        // 3a 100HZ | 29 150Hz | 39 200HZ | 31 171HZ
        // 3a is used in the example
        self.interface
            .cmd_with_data(spi, Command::PllControl, &[0x3C])?;

        // Resolution Settings
        self.interface
            .cmd_with_data(spi, Command::ResolutionSetting, &[0x80, 0x01, 0x28])?;

        // vcom_DC settings
        self.interface
            .cmd_with_data(spi, Command::VcmDcSetting, &[0x12])?;

        self.set_lut(spi, delay, None)?;

        // Power on
        // self.interface.cmd_with_data(
        //     spi,
        //     Command::PowerOn,
        //     &[0x04],
        // );
        self.interface.cmd(spi, Command::PowerOn)?;

        // Get the BUSY level, high to continue, low to wait for the screen to respond.
        //TODO: This is the recommended step in the documentation, but I've ignored it since I've seen other screens that don't wait.
        self.wait_until_idle(spi, delay)?;

        // vcom and data interval settings
        // self.interface
        //     .cmd_with_data(spi, Command::VcomAndDataIntervalSetting, &[0x97])?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn set_lut_helper(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        lut_vcom: &[u8],
        lut_ww: &[u8],
        lut_bw: &[u8],
        lut_wb: &[u8],
        lut_bb: &[u8],
    ) -> Result<(), SPI::Error> {
        let _ = delay;
        // LUT VCOM
        self.interface
            .cmd_with_data(spi, Command::LutForVcom, lut_vcom)?;

        // LUT WHITE to WHITE
        self.interface
            .cmd_with_data(spi, Command::LutWhiteToWhite, lut_ww)?;

        // LUT BLACK to WHITE
        self.interface
            .cmd_with_data(spi, Command::LutBlackToWhite, lut_bw)?;

        // LUT WHITE to BLACK
        self.interface
            .cmd_with_data(spi, Command::LutWhiteToBlack, lut_wb)?;

        // LUT BLACK to BLACK
        self.interface
            .cmd_with_data(spi, Command::LutBlackToBlack, lut_bb)?;
        Ok(())
    }
}
