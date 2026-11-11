use embedded_graphics::{
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use embedded_hal::delay::DelayNs;
use epd_waveshare::{
    epd7in3e::{Display7in3e, Epd7in3e, HEIGHT, WIDTH},
    prelude::*,
};
use linux_embedded_hal::{
    spidev::{self, SpidevOptions},
    sysfs_gpio::Direction,
    Delay, SPIError, SpidevDevice, SysfsPin,
};

const SPI_PATH: &str = "/dev/spidev0.0";
const SPI_SPEED_HZ: u32 = 2_000_000;

// Raspberry Pi BCM GPIO numbers used by the Waveshare HAT.
const BUSY_PIN: u64 = 24;
const DC_PIN: u64 = 25;
const RST_PIN: u64 = 17;

fn export_pin(number: u64, direction: Direction) -> SysfsPin {
    let pin = SysfsPin::new(number);
    pin.export().expect("GPIO export failed");
    while !pin.is_exported() {}
    pin.set_direction(direction)
        .expect("setting GPIO direction failed");
    pin
}

fn main() -> Result<(), SPIError> {
    let mut spi = SpidevDevice::open(SPI_PATH)?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(SPI_SPEED_HZ)
        .mode(spidev::SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)?;

    let busy = export_pin(BUSY_PIN, Direction::In);
    let dc = export_pin(DC_PIN, Direction::Out);
    let rst = export_pin(RST_PIN, Direction::Out);
    let mut delay = Delay {};

    let mut epd = Epd7in3e::new(&mut spi, busy, dc, rst, &mut delay, None)?;

    println!("Displaying built-in six-color block test");
    epd.show_6block(&mut spi, &mut delay)?;
    delay.delay_ms(5_000);

    let mut display = Box::new(Display7in3e::default());
    display.clear(HexColor::White).unwrap();

    let colors = [
        HexColor::Black,
        HexColor::White,
        HexColor::Yellow,
        HexColor::Red,
        HexColor::Blue,
        HexColor::Green,
    ];
    let stripe_width = WIDTH / colors.len() as u32;

    for (index, color) in colors.iter().enumerate() {
        let x = index as u32 * stripe_width;
        let width = if index + 1 == colors.len() {
            WIDTH - x
        } else {
            stripe_width
        };
        Rectangle::new(Point::new(x as i32, 0), Size::new(width, HEIGHT))
            .into_styled(PrimitiveStyle::with_fill(*color))
            .draw(&mut *display)
            .unwrap();
    }

    println!("Displaying six-color test pattern");
    epd.update_and_display_frame(&mut spi, display.buffer(), &mut delay)?;
    delay.delay_ms(5_000);

    // Update a 100x60 region. HexColor stores two four-bit pixels per byte.
    const PARTIAL_WIDTH: u32 = 100;
    const PARTIAL_HEIGHT: u32 = 60;
    let partial = [HexColor::colors_byte(HexColor::Red, HexColor::Red);
        (PARTIAL_WIDTH * PARTIAL_HEIGHT / 2) as usize];

    println!("Displaying partial red window");
    epd.update_partial_frame(
        &mut spi,
        &mut delay,
        &partial,
        100,
        100,
        PARTIAL_WIDTH,
        PARTIAL_HEIGHT,
    )?;
    epd.display_frame(&mut spi, &mut delay)?;
    delay.delay_ms(5_000);

    println!("Putting display to sleep");
    epd.sleep(&mut spi, &mut delay)
}
