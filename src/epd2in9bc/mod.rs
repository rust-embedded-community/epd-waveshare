//! A simple Driver for the Waveshare 2.9" (B/C) E-Ink Display via SPI
//!
//! # Example for the 2.9" E-Ink Display
//!
//!```rust, no_run
//!# use embedded_hal_mock::eh1::*;
//!# fn main() -> Result<(), embedded_hal::spi::ErrorKind> {
//!use embedded_graphics::{
//!    pixelcolor::BinaryColor::On as Black, prelude::*, primitives::{Line, PrimitiveStyle},
//!};
//!use epd_waveshare::{epd2in9bc::*, prelude::*};
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
//!let mut epd = Epd2in9bc::new(&mut spi, busy_in, dc, rst, &mut delay, None)?;
//!
//!// Use display graphics from embedded-graphics
//!// This display is for the black/white pixels
//!let mut mono_display = Display2in9bc::default();
//!
//!// Use embedded graphics for drawing
//!// A black line
//!let _ = Line::new(Point::new(0, 120), Point::new(0, 200))
//!    .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
//!    .draw(&mut mono_display);
//!
//!// Use a second display for red/yellow
//!let mut chromatic_display = Display2in9bc::default();
//!
//!// We use `Black` but it will be shown as red/yellow
//!let _ = Line::new(Point::new(15, 120), Point::new(15, 200))
//!    .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
//!    .draw(&mut chromatic_display);
//!
//!// Display updated frame
//!epd.update_color_frame(
//!    &mut spi,
//!    &mut delay,
//!    &mono_display.buffer(),
//!    &chromatic_display.buffer()
//!)?;
//!epd.display_frame(&mut spi, &mut delay)?;
//!
//!// Set the EPD to sleep
//!epd.sleep(&mut spi, &mut delay)?;
//!# Ok(())
//!# }
//!```
use embedded_hal::digital::*;

use crate::interface::{DelayNs, DisplayInterface, SpiDevice};
use crate::traits::{
    InternalWiAdditions, RefreshLut, WaveshareDisplay, WaveshareThreeColorDisplay,
};

/// Width of epd2in9bc in pixels
pub const WIDTH: u32 = 128;
/// Height of epd2in9bc in pixels
pub const HEIGHT: u32 = 296;
/// Default background color (white) of epd2in9bc display
pub const DEFAULT_BACKGROUND_COLOR: Color = Color::White;

const NUM_DISPLAY_BITS: u32 = WIDTH / 8 * HEIGHT;

const IS_BUSY_LOW: bool = true;
const VCOM_DATA_INTERVAL: u8 = 0x07;
const WHITE_BORDER: u8 = 0x70;
const BLACK_BORDER: u8 = 0x30;
const CHROMATIC_BORDER: u8 = 0xb0;
const FLOATING_BORDER: u8 = 0xF0;
const SINGLE_BYTE_WRITE: bool = true;

use crate::color::{Color, TriColor};

pub(crate) mod command;
use self::command::Command;
use crate::buffer_len;

/// Full size buffer for use with the 2in9b/c EPD
/// TODO this should be a TriColor, but let's keep it as is at first
#[cfg(feature = "graphics")]
pub type Display2in9bc = crate::graphics::Display<
    WIDTH,
    HEIGHT,
    false,
    { buffer_len(WIDTH as usize, HEIGHT as usize) },
    Color,
>;

/// Epd2in9bc driver
pub struct Epd2in9bc<SPI, BUSY, DC, RST, DELAY> {
    interface: DisplayInterface<SPI, BUSY, DC, RST, DELAY, SINGLE_BYTE_WRITE>,
    color: Color,
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
    for Epd2in9bc<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Values taken from datasheet and sample code

        self.interface.reset(delay, 10_000, 10_000).await;

        // start the booster
        self.interface
            .cmd_with_data(spi, Command::BoosterSoftStart, &[0x17, 0x17, 0x17])
            .await?;

        // power on
        self.command(spi, Command::PowerOn).await?;
        delay.delay_us(5000).await;
        self.wait_until_idle(spi, delay).await?;

        // set the panel settings
        self.cmd_with_data(spi, Command::PanelSetting, &[0x8F])
            .await?;

        self.cmd_with_data(
            spi,
            Command::VcomAndDataIntervalSetting,
            &[WHITE_BORDER | VCOM_DATA_INTERVAL],
        )
        .await?;

        // set resolution
        self.send_resolution(spi).await?;

        self.cmd_with_data(spi, Command::VcmDcSetting, &[0x0A])
            .await?;

        self.wait_until_idle(spi, delay).await?;

        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareThreeColorDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in9bc<SPI, BUSY, DC, RST, DELAY>
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

    /// Update only the black/white data of the display.
    ///
    /// Finish by calling `update_chromatic_frame`.
    async fn update_achromatic_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
        black: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface
            .cmd(spi, Command::DataStartTransmission1)
            .await?;
        self.interface.data(spi, black).await?;
        Ok(())
    }

    /// Update only chromatic data of the display.
    ///
    /// This data takes precedence over the black/white data.
    async fn update_chromatic_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        chromatic: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface
            .cmd(spi, Command::DataStartTransmission2)
            .await?;
        self.interface.data(spi, chromatic).await?;

        self.wait_until_idle(spi, delay).await?;
        Ok(())
    }
}

#[maybe_async::maybe_async(AFIT)]
impl<SPI, BUSY, DC, RST, DELAY> WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
    for Epd2in9bc<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    type DisplayColor = Color;
    async fn new(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
    ) -> Result<Self, SPI::Error> {
        let interface = DisplayInterface::new(busy, dc, rst, delay_us);
        let color = DEFAULT_BACKGROUND_COLOR;

        let mut epd = Epd2in9bc { interface, color };

        epd.init(spi, delay).await?;

        Ok(epd)
    }

    async fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        // Section 8.2 from datasheet
        self.interface
            .cmd_with_data(
                spi,
                Command::VcomAndDataIntervalSetting,
                &[FLOATING_BORDER | VCOM_DATA_INTERVAL],
            )
            .await?;

        self.command(spi, Command::PowerOff).await?;
        // The example STM code from Github has a wait after PowerOff
        self.wait_until_idle(spi, delay).await?;

        self.cmd_with_data(spi, Command::DeepSleep, &[0xA5]).await?;

        Ok(())
    }

    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.init(spi, delay).await
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

    async fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error> {
        self.interface
            .cmd(spi, Command::DataStartTransmission1)
            .await?;

        self.interface.data(spi, buffer).await?;

        // Clear the chromatic layer
        let color = self.color.get_byte_value();

        self.interface
            .cmd(spi, Command::DataStartTransmission2)
            .await?;
        self.interface
            .data_x_times(spi, color, NUM_DISPLAY_BITS)
            .await?;

        self.wait_until_idle(spi, delay).await?;
        Ok(())
    }

    #[allow(unused)]
    async fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error> {
        Ok(())
    }

    async fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.command(spi, Command::DisplayRefresh).await?;

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

    async fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error> {
        self.send_resolution(spi).await?;

        let color = DEFAULT_BACKGROUND_COLOR.get_byte_value();

        // Clear the black
        self.interface
            .cmd(spi, Command::DataStartTransmission1)
            .await?;

        self.interface
            .data_x_times(spi, color, NUM_DISPLAY_BITS)
            .await?;

        // Clear the chromatic
        self.interface
            .cmd(spi, Command::DataStartTransmission2)
            .await?;
        self.interface
            .data_x_times(spi, color, NUM_DISPLAY_BITS)
            .await?;

        self.wait_until_idle(spi, delay).await?;
        Ok(())
    }

    async fn set_lut(
        &mut self,
        _spi: &mut SPI,
        _delay: &mut DELAY,
        _refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error> {
        Ok(())
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
impl<SPI, BUSY, DC, RST, DELAY> Epd2in9bc<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    async fn command(&mut self, spi: &mut SPI, command: Command) -> Result<(), SPI::Error> {
        self.interface.cmd(spi, command).await
    }

    async fn send_data(&mut self, spi: &mut SPI, data: &[u8]) -> Result<(), SPI::Error> {
        self.interface.data(spi, data).await
    }

    async fn cmd_with_data(
        &mut self,
        spi: &mut SPI,
        command: Command,
        data: &[u8],
    ) -> Result<(), SPI::Error> {
        self.interface.cmd_with_data(spi, command, data).await
    }

    async fn send_resolution(&mut self, spi: &mut SPI) -> Result<(), SPI::Error> {
        let w = self.width();
        let h = self.height();

        self.command(spi, Command::ResolutionSetting).await?;

        self.send_data(spi, &[w as u8]).await?;
        self.send_data(spi, &[(h >> 8) as u8]).await?;
        self.send_data(spi, &[h as u8]).await
    }

    /// Set the outer border of the display to the chosen color.
    pub async fn set_border_color(
        &mut self,
        spi: &mut SPI,
        color: TriColor,
    ) -> Result<(), SPI::Error> {
        let border = match color {
            TriColor::Black => BLACK_BORDER,
            TriColor::White => WHITE_BORDER,
            TriColor::Chromatic => CHROMATIC_BORDER,
        };
        self.cmd_with_data(
            spi,
            Command::VcomAndDataIntervalSetting,
            &[border | VCOM_DATA_INTERVAL],
        )
        .await
    }
}
