use crate::traits;

#[allow(dead_code, clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Command {
    Ox00 = 0x00,
    Ox01 = 0x01,

    PowerOff = 0x02,

    Ox03 = 0x03,

    PowerOn = 0x04,

    Ox05 = 0x05,
    Ox06 = 0x06,

    DeepSleep = 0x07,

    Ox08 = 0x08,

    DataStartTransmission = 0x10,

    DataRefresh = 0x12,

    IPC = 0x13,

    Ox30 = 0x30,

    TSE = 0x41,

    Ox50 = 0x50,
    Ox60 = 0x60,
    Ox61 = 0x61,

    Ox82 = 0x82,
    Ox84 = 0x84,

    CMDH = 0xAA,

    AGID = 0x86,

    CCSET = 0xE0,

    OxE3 = 0xE3,

    TSSET = 0xE6,
}

impl traits::Command for Command {
    /// Returns the address of the command
    fn address(self) -> u8 {
        self as u8
    }
}
