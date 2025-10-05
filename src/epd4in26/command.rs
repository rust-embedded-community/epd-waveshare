use crate::traits;

#[allow(dead_code)]
#[derive(Copy, Clone)]
pub(crate) enum Command {
    DriverOutputControl = 0x01,
    GateDrivingVoltage = 0x03,
    SourceDrivingVoltage = 0x04,
    BoosterSoftStart = 0x0C,
    DeepSleep = 0x10,
    DataEntryMode = 0x11,
    SwReset = 0x12,
    TemperatureSensorSelection = 0x18,
    TemperatureSensorWrite = 0x1A,
    MasterActivation = 0x20,
    DisplayUpdateControl = 0x22,
    WriteRam = 0x24,
    WriteRam2 = 0x26,
    WriteVcomRegister = 0x2C,
    WriteLut = 0x32,
    BorderWaveformControl = 0x3C,
    SetRamXAddressStartEnd = 0x44,
    SetRamYAddressStartEnd = 0x45,
    SetRamXAddressCounter = 0x4E,
    SetRamYAddressCounter = 0x4F,
}

impl traits::Command for Command {
    fn address(self) -> u8 {
        self as u8
    }
}
