//! Graphics Support for EPDs

use crate::color::{Color, ColorType, TriColor};
use core::marker::PhantomData;
use embedded_graphics_core::prelude::*;

/// Display rotation, only 90° increments supported
#[derive(Clone, Copy, Default)]
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

/// count the number of bytes per line knowing that it may contains padding bits
const fn line_bytes(width: u32, bits_per_pixel: usize) -> usize {
    // round to upper 8 bit count
    (width as usize * bits_per_pixel + 7) / 8
}

/// count the number of bytes needed for a partial window buffer
const fn partial_frame_buffer_size(
    x: u32,
    width: u32,
    height: u32,
    bits_per_pixel: usize,
) -> usize {
    let aligned_x = x & !0b111;
    let x_end = x + width - 1;
    let aligned_x_end = x_end | 0b111;
    let aligned_width = aligned_x_end - aligned_x + 1;
    let buffer_size = height as usize * line_bytes(aligned_width, bits_per_pixel);
    buffer_size
}

/// Display buffer used for drawing with embedded graphics
/// This can be rendered on EPD using ...
///
/// - WIDTH: width in pixel when display is not rotated
/// - HEIGHT: height in pixel when display is not rotated
/// - BWRBIT: mandatory value of the B/W when chromatic bit is set, can be any value for non
///           tricolor epd
/// - COLOR: color type used by the target display
/// - BYTECOUNT: This is redundant with previous data and should be removed when const generic
///              expressions are stabilized
///
/// More on BWRBIT:
///
/// Different chromatic displays differently treat the bits in chromatic color planes.
/// Some of them ([crate::epd2in13bc]) will render a color pixel if bit is set for that pixel,
/// which is a `BWRBIT = true` mode.
///
/// Other displays, like [crate::epd5in83b_v2] in opposite, will draw color pixel if bit is
/// cleared for that pixel, which is a `BWRBIT = false` mode.
///
/// BWRBIT=true: chromatic doesn't override white, white bit cleared for black, white bit set for white, both bits set for chromatic
/// BWRBIT=false: chromatic does override white, both bits cleared for black, white bit set for white, red bit set for black
pub struct Display<
    const WIDTH: u32,
    const HEIGHT: u32,
    const BWRBIT: bool,
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
        const BWRBIT: bool,
        const BYTECOUNT: usize,
        COLOR: ColorType + PixelColor,
    > Default for Display<WIDTH, HEIGHT, BWRBIT, BYTECOUNT, COLOR>
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
        const BWRBIT: bool,
        const BYTECOUNT: usize,
        COLOR: ColorType + PixelColor,
    > DrawTarget for Display<WIDTH, HEIGHT, BWRBIT, BYTECOUNT, COLOR>
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
        const BWRBIT: bool,
        const BYTECOUNT: usize,
        COLOR: ColorType + PixelColor,
    > OriginDimensions for Display<WIDTH, HEIGHT, BWRBIT, BYTECOUNT, COLOR>
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
        const BWRBIT: bool,
        const BYTECOUNT: usize,
        COLOR: ColorType + PixelColor,
    > Display<WIDTH, HEIGHT, BWRBIT, BYTECOUNT, COLOR>
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
        set_pixel(
            &mut self.buffer,
            WIDTH,
            HEIGHT,
            self.rotation,
            BWRBIT,
            pixel,
        );
    }

    /// count the number of bytes needed for a partial window buffer
    pub fn partial_frame_buffer_size(x: u32, width: u32, height: u32) -> usize {
        partial_frame_buffer_size(
            x,
            width,
            height,
            COLOR::BITS_PER_PIXEL_PER_BUFFER * COLOR::BUFFER_COUNT,
        )
    }

    /// Creates a virtual partial frame
    /// Handles byte-alignment for you and keeps the full display buffer in sync
    pub fn get_partial_frame<'a, 'b>(
        &'a mut self,
        buffer: &'b mut [u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> PartialFrame<'a, 'b, COLOR> {
        PartialFrame::new(
            x,
            y,
            width,
            height,
            buffer,
            &mut self.buffer,
            WIDTH,
            HEIGHT,
            BYTECOUNT,
            BWRBIT,
        )
    }
}

/// Some Tricolor specifics
impl<const WIDTH: u32, const HEIGHT: u32, const BWRBIT: bool, const BYTECOUNT: usize>
    Display<WIDTH, HEIGHT, BWRBIT, BYTECOUNT, TriColor>
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
    bwrbit: bool,
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
    /// bwrbit should be false for non tricolor displays
    pub fn new(
        width: u32,
        height: u32,
        buffer: &'a mut [u8],
        bwrbit: bool,
    ) -> Result<Self, VarDisplayError> {
        let myself = Self {
            width,
            height,
            bwrbit,
            buffer,
            rotation: DisplayRotation::default(),
            _color: PhantomData,
        };
        // enforce some constraints dynamically
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
            self.bwrbit,
            pixel,
        );
    }

    /// count the number of bytes needed for a partial window buffer
    pub fn partial_frame_buffer_size(x: u32, width: u32, height: u32) -> usize {
        partial_frame_buffer_size(
            x,
            width,
            height,
            COLOR::BITS_PER_PIXEL_PER_BUFFER * COLOR::BUFFER_COUNT,
        )
    }

    /// Creates a virtual partial frame
    /// Handles byte-alignment for you and keeps the full display buffer in sync
    pub fn get_partial_frame<'b, 'c>(
        &'b mut self,
        buffer: &'c mut [u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> PartialFrame<'b, 'c, COLOR> {
        let buffer_size = self.buffer_size();
        PartialFrame::new(
            x,
            y,
            width,
            height,
            buffer,
            &mut self.buffer,
            self.width,
            self.height,
            buffer_size,
            self.bwrbit,
        )
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

/// Same as `Display`, except that its characteristics are defined at runtime, and it's buffer is
/// byte-aligned relative to the full display.
/// See display for documentation as everything is the same except that default
/// is replaced by a `new` method.
pub struct PartialFrame<'a, 'b, COLOR: ColorType + PixelColor> {
    original_x: u32,
    aligned_x: u32,
    y: u32,
    original_width: u32,
    aligned_width: u32,
    height: u32,
    bwrbit: bool,
    buffer: &'b mut [u8],
    full_display_buffer: &'a mut [u8],
    full_display_width: u32,
    full_display_size: usize,
    rotation: DisplayRotation,
    _color: PhantomData<COLOR>,
}

/// Byte-aligned dimensions and coordinates of a partial frame.
/// To be used as parameters for the [crate::traits::WaveshareDisplay::update_partial_frame] function.
pub struct PartialUpdateParameters<'a> {
    /// X-coordinate of the partial frame, byte-aligned
    pub x: u32,
    /// Y-coordinate of the partial frame, unchanged
    pub y: u32,
    /// Width of the partial frame, byte-aligned
    pub width: u32,
    /// Height of the partial frame, unchanged
    pub height: u32,
    /// Byte-aligned buffer
    pub buffer: &'a [u8],
}

/// For use with embedded_grahics
impl<COLOR: ColorType + PixelColor> DrawTarget for PartialFrame<'_, '_, COLOR> {
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
impl<COLOR: ColorType + PixelColor> OriginDimensions for PartialFrame<'_, '_, COLOR> {
    fn size(&self) -> Size {
        match self.rotation {
            DisplayRotation::Rotate0 | DisplayRotation::Rotate180 => {
                Size::new(self.original_width, self.height)
            }
            DisplayRotation::Rotate90 | DisplayRotation::Rotate270 => {
                Size::new(self.height, self.original_width)
            }
        }
    }
}

impl<'a, 'b, COLOR: ColorType + PixelColor> PartialFrame<'a, 'b, COLOR> {
    /// Creates a byte-aligned buffer for you, based on X-coordinate and height.
    ///
    /// Parameters are documented in `Display` as they are the same as the const generics there.
    /// bwrbit should be false for non tricolor displays
    ///
    /// Panics when width or height is equal to 0.
    fn new(
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        buffer: &'b mut [u8],
        full_display_buffer: &'a mut [u8],
        full_display_width: u32,
        full_display_height: u32,
        full_display_size: usize,
        bwrbit: bool,
    ) -> Self {
        if width == 0 {
            panic!("The width should be greater than 0");
        }
        if height == 0 {
            panic!("The height should be greater than 0");
        }
        if x + width > full_display_width || y + height > full_display_height {
            panic!("Partial window out of bounds of full display. Partial windows should fully lie within the bounds of the display.")
        }

        let aligned_x = x & !0b111;
        let x_end = x + width - 1;
        let aligned_x_end = x_end | 0b111;
        let aligned_width = aligned_x_end - aligned_x + 1;

        Self {
            original_x: x,
            aligned_x,
            y,
            original_width: width,
            aligned_width,
            height,
            bwrbit,
            buffer,
            full_display_buffer,
            full_display_width,
            full_display_size,
            rotation: DisplayRotation::default(),
            _color: PhantomData,
        }
    }

    /// get the number of used bytes in the buffer
    fn buffer_size(&self) -> usize {
        self.buffer.len()
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
    pub fn set_pixel(&mut self, mut pixel: Pixel<COLOR>) {
        // Calculate alignment offset based on physical X coordinate
        let offset_left = (self.original_x - self.aligned_x) as i32;
        let offset_right = (self.aligned_width - self.original_width) as i32 - offset_left;

        // Apply offset to the appropriate virtual coordinate
        match self.rotation {
            DisplayRotation::Rotate0 => {
                pixel.0.x += offset_left;
            }
            DisplayRotation::Rotate90 => {
                pixel.0.y += offset_right;
            }
            DisplayRotation::Rotate180 => {
                pixel.0.x += offset_right;
            }
            DisplayRotation::Rotate270 => {
                pixel.0.y += offset_left;
            }
        }

        let size = self.buffer_size();
        set_pixel(
            &mut self.buffer[..size],
            self.aligned_width,
            self.height,
            self.rotation,
            self.bwrbit,
            pixel,
        );
    }

    /// Copy padding pixels from source buffer to destination buffer and update source buffer with destination content.
    ///
    /// This function:
    /// 1. Copies padding bits from `full_display_buffer` to `self.buffer` to fill byte-alignment offsets
    /// 2. Updates `full_display_buffer` with the full content from `self.buffer` to keep buffers in sync
    fn copy_and_sync_buffer(
        &mut self,
        full_display_start: usize,
        full_display_end: usize,
        partial_buffer_start: usize,
        partial_buffer_end: usize,
    ) {
        let full_display_slice =
            &mut self.full_display_buffer[full_display_start..full_display_end];
        let partial_buffer_slice = &mut self.buffer[partial_buffer_start..partial_buffer_end];

        let partial_row_bytes = (self.aligned_width as usize + 7) / 8;
        let full_display_row_bytes = (self.full_display_width as usize + 7) / 8;
        let partial_x_byte_offset = self.aligned_x as usize / 8;

        let left_padding_bits = self.original_x - self.aligned_x;
        let right_padding_bits = self.aligned_width - left_padding_bits - self.original_width;

        for row_idx in 0..self.height as usize {
            let partial_row_start = row_idx * partial_row_bytes;
            let full_display_row_start = (self.y as usize + row_idx) * full_display_row_bytes;
            let full_display_byte_start = full_display_row_start + partial_x_byte_offset;

            // Copy left padding bits from full display to partial buffer
            copy_left_padding_bits(
                &mut partial_buffer_slice[partial_row_start],
                full_display_slice[full_display_byte_start],
                left_padding_bits,
            );

            // Copy right padding bits from full display to partial buffer
            let partial_last_byte_idx = partial_row_start + partial_row_bytes - 1;
            let full_display_last_byte_idx = full_display_byte_start + partial_row_bytes - 1;
            copy_right_padding_bits(
                &mut partial_buffer_slice[partial_last_byte_idx],
                full_display_slice[full_display_last_byte_idx],
                right_padding_bits,
            );

            // Update full display buffer with the merged content from partial buffer
            full_display_slice
                [full_display_byte_start..full_display_byte_start + partial_row_bytes]
                .copy_from_slice(
                    &partial_buffer_slice[partial_row_start..partial_row_start + partial_row_bytes],
                );
        }
    }
}

/// Some Monochrome specifics
impl PartialFrame<'_, '_, Color> {
    /// To be used as parameters for the [`crate::traits::WaveshareDisplay::update_partial_frame`] function.
    ///
    /// Copies padding pixels from `from_buffer` to fill the byte-alignment offset on both left and right sides.
    /// Also updates `from_buffer` with the contents of the partial frame to keep it consistent.
    pub fn get_update_parameters(&mut self) -> PartialUpdateParameters<'_> {
        self.copy_and_sync_buffer(0, self.full_display_size, 0, self.buffer.len());

        PartialUpdateParameters {
            x: self.aligned_x,
            y: self.y,
            width: self.aligned_width,
            height: self.height,
            buffer: &self.buffer,
        }
    }
}

/// Some Tricolor specifics
impl PartialFrame<'_, '_, TriColor> {
    /// get black/white internal buffer to use it (to draw in epd)
    pub fn bw_buffer(&self) -> &[u8] {
        &self.buffer[..self.buffer_size() / 2]
    }

    /// get chromatic internal buffer to use it (to draw in epd)
    pub fn chromatic_buffer(&self) -> &[u8] {
        &self.buffer[self.buffer_size() / 2..self.buffer_size()]
    }

    /// To be used as parameters for the [`crate::traits::WaveshareDisplay::update_partial_frame`] function.
    ///
    /// Copies padding pixels from `from_buffer` to fill the byte-alignment offset on both left and right sides.
    /// Also updates `from_buffer` with the contents of the partial frame to keep it consistent.
    pub fn get_update_parameters(&mut self) -> PartialUpdateParameters<'_> {
        let half_size = self.buffer_size() / 2;
        let full_display_half_size = self.full_display_size / 2;

        // Process BW buffer
        self.copy_and_sync_buffer(0, full_display_half_size, 0, half_size);

        // Process chromatic buffer
        self.copy_and_sync_buffer(
            full_display_half_size,
            self.full_display_size,
            half_size,
            self.buffer_size(),
        );

        PartialUpdateParameters {
            x: self.aligned_x,
            y: self.y,
            width: self.aligned_width,
            height: self.height,
            buffer: &self.buffer,
        }
    }
}

/// Copy the leftmost `offset_pixels` bits from src to dst
fn copy_left_padding_bits(dst: &mut u8, src: u8, offset_pixels: u32) {
    if offset_pixels == 0 {
        return;
    }

    // Create mask for the padding bits (leftmost offset_pixels bits)
    // For example, if offset_pixels = 3: mask = 0b11100000
    let padding_mask = 0xFFu8 << (8 - offset_pixels);

    // Clear padding bits in dst and copy from src
    *dst = (*dst & !padding_mask) | (src & padding_mask);
}

/// Copy the rightmost `offset_pixels` bits from src to dst
fn copy_right_padding_bits(dst: &mut u8, src: u8, offset_pixels: u32) {
    if offset_pixels == 0 {
        return;
    }

    // Create mask for the padding bits (rightmost offset_pixels bits)
    // For example, if offset_pixels = 3: mask = 0b00000111
    let padding_mask = (1u8 << offset_pixels) - 1;

    // Clear padding bits in dst and copy from src
    *dst = (*dst & !padding_mask) | (src & padding_mask);
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
    bwrbit: bool,
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

    let index = x as usize * COLOR::BITS_PER_PIXEL_PER_BUFFER / 8
        + y as usize * line_bytes(width, COLOR::BITS_PER_PIXEL_PER_BUFFER);
    let (mask, bits) = color.bitmask(bwrbit, x as u32);

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
        let display = Display::<200, 200, false, { 200 * 200 / 8 }, Color>::default();
        assert_eq!(display.buffer().len(), 5000);
    }

    // test default background color on all bytes
    #[test]
    fn graphics_default() {
        let display = Display::<200, 200, false, { 200 * 200 / 8 }, Color>::default();
        for &byte in display.buffer() {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn graphics_rotation_0() {
        let mut display = Display::<200, 200, false, { 200 * 200 / 8 }, Color>::default();
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
        let mut display = Display::<200, 200, false, { 200 * 200 / 8 }, Color>::default();
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
        let mut display = Display::<200, 200, false, { 200 * 200 / 8 }, Color>::default();
        display.set_rotation(DisplayRotation::Rotate180);
        let _ = Line::new(Point::new(192, 199), Point::new(199, 199))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(&mut display);

        let buffer = display.buffer();

        extern crate std;
        std::println!("{:?}", buffer);

        assert_eq!(buffer[0], Color::Black.get_byte_value());

        for &byte in buffer.iter().skip(1) {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn graphics_rotation_270() {
        let mut display = Display::<200, 200, false, { 200 * 200 / 8 }, Color>::default();
        display.set_rotation(DisplayRotation::Rotate270);
        let _ = Line::new(Point::new(199, 0), Point::new(199, 7))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(&mut display);

        let buffer = display.buffer();

        extern crate std;
        std::println!("{:?}", buffer);

        assert_eq!(buffer[0], Color::Black.get_byte_value());

        for &byte in buffer.iter().skip(1) {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn graphics_set_pixel_tricolor_false() {
        let mut display = Display::<4, 4, false, { 4 * 4 * 2 / 8 }, TriColor>::default();
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
        let mut display = Display::<4, 4, true, { 4 * 4 * 2 / 8 }, TriColor>::default();
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
