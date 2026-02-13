//! Graphics Support for EPDs

use crate::color::{ColorType, TriColor};
use core::marker::PhantomData;
use embedded_graphics_core::prelude::*;

/// Display rotation, only 90° increments supported
#[derive(Default, Clone, Copy)]
pub enum DisplayRotation {
    /// No rotation
    #[default]
    Rotate0,
    /// Rotate by 90 degrees clockwise
    Rotate90,
    /// Rotate by 180 degrees clockwise
    Rotate180,
    /// Rotate 270 degrees clockwise
    Rotate270,
}

/// DisplayMode carries modes avaiable for color representaion on TriColor displays
/// Each mode contains list of (bw, color) pairs, where bw - value for BW layer, color - value for Chromatic layer
#[repr(u8)]
pub enum DisplayMode {
    /// BwrBitOn chromatic doesn't override white, white bit cleared for black, white bit set for white, both bits set for chromatic
    /// Colors: White - (0xFF, 0); Black - (0, 0); Red - (0, 0xFF)
    BwrBitOn = 0,
    /// BwrBitOff is chromatic does override white, both bits cleared for black, white bit set for white, red bit set for black
    /// Colors: White - (0xFF, 0); Black - (0, 0); Red - (0xFF, 0xFF)
    BwrBitOff = 1,
    /// BwrBitOnColorInverted is same as standard, but with color layer values inverted
    /// Colors: White - (0xFF, 0xFF); Black - (0, 0xFF); Red - (0, 0)
    BwrBitOnColorInverted = 2,
}

impl DisplayMode {
    fn from_u8(value: u8) -> DisplayMode {
        match value {
            0 => DisplayMode::BwrBitOn,
            1 => DisplayMode::BwrBitOff,
            2 => DisplayMode::BwrBitOnColorInverted,
            v => panic!("Unsupported DisplayMode value {v}"),
        }
    }
}

/// count the number of bytes per line knowing that it may contains padding bits
const fn line_bytes(width: u32, bits_per_pixel: usize) -> usize {
    // round to upper 8 bit count
    (width as usize * bits_per_pixel + 7) / 8
}

/// Display buffer used for drawing with embedded graphics
/// This can be rendered on EPD using ...
///
/// - WIDTH: width in pixel when display is not rotated
/// - HEIGHT: height in pixel when display is not rotated
/// - MODE: mandatory value of the B/W when chromatic bit is set, can be any value for non
///           tricolor epd
/// - COLOR: color type used by the target display
/// - BYTECOUNT: This is redundant with previous data and should be removed when const generic
///              expressions are stabilized
///
/// More on MODE:
///
/// Different chromatic displays differently treat the bits in chromatic color planes.
/// Some of them ([crate::epd2in13bc]) will render a color pixel if bit is set for that pixel,
/// which is a `MODE = BwrBitOn` mode.
///
/// Other displays, like [crate::epd5in83b_v2] in opposite, will draw color pixel if bit is
/// cleared for that pixel, which is a `MODE = BwrBitOff` mode.
///
/// A few displays, like [crate::epd4in2bc] will draw color pixel if bit is set for that pixel,
/// but will invert the color values, which is a `MODE = BwrBitOnColorInverted` mode.
///
/// MODE=BwrBitOn: chromatic doesn't override white, white bit cleared for black, white bit set for white, both bits set for chromatic
/// MODE=BwrBitOff: chromatic does override white, both bits cleared for black, white bit set for white, red bit set for black
/// MODE=BwrBitOnColorInverted: same as standard, but with color layer values inverted
pub struct Display<
    const WIDTH: u32,
    const HEIGHT: u32,
    const MODE: u8,
    const BYTECOUNT: usize,
    COLOR: ColorType + PixelColor,
> {
    buffer: [u8; BYTECOUNT],
    rotation: DisplayRotation,
    _color: PhantomData<COLOR>,
}

impl<
        const WIDTH: u32,
        const HEIGHT: u32,
        const MODE: u8,
        const BYTECOUNT: usize,
        COLOR: ColorType + PixelColor,
    > Default for Display<WIDTH, HEIGHT, MODE, BYTECOUNT, COLOR>
{
    /// Initialize display with the color '0', which may not be the same on all device.
    /// Many devices have a bit parameter polarity that should be changed if this is not the right
    /// one.
    /// However, every device driver should implement a DEFAULT_COLOR constant to indicate which
    /// color this represents (TODO)
    ///
    /// If you want a specific default color, you can still call clear() to set one.
    // inline is necessary here to allow heap allocation via Box on stack limited programs
    #[inline(always)]
    fn default() -> Self {
        Self {
            // default color must be 0 for every bit in a pixel to make this work everywere
            buffer: [0u8; BYTECOUNT],
            rotation: DisplayRotation::default(),
            _color: PhantomData,
        }
    }
}

/// For use with embedded_grahics
impl<
        const WIDTH: u32,
        const HEIGHT: u32,
        const MODE: u8,
        const BYTECOUNT: usize,
        COLOR: ColorType + PixelColor,
    > DrawTarget for Display<WIDTH, HEIGHT, MODE, BYTECOUNT, COLOR>
{
    type Color = COLOR;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for pixel in pixels {
            self.set_pixel(pixel);
        }
        Ok(())
    }
}

/// For use with embedded_grahics
impl<
        const WIDTH: u32,
        const HEIGHT: u32,
        const MODE: u8,
        const BYTECOUNT: usize,
        COLOR: ColorType + PixelColor,
    > OriginDimensions for Display<WIDTH, HEIGHT, MODE, BYTECOUNT, COLOR>
{
    fn size(&self) -> Size {
        match self.rotation {
            DisplayRotation::Rotate0 | DisplayRotation::Rotate180 => Size::new(WIDTH, HEIGHT),
            DisplayRotation::Rotate90 | DisplayRotation::Rotate270 => Size::new(HEIGHT, WIDTH),
        }
    }
}

impl<
        const WIDTH: u32,
        const HEIGHT: u32,
        const MODE: u8,
        const BYTECOUNT: usize,
        COLOR: ColorType + PixelColor,
    > Display<WIDTH, HEIGHT, MODE, BYTECOUNT, COLOR>
{
    /// get internal buffer to use it (to draw in epd)
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Set the display rotation.
    ///
    /// This only concerns future drawing made to it. Anything aready drawn
    /// stays as it is in the buffer.
    pub fn set_rotation(&mut self, rotation: DisplayRotation) {
        self.rotation = rotation;
    }

    /// Get current rotation
    pub fn rotation(&self) -> DisplayRotation {
        self.rotation
    }

    /// Set a specific pixel color on this display
    pub fn set_pixel(&mut self, pixel: Pixel<COLOR>) {
        set_pixel(&mut self.buffer, WIDTH, HEIGHT, self.rotation, MODE, pixel);
    }
}

/// Some Tricolor specifics
impl<const WIDTH: u32, const HEIGHT: u32, const MODE: u8, const BYTECOUNT: usize>
    Display<WIDTH, HEIGHT, MODE, BYTECOUNT, TriColor>
{
    /// get black/white internal buffer to use it (to draw in epd)
    pub fn bw_buffer(&self) -> &[u8] {
        &self.buffer[..self.buffer.len() / 2]
    }

    /// get chromatic internal buffer to use it (to draw in epd)
    pub fn chromatic_buffer(&self) -> &[u8] {
        &self.buffer[self.buffer.len() / 2..]
    }
}

/// Same as `Display`, except that its characteristics are defined at runtime.
/// See display for documentation as everything is the same except that default
/// is replaced by a `new` method.
pub struct VarDisplay<'a, COLOR: ColorType + PixelColor> {
    width: u32,
    height: u32,
    mode: u8,
    buffer: &'a mut [u8],
    rotation: DisplayRotation,
    _color: PhantomData<COLOR>,
}

/// For use with embedded_grahics
impl<COLOR: ColorType + PixelColor> DrawTarget for VarDisplay<'_, COLOR> {
    type Color = COLOR;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for pixel in pixels {
            self.set_pixel(pixel);
        }
        Ok(())
    }
}

/// For use with embedded_grahics
impl<COLOR: ColorType + PixelColor> OriginDimensions for VarDisplay<'_, COLOR> {
    fn size(&self) -> Size {
        match self.rotation {
            DisplayRotation::Rotate0 | DisplayRotation::Rotate180 => {
                Size::new(self.width, self.height)
            }
            DisplayRotation::Rotate90 | DisplayRotation::Rotate270 => {
                Size::new(self.height, self.width)
            }
        }
    }
}

/// Error found during usage of VarDisplay
#[derive(Debug)]
pub enum VarDisplayError {
    /// The provided buffer was too small
    BufferTooSmall,
}

impl<'a, COLOR: ColorType + PixelColor> VarDisplay<'a, COLOR> {
    /// You must allocate the buffer by yourself, it must be large enough to contain all pixels.
    ///
    /// Parameters are documented in `Display` as they are the same as the const generics there.
    /// MODE should be false for non tricolor displays
    pub fn new(
        width: u32,
        height: u32,
        buffer: &'a mut [u8],
        mode: u8,
    ) -> Result<Self, VarDisplayError> {
        let myself = Self {
            width,
            height,
            mode,
            buffer,
            rotation: DisplayRotation::default(),
            _color: PhantomData,
        };
        // enfore some constraints dynamicly
        if myself.buffer_size() > myself.buffer.len() {
            return Err(VarDisplayError::BufferTooSmall);
        }
        Ok(myself)
    }

    /// get the number of used bytes in the buffer
    fn buffer_size(&self) -> usize {
        self.height as usize
            * line_bytes(
                self.width,
                COLOR::BITS_PER_PIXEL_PER_BUFFER * COLOR::BUFFER_COUNT,
            )
    }

    /// get internal buffer to use it (to draw in epd)
    pub fn buffer(&self) -> &[u8] {
        &self.buffer[..self.buffer_size()]
    }

    /// Set the display rotation.
    ///
    /// This only concerns future drawing made to it. Anything aready drawn
    /// stays as it is in the buffer.
    pub fn set_rotation(&mut self, rotation: DisplayRotation) {
        self.rotation = rotation;
    }

    /// Get current rotation
    pub fn rotation(&self) -> DisplayRotation {
        self.rotation
    }

    /// Set a specific pixel color on this display
    pub fn set_pixel(&mut self, pixel: Pixel<COLOR>) {
        let size = self.buffer_size();
        set_pixel(
            &mut self.buffer[..size],
            self.width,
            self.height,
            self.rotation,
            self.mode,
            pixel,
        );
    }
}

/// Some Tricolor specifics
impl VarDisplay<'_, TriColor> {
    /// get black/white internal buffer to use it (to draw in epd)
    pub fn bw_buffer(&self) -> &[u8] {
        &self.buffer[..self.buffer_size() / 2]
    }

    /// get chromatic internal buffer to use it (to draw in epd)
    pub fn chromatic_buffer(&self) -> &[u8] {
        &self.buffer[self.buffer_size() / 2..self.buffer_size()]
    }
}

// This is a function to share code between `Display` and `VarDisplay`
// It sets a specific pixel in a buffer to a given color.
// The big number of parameters is due to the fact that it is an internal function to both
// strctures.
fn set_pixel<COLOR: ColorType + PixelColor>(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    rotation: DisplayRotation,
    mode: u8,
    pixel: Pixel<COLOR>,
) {
    let Pixel(point, color) = pixel;

    // final coordinates
    let (x, y) = match rotation {
        // as i32 = never use more than 2 billion pixel per line or per column
        DisplayRotation::Rotate0 => (point.x, point.y),
        DisplayRotation::Rotate90 => (width as i32 - 1 - point.y, point.x),
        DisplayRotation::Rotate180 => (width as i32 - 1 - point.x, height as i32 - 1 - point.y),
        DisplayRotation::Rotate270 => (point.y, height as i32 - 1 - point.x),
    };

    // Out of range check
    if (x < 0) || (x >= width as i32) || (y < 0) || (y >= height as i32) {
        // don't do anything in case of out of range
        return;
    }

    let display_mode = DisplayMode::from_u8(mode);
    let index = x as usize * COLOR::BITS_PER_PIXEL_PER_BUFFER / 8
        + y as usize * line_bytes(width, COLOR::BITS_PER_PIXEL_PER_BUFFER);
    let (mask, bits) = color.bitmask(display_mode, x as u32);

    if COLOR::BUFFER_COUNT == 2 {
        // split buffer is for tricolor displays that use 2 buffer for 2 bits per pixel
        buffer[index] = buffer[index] & mask | (bits & 0xFF) as u8;
        let index = index + buffer.len() / 2;
        buffer[index] = buffer[index] & mask | (bits >> 8) as u8;
    } else {
        buffer[index] = buffer[index] & mask | bits as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::*;
    use embedded_graphics::{
        prelude::*,
        primitives::{Line, PrimitiveStyle},
    };

    // test buffer length
    #[test]
    fn graphics_size() {
        // example definition taken from epd1in54
        let display = Display::<200, 200,     { DisplayMode::BwrBitOff as u8 }
        , { 200 * 200 / 8 }, Color>::default();
        assert_eq!(display.buffer().len(), 5000);
    }

    // test default background color on all bytes
    #[test]
    fn graphics_default() {
        let display = Display::<200, 200, { DisplayMode::BwrBitOff as u8 }, { 200 * 200 / 8 }, Color>::default();
        for &byte in display.buffer() {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn graphics_rotation_0() {
        let mut display = Display::<
            200,
            200,
            { DisplayMode::BwrBitOff as u8 },
            { 200 * 200 / 8 },
            Color,
        >::default();
        let _ = Line::new(Point::new(0, 0), Point::new(7, 0))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(&mut display);

        let buffer = display.buffer();

        assert_eq!(buffer[0], Color::Black.get_byte_value());

        for &byte in buffer.iter().skip(1) {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn graphics_rotation_90() {
        let mut display = Display::<
            200,
            200,
            { DisplayMode::BwrBitOff as u8 },
            { 200 * 200 / 8 },
            Color,
        >::default();
        display.set_rotation(DisplayRotation::Rotate90);
        let _ = Line::new(Point::new(0, 192), Point::new(0, 199))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(&mut display);

        let buffer = display.buffer();

        assert_eq!(buffer[0], Color::Black.get_byte_value());

        for &byte in buffer.iter().skip(1) {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn graphics_rotation_180() {
        let mut display = Display::<
            200,
            200,
            { DisplayMode::BwrBitOff as u8 },
            { 200 * 200 / 8 },
            Color,
        >::default();
        display.set_rotation(DisplayRotation::Rotate180);
        let _ = Line::new(Point::new(192, 199), Point::new(199, 199))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(&mut display);

        let buffer = display.buffer();

        extern crate std;
        std::println!("{buffer:?}");

        assert_eq!(buffer[0], Color::Black.get_byte_value());

        for &byte in buffer.iter().skip(1) {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn graphics_rotation_270() {
        let mut display = Display::<
            200,
            200,
            { DisplayMode::BwrBitOff as u8 },
            { 200 * 200 / 8 },
            Color,
        >::default();
        display.set_rotation(DisplayRotation::Rotate270);
        let _ = Line::new(Point::new(199, 0), Point::new(199, 7))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(&mut display);

        let buffer = display.buffer();

        extern crate std;
        std::println!("{buffer:?}");

        assert_eq!(buffer[0], Color::Black.get_byte_value());

        for &byte in buffer.iter().skip(1) {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn graphics_set_pixel_tricolor_false() {
        let mut display = Display::<
            4,
            4,
            { DisplayMode::BwrBitOff as u8 },
            { 4 * 4 * 2 / 8 },
            TriColor,
        >::default();
        display.set_pixel(Pixel(Point::new(0, 0), TriColor::White));
        display.set_pixel(Pixel(Point::new(1, 0), TriColor::Chromatic));
        display.set_pixel(Pixel(Point::new(2, 0), TriColor::Black));

        let bw_buffer = display.bw_buffer();
        let chromatic_buffer = display.chromatic_buffer();

        extern crate std;
        std::println!("{:?}", bw_buffer);
        std::println!("{:?}", chromatic_buffer);

        assert_eq!(bw_buffer, [192, 0]);
        assert_eq!(chromatic_buffer, [64, 0]);
    }

    #[test]
    fn graphics_set_pixel_tricolor_true() {
        let mut display =
            Display::<4, 4, { DisplayMode::BwrBitOn as u8 }, { 4 * 4 * 2 / 8 }, TriColor>::default(
            );
        display.set_pixel(Pixel(Point::new(0, 0), TriColor::White));
        display.set_pixel(Pixel(Point::new(1, 0), TriColor::Chromatic));
        display.set_pixel(Pixel(Point::new(2, 0), TriColor::Black));

        let bw_buffer = display.bw_buffer();
        let chromatic_buffer = display.chromatic_buffer();

        extern crate std;
        std::println!("{:?}", bw_buffer);
        std::println!("{:?}", chromatic_buffer);

        assert_eq!(bw_buffer, [128, 0]);
        assert_eq!(chromatic_buffer, [64, 0]);
    }
}
