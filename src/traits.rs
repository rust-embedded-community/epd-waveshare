use crate::interface::{DelayNs, SpiDevice};
use core::marker::Sized;
use embedded_hal::digital::*;

/// All commands need to have this trait which gives the address of the command
/// which needs to be send via SPI with activated CommandsPin (Data/Command Pin in CommandMode)
pub(crate) trait Command: Copy {
    fn address(self) -> u8;
}

/// Seperates the different LUT for the Display Refresh process
#[derive(Debug, Clone, PartialEq, Eq, Copy, Default)]
pub enum RefreshLut {
    /// The "normal" full Lookuptable for the Refresh-Sequence
    #[default]
    Full,
    /// The quick LUT where not the full refresh sequence is followed.
    /// This might lead to some
    Quick,
}

#[maybe_async::maybe_async(AFIT)]
pub(crate) trait InternalWiAdditions<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    /// This initialises the EPD and powers it up
    ///
    /// This function is already called from
    ///  - [new()](WaveshareDisplay::new())
    ///  - [`wake_up`]
    ///
    ///
    /// This function calls [reset](WaveshareDisplay::reset),
    /// so you don't need to call reset your self when trying to wake your device up
    /// after setting it to sleep.
    async fn init(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error>;
}

/// Functions to interact with three color panels
#[maybe_async::maybe_async(AFIT)]
#[allow(async_fn_in_trait)]
pub trait WaveshareThreeColorDisplay<SPI, BUSY, DC, RST, DELAY>:
    WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    /// Transmit data to the SRAM of the EPD
    ///
    /// Updates both the black and the secondary color layers
    async fn update_color_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        black: &[u8],
        chromatic: &[u8],
    ) -> Result<(), SPI::Error>;

    /// Update only the black/white data of the display.
    ///
    /// This must be finished by calling `update_chromatic_frame`.
    async fn update_achromatic_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        black: &[u8],
    ) -> Result<(), SPI::Error>;

    /// Update only the chromatic data of the display.
    ///
    /// This should be preceded by a call to `update_achromatic_frame`.
    /// This data takes precedence over the black/white data.
    async fn update_chromatic_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        chromatic: &[u8],
    ) -> Result<(), SPI::Error>;
}

/// All the functions to interact with the EPDs
///
/// This trait includes all public functions to use the EPDs
///
/// # Example
///
///```rust, no_run
///# use embedded_hal_mock::eh1::*;
///# fn main() -> Result<(), embedded_hal::spi::ErrorKind> {
///use embedded_graphics::{
///    pixelcolor::BinaryColor::On as Black, prelude::*, primitives::{Line, PrimitiveStyle},
///};
///use epd_waveshare::{epd4in2::*, prelude::*};
///#
///# let expectations = [];
///# let mut spi = spi::Mock::new(&expectations);
///# let expectations = [];
///# let cs_pin = digital::Mock::new(&expectations);
///# let busy_in = digital::Mock::new(&expectations);
///# let dc = digital::Mock::new(&expectations);
///# let rst = digital::Mock::new(&expectations);
///# let mut delay = delay::NoopDelay::new();
///
///// Setup EPD
///let mut epd = Epd4in2::new(&mut spi, busy_in, dc, rst, &mut delay, None)?;
///
///// Use display graphics from embedded-graphics
///let mut display = Display4in2::default();
///
///// Use embedded graphics for drawing a line
///
///let _ = Line::new(Point::new(0, 120), Point::new(0, 295))
///    .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
///    .draw(&mut display);
///
///    // Display updated frame
///epd.update_frame(&mut spi, &display.buffer(), &mut delay)?;
///epd.display_frame(&mut spi, &mut delay)?;
///
///// Set the EPD to sleep
///epd.sleep(&mut spi, &mut delay)?;
///# Ok(())
///# }
///```
///
/// # Heap allocation
///
/// For systems where stack space is limited but heap space is available
/// (i.e. ESP32 platforms with PSRAM) it's possible to allocate the display buffer
/// in heap; the Displayxxxx:default() is implemented as always inline, so you can
/// use Box::new to request heap space for the display buffer.
///
///```rust, no_run
///# use epd_waveshare::epd4in2::Display4in2;
///# use epd_waveshare::prelude::*;
///# use embedded_graphics_core::prelude::*;
///# use embedded_graphics::primitives::*;
///let mut display = Box::new(Display4in2::default());
///let _ = Line::new(Point::new(0, 120), Point::new(0, 295))
///     .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
///     .draw(&mut *display);
///```
#[maybe_async::maybe_async(AFIT)]
#[allow(async_fn_in_trait)]
pub trait WaveshareDisplay<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    /// The Color Type used by the Display
    type DisplayColor;
    /// Creates a new driver from a SPI peripheral, CS Pin, Busy InputPin, DC
    ///
    /// `delay_us` is the number of us the idle loop should sleep on.
    /// Setting it to 0 implies busy waiting.
    /// Setting it to None means a default value is used.
    ///
    /// This already initialises the device.
    async fn new(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        delay: &mut DELAY,
        delay_us: Option<u32>,
    ) -> Result<Self, SPI::Error>
    where
        Self: Sized;

    /// Let the device enter deep-sleep mode to save power.
    ///
    /// The deep sleep mode returns to standby with a hardware reset.
    async fn sleep(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error>;

    /// Wakes the device up from sleep
    ///
    /// Also reintialises the device if necessary.
    async fn wake_up(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error>;

    /// Sets the backgroundcolor for various commands like [clear_frame](WaveshareDisplay::clear_frame)
    fn set_background_color(&mut self, color: Self::DisplayColor);

    /// Get current background color
    fn background_color(&self) -> &Self::DisplayColor;

    /// Get the width of the display
    fn width(&self) -> u32;

    /// Get the height of the display
    fn height(&self) -> u32;

    /// Transmit a full frame to the SRAM of the EPD
    async fn update_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error>;

    /// Transmits partial data to the SRAM of the EPD
    ///
    /// (x,y) is the top left corner
    ///
    /// BUFFER needs to be of size: width / 8 * height !
    #[allow(clippy::too_many_arguments)]
    async fn update_partial_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error>;

    /// Displays the frame data from SRAM
    ///
    /// This function waits until the device isn`t busy anymore
    async fn display_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error>;

    /// Provide a combined update&display and save some time (skipping a busy check in between)
    async fn update_and_display_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error>;

    /// Clears the frame buffer on the EPD with the declared background color
    ///
    /// The background color can be changed with [`WaveshareDisplay::set_background_color`]
    async fn clear_frame(&mut self, spi: &mut SPI, delay: &mut DELAY) -> Result<(), SPI::Error>;

    /// Trait for using various Waveforms from different LUTs
    /// E.g. for partial refreshes
    ///
    /// A full refresh is needed after a certain amount of quick refreshes!
    ///
    /// WARNING: Quick Refresh might lead to ghosting-effects/problems with your display. Especially for the 4.2in Display!
    ///
    /// If None is used the old value will be loaded on the LUTs once more
    async fn set_lut(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        refresh_rate: Option<RefreshLut>,
    ) -> Result<(), SPI::Error>;

    /// Wait until the display has stopped processing data
    ///
    /// You can call this to make sure a frame is displayed before goin further
    async fn wait_until_idle(&mut self, spi: &mut SPI, delay: &mut DELAY)
        -> Result<(), SPI::Error>;
}

/// Allows quick refresh support for displays that support it; lets you send both
/// old and new frame data to support this.
///
/// When using the quick refresh look-up table, the display must receive separate display
/// buffer data marked as old, and new. This is used to determine which pixels need to change,
/// and how they will change. This isn't required when using full refreshes.
///
/// (todo: Example ommitted due to CI failures.)
/// Example:
///```rust, no_run
///# use embedded_hal_mock::eh1::*;
///# fn main() -> Result<(), embedded_hal::spi::ErrorKind> {
///# use embedded_graphics::{
///#   pixelcolor::BinaryColor::On as Black, prelude::*, primitives::{Line, PrimitiveStyle},
///# };
///# use epd_waveshare::{epd4in2::*, prelude::*};
///# use epd_waveshare::graphics::VarDisplay;
///#
///# let expectations = [];
///# let mut spi = spi::Mock::new(&expectations);
///# let expectations = [];
///# let cs_pin = digital::Mock::new(&expectations);
///# let busy_in = digital::Mock::new(&expectations);
///# let dc = digital::Mock::new(&expectations);
///# let rst = digital::Mock::new(&expectations);
///# let mut delay = delay::NoopDelay::new();
///#
///# // Setup EPD
///# let mut epd = Epd4in2::new(&mut spi, busy_in, dc, rst, &mut delay, None)?;
///let (x, y, frame_width, frame_height) = (20, 40, 80,80);
///
///let mut buffer = [DEFAULT_BACKGROUND_COLOR.get_byte_value(); 80 / 8 * 80];
///let mut display = VarDisplay::new(frame_width, frame_height, &mut buffer,false).unwrap();
///
///epd.update_partial_old_frame(&mut spi, &mut delay, display.buffer(), x, y, frame_width, frame_height)
///  .ok();
///
///display.clear(Color::White).ok();
///// Execute drawing commands here.
///
///epd.update_partial_new_frame(&mut spi, &mut delay, display.buffer(), x, y, frame_width, frame_height)
///  .ok();
///# Ok(())
///# }
///```
#[maybe_async::maybe_async(AFIT)]
#[allow(async_fn_in_trait)]
pub trait QuickRefresh<SPI, BUSY, DC, RST, DELAY>
where
    SPI: SpiDevice,
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    DELAY: DelayNs,
{
    /// Updates the old frame.
    async fn update_old_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error>;

    /// Updates the new frame.
    async fn update_new_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error>;

    /// Displays the new frame
    async fn display_new_frame(
        &mut self,
        spi: &mut SPI,
        _delay: &mut DELAY,
    ) -> Result<(), SPI::Error>;

    /// Updates and displays the new frame.
    async fn update_and_display_new_frame(
        &mut self,
        spi: &mut SPI,
        buffer: &[u8],
        delay: &mut DELAY,
    ) -> Result<(), SPI::Error>;

    /// Updates the old frame for a portion of the display.
    #[allow(clippy::too_many_arguments)]
    async fn update_partial_old_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error>;

    /// Updates the new frame for a portion of the display.
    #[allow(clippy::too_many_arguments)]
    async fn update_partial_new_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        buffer: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error>;

    /// Clears the partial frame buffer on the EPD with the declared background color
    /// The background color can be changed with [`WaveshareDisplay::set_background_color`]
    async fn clear_partial_frame(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), SPI::Error>;
}
