//! A driver for the Waveshare 12.48"(B) E-Ink Display (V2) via SPI
//! (also known as [GDEY1248Z51](https://www.good-display.com/product/422.html))
//!
//! # References
//!
//! - [Datasheet](https://files.waveshare.com/upload/b/b4/12.48inch_e-Paper_B_V2_Specification.pdf)
//! - [Wiki](https://www.waveshare.com/wiki/12.48inch_e-Paper_Module_(B))
//! - [Waveshare C drivers](https://github.com/waveshareteam/12.48inch-e-paper/)
//!

mod command;
mod config;

use embedded_hal::digital::{InputPin, OutputPin, PinState};

use crate::interface::{DelayNs, SpiBus};
pub use crate::rect::Rect;
use command::Command;
pub use config::*;

/// A collection of peripherals controlling the EPD
///
/// The display is composed of 4 sub-displays arranged like so:
/// ```md
///     0        648      1304
///   0 +--------+--------+
///     |   S2   |   M2   |
/// 492 +--------+--------+
///     |   M1   |   S1   |
/// 984 +--------+--------+
/// ```
/// Resolution of `S2` and `M1` is 648 x 492,
/// resolution of `S1` and `M2` is 656 x 492.
///
pub struct Peripherals<INPUT, OUTPUT, SPI>
where
    INPUT: InputPin,
    OUTPUT: OutputPin,
    SPI: SpiBus<u8>,
{
    /// SPI bus shared by all sub-displays.
    pub spi: SPI,
    /// Chip select signal for `M1`.
    pub m1_cs: OUTPUT,
    /// Chip select signal for `S1`.
    pub s1_cs: OUTPUT,
    /// Chip select signal for `M2`.
    pub m2_cs: OUTPUT,
    /// Chip select signal for `S2`.
    pub s2_cs: OUTPUT,
    /// Shared "command/data" signal for `M1` and `S1`.
    pub m1s1_dc: OUTPUT,
    /// Shared "command/data" signal for `M2` and `S2`.
    pub m2s2_dc: OUTPUT,
    /// Shared reset signal for `M1` and `S1`.
    pub m1s1_rst: OUTPUT,
    /// Shared reset signal for `M2` and `S2`.
    pub m2s2_rst: OUTPUT,
    /// "Busy" signal from `M1`.
    pub m1_busy: INPUT,
    /// "Busy" signal from `S1`.
    pub s1_busy: INPUT,
    /// "Busy" signal from `M2`.
    pub m2_busy: INPUT,
    /// "Busy" signal from `S2`.
    pub s2_busy: INPUT,
}

/// EPD width
pub const WIDTH: u32 = 1304;
/// EPD height
pub const HEIGHT: u32 = 984;

const S2_WIDTH: u32 = 648;
const S2_HEIGHT: u32 = 492;

const FULL_RECT: Rect = Rect {
    x: 0,
    y: 0,
    w: WIDTH,
    h: HEIGHT,
};

const S2_RECT: Rect = Rect {
    x: 0,
    y: 0,
    w: S2_WIDTH,
    h: S2_HEIGHT,
};

const M2_RECT: Rect = Rect {
    x: S2_WIDTH,
    y: 0,
    w: WIDTH - S2_WIDTH,
    h: S2_HEIGHT,
};

const M1_RECT: Rect = Rect {
    x: 0,
    y: S2_HEIGHT,
    w: S2_WIDTH,
    h: HEIGHT - S2_HEIGHT,
};

const S1_RECT: Rect = Rect {
    x: S2_WIDTH,
    y: S2_HEIGHT,
    w: WIDTH - S2_WIDTH,
    h: HEIGHT - S2_HEIGHT,
};

type CS = u8;
const CS_M1: CS = 0b0001;
const CS_S1: CS = 0b0010;
const CS_M2: CS = 0b0100;
const CS_S2: CS = 0b1000;
const CS_ALL: CS = CS_M1 | CS_S1 | CS_M2 | CS_S2;
const CS_DATA: CS = 0b10000;

/// Waveshare 12.48"(B)
pub struct EpdDriver<INPUT, OUTPUT, SPI, DELAY>
where
    INPUT: InputPin,
    OUTPUT: OutputPin,
    SPI: SpiBus<u8>,
    DELAY: DelayNs,
{
    peris: Peripherals<INPUT, OUTPUT, SPI>,
    delay: DELAY,
    control_state: CS,
}

#[maybe_async::maybe_async]
impl<INPUT, OUTPUT, SPI, DELAY> EpdDriver<INPUT, OUTPUT, SPI, DELAY>
where
    INPUT: InputPin,
    INPUT::Error: core::fmt::Debug,
    OUTPUT: OutputPin,
    OUTPUT::Error: core::fmt::Debug,
    SPI: SpiBus<u8>,
    SPI::Error: core::fmt::Debug,
    DELAY: DelayNs,
{
    /// Constructs a new instance of the EpdDriver.  
    /// Normally should be followd by calls to [`reset()`](EpdDriver::reset) and [`init()`](EpdDriver::init)
    /// to wake up the display and initialize its registers.
    pub fn new(peris: Peripherals<INPUT, OUTPUT, SPI>, delay: DELAY) -> Self {
        EpdDriver {
            peris,
            delay,
            control_state: 0,
        }
    }

    /// Consumes EpdDriver, releasing peripherals to the caller.
    pub fn into_peripherals(self) -> Peripherals<INPUT, OUTPUT, SPI> {
        self.peris
    }

    /// Reset the display, potentially waking it up from deep sleep.
    /// Normally should be followed by a call to [`init()`](EpdDriver::init).
    pub async fn reset(&mut self) -> Result<(), OUTPUT::Error> {
        drop(self.peris.m1_cs.set_high());
        drop(self.peris.s1_cs.set_high());
        drop(self.peris.m2_cs.set_high());
        drop(self.peris.s2_cs.set_high());
        drop(self.peris.m1s1_dc.set_low());
        drop(self.peris.m2s2_dc.set_low());
        self.control_state = 0;

        self.peris.m1s1_rst.set_high()?;
        self.peris.m2s2_rst.set_high()?;
        self.delay.delay_ms(1).await;

        self.peris.m1s1_rst.set_low()?;
        self.delay.delay_us(100).await; // min RST low = 50us
        self.peris.m1s1_rst.set_high()?;
        self.delay.delay_ms(100).await; // min wait after RST = 10ms

        self.peris.m2s2_rst.set_low()?;
        self.delay.delay_us(100).await;
        self.peris.m2s2_rst.set_high()?;
        self.delay.delay_ms(100).await;

        Ok(())
    }

    /// Initialize display registers.
    pub async fn init(&mut self, config: &Config) -> Result<(), SPI::Error> {
        // booster soft start
        self.cmd_with_data(CS_ALL, Command::BoosterSoftStart, &[0x17, 0x17, 0x39, 0x17])
            .await?;

        // resolution setting
        fn resolution_data(rect: Rect) -> [u8; 4] {
            [
                (rect.w / 256) as u8,
                (rect.w % 256) as u8,
                (rect.h / 256) as u8,
                (rect.h % 256) as u8,
            ]
        }
        self.cmd_with_data(CS_M1, Command::TconResolution, &resolution_data(M1_RECT))
            .await?;
        self.cmd_with_data(CS_S1, Command::TconResolution, &resolution_data(S1_RECT))
            .await?;
        self.cmd_with_data(CS_M2, Command::TconResolution, &resolution_data(M2_RECT))
            .await?;
        self.cmd_with_data(CS_S2, Command::TconResolution, &resolution_data(S2_RECT))
            .await?;

        self.cmd_with_data(CS_ALL, Command::DualSPI, &[0x20])
            .await?;
        self.cmd_with_data(CS_ALL, Command::TconSetting, &[0x22])
            .await?;
        self.cmd_with_data(CS_ALL, Command::PowerSaving, &[0x00])
            .await?;
        self.cmd_with_data(CS_ALL, Command::CascadeSetting, &[0x03])
            .await?;
        self.cmd_with_data(CS_ALL, Command::ForceTemperature, &[25])
            .await?;

        self.set_mode(config).await?;

        self.flush().await
    }

    /// Set data "polarity", waveform lookup table mode, etc, without re-initializing anything else.
    pub async fn set_mode(&mut self, config: &Config) -> Result<(), SPI::Error> {
        let ddx = match (config.inverted_r, config.inverted_kw) {
            (false, true) => 0b00,
            (false, false) => 0b01,
            (true, true) => 0b10,
            (true, false) => 0b11,
        };
        let ddx0 = ddx & 1 == 1;
        let bdv = match (ddx0, config.border_lut) {
            (false, BorderLUT::LUTBD) => 0b00,
            (false, BorderLUT::LUTR) => 0b01,
            (false, BorderLUT::LUTW) => 0b10,
            (false, BorderLUT::LUTK) => 0b11,
            (true, BorderLUT::LUTK) => 0b00,
            (true, BorderLUT::LUTW) => 0b01,
            (true, BorderLUT::LUTR) => 0b10,
            (true, BorderLUT::LUTBD) => 0b11,
        };

        let reg = (config.external_lut as u8) << 5;
        self.cmd_with_data(CS_M1, Command::PanelSetting, &[reg | 0x0F])
            .await?;
        self.cmd_with_data(CS_S1, Command::PanelSetting, &[reg | 0x0F])
            .await?;
        self.cmd_with_data(CS_M2, Command::PanelSetting, &[reg | 0x03])
            .await?;
        self.cmd_with_data(CS_S2, Command::PanelSetting, &[reg | 0x03])
            .await?;

        let bdv = bdv << 4;
        self.cmd_with_data(
            CS_ALL,
            Command::VcomAndDataIntervalSetting,
            &[bdv | ddx, 0x07],
        )
        .await?;

        self.flush().await
    }

    /// Fill data1 buffer with pixels:
    /// - data1 containes the black/white image channel,
    /// - data2 contains the red/not red channel.
    ///
    /// `pixels` may contain a lesser number of rows than the window being written,
    /// in which case it will be treated as circular.
    pub async fn write_data1(&mut self, pixels: &[u8]) -> Result<(), SPI::Error> {
        self.write_window_data(Command::DataStartTransmission1, FULL_RECT, pixels)
            .await?;
        self.flush().await
    }

    /// Fill data2 buffer with pixels.
    /// See also [`write_data1`](EpdDriver::write_data1).
    pub async fn write_data2(&mut self, pixels: &[u8]) -> Result<(), SPI::Error> {
        self.write_window_data(Command::DataStartTransmission2, FULL_RECT, pixels)
            .await?;
        self.flush().await
    }

    /// Fill a window in the data1 buffer with pixels.
    /// See also [`write_data1`](EpdDriver::write_data1).
    pub async fn write_data1_partial(
        &mut self,
        window: Rect,
        pixels: &[u8],
    ) -> Result<(), SPI::Error> {
        self.write_partial(Command::DataStartTransmission1, window, pixels)
            .await?;
        self.flush().await
    }

    /// Fill a window in the data2 buffer with pixels.
    /// See also [`write_data1`](EpdDriver::write_data1).
    pub async fn write_data2_partial(
        &mut self,
        window: Rect,
        pixels: &[u8],
    ) -> Result<(), SPI::Error> {
        self.write_partial(Command::DataStartTransmission2, window, pixels)
            .await?;
        self.flush().await
    }

    /// Store VCOM Look-Up Table.
    ///
    /// If LUT data is shorter than expected, the rest is filled with zeroes.<br>
    /// Note that stored lookup tables need to be activated by setting
    /// [`Config::external_lut`](config::Config::external_lut)`=true`.
    pub async fn set_lutc(&mut self, data: &[u8]) -> Result<(), SPI::Error> {
        self.set_lut(Command::LutC, data, 60).await
    }

    /// Store White-to-White Look-Up Table.
    /// See also [`write_data1`](EpdDriver::set_lutc).
    pub async fn set_lutww(&mut self, data: &[u8]) -> Result<(), SPI::Error> {
        self.set_lut(Command::LutWW, data, 42).await
    }

    /// Store Black-to-White (KW mode) / Red (KWR mode) Look-Up Table.
    /// See also [`write_data1`](EpdDriver::set_lutc).
    pub async fn set_lutkw_lutr(&mut self, data: &[u8]) -> Result<(), SPI::Error> {
        self.set_lut(Command::LutKW_LutR, data, 60).await
    }

    /// Store White-to-Black (KW mode) / White (KWR mode) Look-Up Table.
    /// See also [`write_data1`](EpdDriver::set_lutc).
    pub async fn set_lutwk_lutw(&mut self, data: &[u8]) -> Result<(), SPI::Error> {
        self.set_lut(Command::LutWK_LutW, data, 60).await
    }

    /// Store Black-to-Black (KW mode) / Black (KWR mode) Look-Up Table.
    /// See also [`write_data1`](EpdDriver::set_lutc).
    pub async fn set_lutkk_lutk(&mut self, data: &[u8]) -> Result<(), SPI::Error> {
        self.set_lut(Command::LutKK_LutK, data, 60).await
    }

    /// Store Border Look-Up Table.
    /// See also [`write_data1`](EpdDriver::set_lutc).
    pub async fn set_lutbd(&mut self, data: &[u8]) -> Result<(), SPI::Error> {
        self.set_lut(Command::LutBD, data, 42).await
    }

    async fn set_lut(
        &mut self,
        cmd: Command,
        data: &[u8],
        reqd_len: usize,
    ) -> Result<(), SPI::Error> {
        self.cmd_with_data(CS_ALL, cmd, data).await?;
        if data.len() < reqd_len {
            let zeroes = [0; 60];
            self.spi_write(CS_ALL | CS_DATA, &zeroes[..reqd_len - data.len()])
                .await?;
        }
        self.flush().await
    }

    /// Refresh the entire display.
    pub async fn refresh_display(&mut self) -> Result<(), SPI::Error> {
        self.begin_refresh_display().await?;
        drop(self.wait_ready(CS_ALL).await);
        Ok(())
    }

    /// Asynchronous version of [`refresh_display`](EpdDriver::refresh_display).
    /// Use [`is_busy`](EpdDriver::is_busy) to poll for completion.
    pub async fn begin_refresh_display(&mut self) -> Result<(), SPI::Error> {
        self.cmd(CS_ALL, Command::PowerOn).await?;
        drop(self.wait_ready(CS_ALL).await);
        // Appears to be required to reliably trigger display refresh after a power-on.
        self.delay.delay_ms(100).await;

        self.cmd(CS_ALL, Command::DisplayRefresh).await?;

        self.flush().await
    }

    /// Refresh the specified sub-window of the display.  
    ///
    /// Technically, this works, however, after 2+ partial updates, the rest of the displayed image becomes visibly degraded.
    pub async fn refresh_display_partial(&mut self, window: Rect) -> Result<(), SPI::Error> {
        self.begin_refresh_display_partial(window).await?;

        drop(self.wait_ready(CS_ALL).await);
        Ok(())
    }

    /// Asynchronous version of [`refresh_display_partial`](EpdDriver::refresh_display_partial).
    /// Use [`is_busy`](EpdDriver::is_busy) to poll for completion.
    pub async fn begin_refresh_display_partial(&mut self, window: Rect) -> Result<(), SPI::Error> {
        self.setup_partial_windows(window).await?;

        self.cmd(CS_ALL, Command::PowerOn).await?;
        drop(self.wait_ready(CS_ALL).await);
        self.delay.delay_ms(100).await;

        self.cmd(CS_ALL, Command::PartialIn).await?;
        self.cmd(CS_ALL, Command::DisplayRefresh).await?;
        self.cmd(CS_ALL, Command::PartialOut).await?;

        self.flush().await
    }

    /// Turn off booster, controller, source driver, gate driver, VCOM, and temperature sensor.
    /// However, the contents of the data memory buffers will be retained.
    pub async fn power_off(&mut self) -> Result<(), SPI::Error> {
        self.cmd(CS_ALL, Command::PowerOff).await?;
        drop(self.wait_ready(CS_ALL).await);

        self.flush().await
    }

    /// Put display into deep sleep.  Only [`reset()`](EpdDriver::reset) can bring it out of this state.
    /// The contents of the data memory buffers will be lost.
    pub async fn hibernate(&mut self) -> Result<(), SPI::Error> {
        self.cmd(CS_ALL, Command::PowerOff).await?;
        drop(self.wait_ready(CS_ALL).await);

        self.cmd_with_data(CS_ALL, Command::DeepSleep, &[0xA5])
            .await?;

        self.flush().await
    }

    async fn setup_partial_windows(&mut self, window: Rect) -> Result<(), SPI::Error> {
        let s2_part = window.intersect(S2_RECT).sub_offset(S2_RECT.x, S2_RECT.y);
        let m2_part = window.intersect(M2_RECT).sub_offset(M2_RECT.x, M2_RECT.y);
        let m1_part = window.intersect(M1_RECT).sub_offset(M1_RECT.x, M1_RECT.y);
        let s1_part = window.intersect(S1_RECT).sub_offset(S1_RECT.x, S1_RECT.y);

        fn partial_window_data(window: Rect, reverse_scan: Option<u32>) -> [u8; 9] {
            if window.is_empty() {
                [0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x01]
            } else {
                let start_x = match reverse_scan {
                    Some(width) => width - window.x - window.w,
                    None => window.x,
                };
                let end_x = start_x + window.w - 1;
                let start_y = window.y;
                let end_y = start_y + window.h - 1;
                [
                    (start_x / 256) as u8,
                    (start_x % 256) as u8,
                    (end_x / 256) as u8,
                    (end_x % 256) as u8,
                    (start_y / 256) as u8,
                    (start_y % 256) as u8,
                    (end_y / 256) as u8,
                    (end_y % 256) as u8,
                    0x01,
                ]
            }
        }

        self.cmd_with_data(
            CS_S2,
            Command::PartialWindow,
            &partial_window_data(s2_part, Some(S2_RECT.w)),
        )
        .await?;
        self.cmd_with_data(
            CS_M2,
            Command::PartialWindow,
            &partial_window_data(m2_part, Some(M2_RECT.w)),
        )
        .await?;
        self.cmd_with_data(
            CS_M1,
            Command::PartialWindow,
            &partial_window_data(m1_part, None),
        )
        .await?;
        self.cmd_with_data(
            CS_S1,
            Command::PartialWindow,
            &partial_window_data(s1_part, None),
        )
        .await?;

        Ok(())
    }

    async fn write_partial(
        &mut self,
        transmission_cmd: Command,
        window: Rect,
        pixels: &[u8],
    ) -> Result<(), SPI::Error> {
        if window.x % 8 != 0 || window.w % 8 != 0 {
            panic!("Window is not 8-aligned horizontally");
        }

        self.cmd(CS_ALL, Command::PartialIn).await?;

        self.setup_partial_windows(window).await?;
        self.write_window_data(transmission_cmd, window, pixels)
            .await?;

        self.cmd(CS_ALL, Command::PartialOut).await
    }

    // Send data to each sub-display for the window area that overlaps with it.
    async fn write_window_data(
        &mut self,
        transmission_cmd: Command,
        window: Rect,
        pixels: &[u8],
    ) -> Result<(), SPI::Error> {
        assert!(!pixels.is_empty());

        let s2_part = window.intersect(S2_RECT);
        let s1_part = window.intersect(S1_RECT);

        let top_rows = s2_part.h as usize;
        let bottom_rows = s1_part.h as usize;
        let left_bytes = (s2_part.w / 8) as usize;
        let right_bytes = (s1_part.w / 8) as usize;

        let row_offset = |row| {
            let offset = row * (left_bytes + right_bytes);
            if offset < pixels.len() {
                offset
            } else {
                // Wrap around
                offset % pixels.len()
            }
        };

        if top_rows > 0 {
            if left_bytes > 0 {
                self.cmd(CS_S2, transmission_cmd).await?;
                for y in 0..top_rows {
                    let begin = row_offset(y);
                    let end = begin + left_bytes;
                    self.spi_write(CS_S2 | CS_DATA, &pixels[begin..end]).await?;
                }
            }

            if right_bytes > 0 {
                self.cmd(CS_M2, transmission_cmd).await?;
                for y in 0..top_rows {
                    let begin = row_offset(y) + left_bytes;
                    let end = begin + right_bytes;
                    self.spi_write(CS_M2 | CS_DATA, &pixels[begin..end]).await?;
                }
            }
        }

        if bottom_rows > 0 {
            if left_bytes > 0 {
                self.cmd(CS_M1, transmission_cmd).await?;
                for y in 0..bottom_rows {
                    let begin = row_offset(top_rows + y);
                    let end = begin + left_bytes;
                    self.spi_write(CS_M1 | CS_DATA, &pixels[begin..end]).await?;
                }
            }

            if right_bytes > 0 {
                self.cmd(CS_S1, transmission_cmd).await?;
                for y in 0..bottom_rows {
                    let begin = row_offset(top_rows + y) + left_bytes;
                    let end = begin + right_bytes;
                    self.spi_write(CS_S1 | CS_DATA, &pixels[begin..end]).await?;
                }
            }
        }

        Ok(())
    }

    async fn cmd(&mut self, chips: CS, command: Command) -> Result<(), SPI::Error> {
        self.spi_write(chips, &[command as u8]).await
    }

    async fn cmd_with_data(
        &mut self,
        chips: CS,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.spi_write(chips, &[command as u8]).await?;
        self.spi_write(chips | CS_DATA, data).await
    }

    // Set control pins to the specified state, then send data via SPI.
    async fn spi_write(&mut self, control: CS, data: &[u8]) -> Result<(), SPI::Error> {
        if self.control_state != control {
            fn pin_state(high: bool) -> PinState {
                if high {
                    PinState::High
                } else {
                    PinState::Low
                }
            }

            self.peris.spi.flush().await?;
            self.delay.delay_ns(100).await; // Tscc = 20ns, Tchw = 40ns

            // CS is active low
            drop(self.peris.m1_cs.set_state(pin_state(control & CS_M1 == 0)));
            drop(self.peris.s1_cs.set_state(pin_state(control & CS_S1 == 0)));
            drop(self.peris.m2_cs.set_state(pin_state(control & CS_M2 == 0)));
            drop(self.peris.s2_cs.set_state(pin_state(control & CS_S2 == 0)));

            // DC is active high
            let dc = pin_state(control & CS_DATA != 0);
            drop(self.peris.m1s1_dc.set_state(dc));
            drop(self.peris.m2s2_dc.set_state(dc));

            self.delay.delay_ns(100).await; // Tcss = 60ns, Tsds = 30ns
            self.control_state = control;
        }

        self.peris.spi.write(data).await
    }

    // Flush SPI, reset control pins to the default state.
    async fn flush(&mut self) -> Result<(), SPI::Error> {
        self.peris.spi.flush().await?;
        drop(self.peris.m1_cs.set_high());
        drop(self.peris.s1_cs.set_high());
        drop(self.peris.m2_cs.set_high());
        drop(self.peris.s2_cs.set_high());
        drop(self.peris.m1s1_dc.set_low());
        drop(self.peris.m2s2_dc.set_low());
        self.control_state = 0;
        Ok(())
    }

    async fn wait_ready(&mut self, chips: CS) -> Result<(), INPUT::Error> {
        while self.busy_chips(chips)? != 0 {
            self.delay.delay_ms(200).await;
        }
        Ok(())
    }

    fn busy_chips(&mut self, chips: CS) -> Result<CS, INPUT::Error> {
        let mut busy = 0;
        if chips & CS_M1 != 0 && self.peris.m1_busy.is_low()? {
            busy |= CS_M1;
        }
        if chips & CS_S1 != 0 && self.peris.s1_busy.is_low()? {
            busy |= CS_S1;
        }
        if chips & CS_M2 != 0 && self.peris.m2_busy.is_low()? {
            busy |= CS_M2;
        }
        if chips & CS_S2 != 0 && self.peris.s2_busy.is_low()? {
            busy |= CS_S2;
        }
        Ok(busy)
    }

    /// Poll readiness status of all sub-displays and return a bit mask of the busy ones.
    pub fn get_busy(&mut self) -> u8 {
        self.busy_chips(CS_ALL).unwrap()
    }

    /// Check if any of the sub-displays is busy.
    pub fn is_busy(&mut self) -> bool {
        self.busy_chips(CS_ALL).unwrap() != 0
    }

    /// Query and return the status byte of each sub-display.
    /// Order: \[M1, S1, M2, S2\].
    pub async fn get_status(&mut self) -> Result<[u8; 4], SPI::Error> {
        self.control_state = 0xFF;
        let mut status = [0u8; 4];
        for i in 0..4 {
            let (cs, dc) = match i {
                0 => (&mut self.peris.m1_cs, &mut self.peris.m1s1_dc),
                1 => (&mut self.peris.s1_cs, &mut self.peris.m1s1_dc),
                2 => (&mut self.peris.m2_cs, &mut self.peris.m2s2_dc),
                _ => (&mut self.peris.s2_cs, &mut self.peris.m2s2_dc),
            };
            // Request status
            drop(cs.set_low());
            drop(dc.set_low());
            self.delay.delay_ns(100).await; // Tcss = 60ns
            self.peris.spi.write(&[Command::GetStatus as u8]).await?;
            self.peris.spi.flush().await?;
            self.delay.delay_ns(100).await; // Tsds = 30ns

            // Read status
            drop(dc.set_high());
            self.delay.delay_ns(100).await; // Tsdh = 30ns
            self.peris.spi.read(&mut status[i..i + 1]).await?;
            self.delay.delay_ns(100).await; // Tscc = 20ns
            drop(dc.set_low());

            drop(cs.set_high());
            self.delay.delay_ns(100).await; // Tchw = 40ns
        }
        self.control_state = 0;
        Ok(status)
    }
}
