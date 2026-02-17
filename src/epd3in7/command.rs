//! SPI Commands for the Waveshare 3.7" E-Ink Display

use crate::traits;

/// EPD3IN7 commands
///
/// Should rarely (never?) be needed directly.
///
/// For more infos about the addresses and what they are doing look into the pdfs
///
/// The description of the single commands is mostly taken from EDP3IN7 specification
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub(crate) enum Command {
    GateSetting = 0x01,
    PowerOff = 0x02,
    Sleep2 = 0x07,
    GateVoltage = 0x03,
    GateVoltageSource = 0x04,
    BoosterSoftStartControl = 0x0C,
    /// After this command initiated, the chip will enter Deep Sleep Mode,
    /// BUSY pad will keep output high.
    ///
    /// Note: To exit Deep Sleep Mode, User required to send HWRESET to the driver.
    DeepSleep = 0x10,
    DataEntrySequence = 0x11,
    /// This command resets commands and parameters to their S/W Reset default values,
    /// except Deep Sleep Mode.
    /// During this operation BUSY pad will keep output high.
    ///
    /// Note: RAM is unaffected by this command.
    SwReset = 0x12,
    /// This command selects the Internal or External temperature sensor and offset
    TemperatureSensorSelection = 0x18,
    /// Write to temperature register
    TemperatureSensorWrite = 0x1A,
    /// Read from temperature register
    TemperatureSensorRead = 0x1B,
    /// This command activates Display Update sequence.
    /// The Display Update sequence option is located at R22h.
    ///
    /// Note: BUSY pad will output high during operation. User **should not** interrupt this operation
    /// to avoid corruption of panel images.
    DisplayUpdateSequence = 0x20,
    /// This command sets a Display Update Sequence option.
    DisplayUpdateSequenceSetting = 0x22,
    /// This command will transfer its data to B/W RAM, until another command is written
    WriteRam = 0x24,
    /// This command writes VCOM register from MCU interface
    WriteVcomRegister = 0x2C,
    /// This command will transfer its data to R RAM, until another command is written
    WriteRamRed = 0x26,
    /// This command writes LUT register from MCU interface (105 bytes),
    /// which contains the content of VS [nx-LUT], TP #[nX], RP #[n]
    WriteLutRegister = 0x32,
    DisplayOption = 0x37,
    BorderWaveformControl = 0x3C,
    /// This command specifies the start/end positions of the window address in the X direction,
    /// by an address unit of RAM.
    SetRamXAddressStartEndPosition = 0x44,
    /// This command specifies the start/end positions of the window address in the Y direction,
    /// by an address unit of RAM.
    SetRamYAddressStartEndPosition = 0x45,
    AutoWriteRedRamRegularPattern = 0x46,
    AutoWriteBwRamRegularPattern = 0x47,
    /// This command makes the initial settings for the RAM X address in the address counter (AC)
    SetRamXAddressCounter = 0x4E,
    /// This command makes the initial settings for the RAM Y address in the address counter (AC)
    SetRamYAddressCounter = 0x4F,
    Sleep = 0x50,
}

impl traits::Command for Command {
    /// Returns the address of the command
    fn address(self) -> u8 {
        self as u8
    }
}
