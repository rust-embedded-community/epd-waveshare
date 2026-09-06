<p align="center">
    <a href="https://github.com/caemor/epd-waveshare"><img src="https://github.com/caemor/epd-waveshare/actions/workflows/rust.yml/badge.svg" alt="Github CI"></a>
    <a href="https://crates.io/crates/epd-waveshare"><img src="https://img.shields.io/crates/v/epd-waveshare.svg" alt="Crates.io"></a>
    <a href="https://docs.rs/epd-waveshare"><img src="https://docs.rs/epd-waveshare/badge.svg" alt="Docs.rs"></a>
</p>

# EPD Driver

This library contains a driver for E-Paper Modules mostly from Waveshare (which are basically the same as the Dalian Good
Display ones).

It uses the [embedded graphics](https://crates.io/crates/embedded-graphics) library for the optional graphics support.

A 2021-edition compatible version (Rust 1.62+) is needed.

Other similar libraries with support for much more displays are [u8g2](https://github.com/olikraus/u8g2)
and [GxEPD](https://github.com/ZinggJM/GxEPD) for arduino.

## Examples

There are multiple examples in the examples folder. Use `cargo run --example example_name` to try them.

```Rust
// Setup the epd
let mut epd4in2 =
    Epd4in2::new(&mut spi, busy, dc, rst, &mut delay, None).expect("eink initalize error");

// Setup the graphics
let mut display = Display4in2::default();

// Build the style
let style = MonoTextStyleBuilder::new()
    .font(&embedded_graphics::mono_font::ascii::FONT_6X10)
    .text_color(Color::White)
    .background_color(Color::Black)
    .build();
let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();

// Draw some text at a certain point using the specified text style
let _ = Text::with_text_style("It's working-WoB!", Point::new(175, 250), style, text_style)
    .draw(&mut display);

// Show display on e-paper
epd4in2.update_and_display_frame(&mut spi, display.buffer(), &mut delay).expect("display error");

// Going to sleep
epd4in2.sleep(&mut spi, &mut delay)
```

> Check the complete example [here](./examples/epd4in2.rs).

## (Supported) Devices

| Device (with Link) | Colors | Flexible Display | Partial Refresh | Supported | Tested |
| :---: | --- | :---: | :---: | :---: | :---: |
| [7.5 Inch B/W/R V2/V3 (B)](https://www.waveshare.com/product/displays/e-paper/epaper-1/7.5inch-e-paper-b.htm) | Black, White, Red | ✕ | ✕ | ✔ | ✔ |
| [7.5 Inch B/W HD (A)](https://www.waveshare.com/product/displays/e-paper/epaper-1/7.5inch-hd-e-paper-hat.htm) | Black, White | ✕ | ✕ | ✔ | ✔ |
| [7.5 Inch B/W V2 (A)](https://www.waveshare.com/product/7.5inch-e-paper-hat.htm) [[1](#1-75-inch-bw-v2-a)] | Black, White | ✕ | ✕ | ✔ | ✔ |
| [7.5 Inch B/W (A)](https://www.waveshare.com/product/7.5inch-e-paper-hat.htm) | Black, White | ✕ | ✕ | ✔ | ✔ |
| [7.3 Inch HAT (F)](https://www.waveshare.com/product/7.3inch-e-paper-hat-f.htm) | Black, White, Red, Green, Blue, Yellow, Orange | ✕ | ✕ | ✔ | ✔ |
| [5.83 Inch B/W/R (b)](https://www.waveshare.com/5.83inch-e-Paper-B.htm) | Black, White, Red | ✕ | Not officially | ✔ | ✔ |
| [5.65 Inch 7 Color (F)](https://www.waveshare.com/5.65inch-e-paper-module-f.htm) | Black, White, Red, Green, Blue, Yellow, Orange | ✕ | ✕ | ✔ | ✔ |
| [4.2 Inch B/W (A)](https://www.waveshare.com/product/4.2inch-e-paper-module.htm) | Black, White | ✕ | Not officially [[2](#2-42-inch-e-ink-blackwhite---partial-refresh)] | ✔ | ✔ |
| [4.2 Inch B/W V2 (A)](https://www.waveshare.com/4.2inch-e-paper-module.htm) | Black, White | ✕ | ✔ | ✔ | ✔ |
| [2.13 Inch B/W (A) V2](https://www.waveshare.com/product/2.13inch-e-paper-hat.htm) | Black, White | ✕ | ✔ | ✔  | ✔  |
| [2.13 Inch B/W/R (B/C) V2](https://www.waveshare.com/product/raspberry-pi/displays/e-paper/2.13inch-e-paper-hat-b.htm) | Black, White, Red | ✕ | ✕ | ✔  | ✔  |
| [2.9 Inch B/W/R (B/C)](https://www.waveshare.com/product/displays/e-paper/epaper-2/2.9inch-e-paper-module-b.htm) | Black, White, Red | ✕ | ✕ | ✔ | ✔ |
| [2.9 Inch B/W (A)](https://www.waveshare.com/product/2.9inch-e-paper-module.htm) | Black, White | ✕ | ✔ | ✔ | ✔ |
| [2.9 Inch B/W V2 (A)](https://www.waveshare.com/product/2.9inch-e-paper-module.htm) | Black, White | ✕ | ✔ | ✔ | ✔ |
| [2.7 Inch 3 Color (B)](https://www.waveshare.com/2.7inch-e-paper-b.htm) | Black, White, Red | ✕ | ✔ | ✔ | ✔ |
| [2.7 Inch B/W V2](https://www.waveshare.com/2.7inch-e-paper.htm) | Black, White | ✕ | (✔) | ✔ | ✔ |
| [2.66 Inch 3 Color (B)](https://www.waveshare.com/wiki/Pico-ePaper-2.66-B) | Black, White, Red | ✕ | ✕ | ✔ | ✔ |
| [1.54 Inch B/W/Y (C) (Discontinued)](https://www.waveshare.com/1.54inch-e-paper-module-c.htm) | Black, White, Yellow | ✕ | ✕ | ✔ | ✔ |
| [1.54 Inch B/W/R (B)](https://www.waveshare.com/1.54inch-e-Paper-B.htm) | Black, White, Red | ✕ | ✕ | ✔ | ✔ |
| [1.54 Inch B/W (A)](https://www.waveshare.com/1.54inch-e-Paper-Module.htm) | Black, White | ✕ | ✔ | ✔ | ✔ |

### [1]: 7.5 Inch B/W V2 (A)

Since November 2019 Waveshare sells their updated version of these displays. They should have a "V2" marking sticker on
the backside of the panel.

Use `epd7in5_v2` instead of `epd7in5`, because the protocol changed.

### [2]: 4.2 Inch E-Ink Black/White - Partial Refresh

Out of the Box the original driver from Waveshare only supports full updates.

That means: Be careful with the quick refresh updates: <br>
It's possible with this driver but might lead to ghosting / burn-in effects therefore it's hidden behind a feature.

### Interface

| Interface | Description |
| :---: |  :--- |
| VCC    |   3.3V |
| GND   |    GND |
| DIN   |    SPI MOSI |
| CLK   |    SPI SCK |
| CS    |    SPI chip select (Low active) |
| DC    |    Data/Command control pin (High for data, and low for command) |
| RST   |    External reset pin (Low for reset) |
| BUSY  |    Busy state output pin (Low for busy)  |

### Display Configs

There are two types of Display Configurations used in Waveshare EPDs, which also needs to be set on the "new" E-Paper
Driver HAT. They are also called A and B, but you shouldn't get confused and mix it with the Type A,B,C and D of the
various Displays, which just describe different types (colored variants) or new versions. In the Display Config the
separation is most likely due to included fast partial refresh of the displays. In a Tabular form:

| Type A | Type B |
| :---: |  :---: |
| 1.54in (A) | 1.54in (B) |
| 2.13in (A) | 1.54in (C) |
| 2.13in (D) | 2.13in (B) |
| 2.9in (A)  | 2.13in (C) |
|            | 2.7in  (A) |
|            | 2.7in  (B) |
|            | 2.9in  (B) |
|            | 2.9in  (C) |
|            | 4.2in  (A) |
|            | 4.2in  (B) |
|            | 4.2in  (C) |
|            | 7.5in  (A) |
|            | 7.5in  (B) |
|            | 7.5in  (C) |
