use crate::traits;

#[allow(dead_code)]
#[derive(Copy, Clone)]
pub(crate) enum Command {
    GateSettings = 0x01,
    GateVoltage = 0x03,
    SourceVoltage = 0x04,
    DeepSleep = 0x10,
    DataEntryMode = 0x11,
    SwReset = 0x12,
    TemperatureSensorControl = 0x18,
    TemperatureSensorControlWrite = 0x1A,
    DisplayUpdateSequence = 0x20,
    DisplayUpdateControl1 = 0x21,
    DisplayUpdateControl2 = 0x22,
    WriteRamBw = 0x24,
    WriteRamRed = 0x26,
    WriteVcomRegister = 0x2C,
    OTPRead = 0x2D,
    StatusBitRead = 0x2F,
    WriteLutRegister = 0x32,
    BorderWaveform = 0x3C,
    SetRamXAdressStartEndPosition = 0x44,
    SetRamYAdressStartEndPosition = 0x45,
    SetRamXAdressCounter = 0x4E,
    SetRamYAdressCounter = 0x4F,

    BoosterSoftStart = 0x0C,
}

impl traits::Command for Command {
    fn address(self) -> u8 {
        self as u8
    }
}
