# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add support for the 4.2 inch 3 color (B/C) display, in both the red (B) and yellow (C) pigments

## [v0.6.0] - 2024-10-28

### Added

- Add support for positive and negatives modes of rendering in TriColor display in #92 (thanks to @akashihi)
- Add Epd 5in83 V2 (B) support in #92 (thanks to @akashihi)
- Add Epd 7in5 (B) V2 and V3 support in #118 (should also work on v3), thanks to @peckpeck
- Add 7.3 Inch HAT (F) support in #191 (thanks to @jetjinser)
- Add support for EPD 2in9 D in #171 (thanks to @wsndshx)
- Add Epd 5in83 V2 support in #159 (thanks to @Carbonhell)
- Add support for Pi hat Pico Epd2in66B (B/W/R) in #147 (thanks to @ReinoutHeeck and @Lite5h4dow)
- Add support for EPD 2in13 v3 in #126 and #138 (thanks to @fmeef)
- Add embedded-graphics traits for color in #132 (thanks to @peckpeck)
- Add support for EPD 3in7 in #129 (thanks to @mangelajo)
- Add convert traits for TriColor
- Add support for GDEH0154D67 (aka epd1in54_v2) in #106 (thanks to @jcard0na)
- Add option to switch between single byte and blockwise data writen to the spi device
- Added tests and fixed sized method when rotated

### Changed

- Made Examples and Linux embedded hal optional (linux only) and therefore allowed building on other hosts (#101, #94)
- Update to eh-1.0, eh-mock 0.10 , leh 0.4.0
- Documentation tweaks
- Updated and improved Examples and Readme multiple times (thanks to @shymega and many others)
- Updated refresh rate for 2.9in v2 display to make it much faster thanks to @andber1 in #150 (and #185)
- Removed epd7in5_v3 in favour of edp7in5b_v2 since they work the same way in #177
- Migrated to Rust 2021 in #133 (thanks to @peckpeck)
- Migrate `DelayMs<u8>` to `DelayUs<u32>` to allow shorter and longer sleeps in #131 (thanks to @peckpeck)
- Improved delay handling by allowing busy or sleep loops in wait_for_idle in #125 thanks to @peckpeck
- Make Display more generic in #123 and #121 (thanks to @peckpeck)

### Fixed

- Overflow error for all displays thanks to @tippfehlr in @186
- Fix build when feature graphics is not enabled in #176 (thanks to @vhdirk)
- Optimize overflow in the calculation of `NUM_DISPLAY_BYTES` on small architectures in #173 (thanks to @Idicarlo)
- Fixed init code for epd1in54 thanks to @fakusb in #156
- Fix off-by-one bug for `set_pixel` in #148 thanks to @ReinoutHeeck
- Fix 7in5(HD) by allowing blockwise data to be written in #141 (see issues and discussions in #70, #83, #142)
- Fix enter deep sleep for epd1in54 v2 in #139 (thanks to @jcard0na)
- Fixed buffer length in display struct in #128 (thanks to @peckpeck)
- LUT Fixes for EPD 2in9 v2 in #103 (thanks to @mike-kfed)
- Fix pins for epd2in13_v2 example in #91 Universal e-Paper Raw Panel Driver HAT (thanks to @ole-treichel)
- Fix Color Bitmask calculation for OctColor in #190 (thanks to @jetjinser)

## [v0.5.0] - 2021-11-28

### Added

- Added QuickRefresh Trait and implemented it for EPD4in2 in #62 (thanks to @David-OConnor)
- Added Epd 2in7 (B) support in #60 (thanks to @pjsier)
- Added Epd 7in5 HD support (thanks to @whiite)
- Added Epd 2in9 V2 support in #73 & #74 (thanks to @andber1)
- Added Epd 2in13 (BC) support in #75 (thanks to @Irbalt)
- Added Color conversion methods in #87 & #88 (thanks to @crzysdrs)
- Provide full QuickRefresh interface for 4.2 inch display in #81 (thanks to @sirhcel)

### Changed

- Updated embedded-graphics to 0.7 and switch to e-g-core #78 (@Irbalt) & #85 (@jamwaffles)
- Use specific ParseColorError instead of ()
- Epd4in2: Don't set the resolution (and some more) over and over again (#48)
- Removed `#[allow(non_camel_case_types)]` to fix various issues around it
- Added Delay to QuickRefresh Trait due to #74 (thanks to @andber1)
- Write data over SPI 1 byte at a time due to #82 (thanks to @belak)
- Enable drawing in three colors for epd2in13bc in #76 (thanks to @Irbalt)

## [v0.4.0] - 2020-04-06

### Added

- New supported epds: epd7in5 (thanks to @str4d), epd7in5 v2 (thanks to @asaaki), epd1in54b (thanks to @jkristell)
- Added update_and_display_frame to WaveshareDisplay trait (fixes #38)
- also improve position of busy_wait (#30) once more
- More Documentation

### Changed

- Update embedded-graphics to 0.6 (changes Display Trait) (and to 0.5 before thanks to @dbr)
- Remove useless feature gates (Doesn't change size)
- Update and integrate a few important examples and remove the others
- Use Embedded_hal:digital::v2

### Fixed

- Doc Tests

## [v0.3.2] - 2019-06-17

### Fixed

- Added some more missing wait_until_idle calls

## [v0.3.1] - 2019-04-06

### Added

- Example for epd4in2 and BluePill-Board

### Changed

- Improved CI

### Fixed

- Timing issues in display_frame function: epd1in54 and epd2in9 were both missing a necessary wait_until_idle call at
  the end of their display_frame function which sometimes caused invalid/ignored commands/inputs afterwards
- Some CI Targets were not tested correctly before

## [v0.3.0] - 2019-04-04

### Added

- added eink to keywords
- added reference to previous crate-name
- improved readme/docs e.g. added reference to a few great arduino display libs for these epds
- Added is_busy to Waveshare_Interface
- Added IS_BUSY_LOW const for all supported epds
- Added is_busy to DisplayInterface
- Added VarDisplay (a variable buffer-size display/graphic driver)
- Updated and added more examples
- add a feature gated alternative full lut for type_a displays

### Removed

- Removed all Buffers (Buffer1in54,...) and instead made specialised Displays (Display1in54,...) with included Buffers

### Changed

- Switch to 2018 edition
- "cargo fix --edition" for the library
- Use cargo fix edition-idioms and remove the internal renaming from embedded_hal to hal
- moved width, height and default_background_color directly to epd4in2 module
- remove pub from set_lut_helper function
- fix behaviour of set_lut for epd2in9. it always sets the LUT now!

## v0.2.0 - 2018-10-30

Initial release with Changelog

### Added

- Uses embedded-graphics now
- Tested and fixed 1.54 inch, 2.9 inch and 4.2 inch display

### Removed

- Old included Graphics Library

### Changed

- Lots of internal changes
- Renamed to `epd-waveshare`

[v0.6.0]: https://github.com/Caemor/epd-waveshare/compare/v0.5.0...v0.6.0

[v0.5.0]: https://github.com/Caemor/epd-waveshare/compare/v0.4.0...v0.5.0

[v0.4.0]: https://github.com/Caemor/epd-waveshare/compare/v0.3.2...v0.4.0

[v0.3.2]: https://github.com/Caemor/epd-waveshare/compare/v0.3.1...v0.3.2

[v0.3.1]: https://github.com/Caemor/epd-waveshare/compare/v0.3.0...v0.3.1

[v0.3.0]: https://github.com/Caemor/epd-waveshare/compare/v0.2.0...v0.3.0
