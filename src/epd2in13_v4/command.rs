//! SPI Commands for the Waveshare 2.13" V4 (SSD1680)

use crate::traits;

/// EPD 2.13" V4 commands
#[allow(dead_code)]
#[derive(Copy, Clone)]
#[repr(u8)]
pub(crate) enum Command {
    /// Software reset
    SwReset = 0x12,
    /// Driver output control
    DriverOutputControl = 0x01,
    /// Data entry mode setting
    DataEntryModeSetting = 0x11,
    /// Set RAM X address start/end position
    SetRamXAddressStartEndPosition = 0x44,
    /// Set RAM Y address start/end position
    SetRamYAddressStartEndPosition = 0x45,
    /// Border waveform control
    BorderWaveformControl = 0x3C,
    /// Display update control 1
    DisplayUpdateControl1 = 0x21,
    /// Temperature sensor control
    TemperatureSensorControl = 0x18,
    /// Set RAM X address counter
    SetRamXAddressCounter = 0x4E,
    /// Set RAM Y address counter
    SetRamYAddressCounter = 0x4F,
    /// Display update control 2
    DisplayUpdateControl2 = 0x22,
    /// Master activation
    MasterActivation = 0x20,
    /// Write RAM (BW)
    WriteRam = 0x24,
    /// Write RAM (RED/second)
    WriteRam2 = 0x26,
    /// Deep sleep mode
    DeepSleepMode = 0x10,
    /// Write temperature register
    WriteTempRegister = 0x1A,
    /// Read built-in temperature sensor (sent as command, not data)
    ReadBuiltInTempSensor = 0x80,
}

impl traits::Command for Command {
    /// Returns the address of the command
    fn address(self) -> u8 {
        self as u8
    }
}
