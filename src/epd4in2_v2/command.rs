//! SPI commands for the Waveshare 4.2" V2 E-Ink Display
use crate::traits;

#[derive(Copy, Clone)]
pub(crate) enum Command {
    DriverOutputControl = 0x01,
    DeepSleepMode = 0x10,
    DataEntryModeSetting = 0x11,
    SwReset = 0x12,
    TemperatureSensorControl = 0x18,
    WriteTemperatureRegister = 0x1a,
    MasterActivation = 0x20,
    DisplayUpdateControl1 = 0x21,
    DisplayUpdateControl2 = 0x22,
    WriteRamBw = 0x24,
    WriteRamRed = 0x26,
    BorderWaveformControl = 0x3c,
    SetRamXAddressStartEnd = 0x44,
    SetRamYAddressStartEnd = 0x45,
    SetRamXAddressCounter = 0x4e,
    SetRamYAddressCounter = 0x4f,
}

impl traits::Command for Command {
    /// Returns the address of the command
    fn address(self) -> u8 {
        self as u8
    }
}
