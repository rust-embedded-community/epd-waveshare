//! SPI Commands for the Waveshare 3.52" E-Ink Display

use crate::traits;

/// EPD3IN52 commands
///
/// Should rarely (never?) be needed directly.
///
/// For more infos about the addresses and what they are doing look into the pdfs
#[allow(dead_code)]
#[derive(Copy, Clone)]
#[repr(u8)]
pub(crate) enum Command {
    PanelSetting = 0x00,
    PowerSetting = 0x01,
    BoosterSoftStart = 0x06,
    DataStartTransmission = 0x13,
    Refresh = 0x17,
    LutVcom = 0x20,
    LutBlue = 0x21,
    LutWhite = 0x22,
    LutGray1 = 0x23,
    LutGray2 = 0x24,
    PllControl = 0x30,
    VcomDataSetting = 0x50,
    TconSetting = 0x60,
    ResolutionSetting = 0x61,
    VcomDcSetting = 0x82,
    PowerSaving = 0xE3,
    Sleep = 0x07,
}

impl traits::Command for Command {
    /// Returns the address of the command
    fn address(self) -> u8 {
        self as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Command as CommandTrait;

    #[test]
    fn command_addr() {
        assert_eq!(Command::PanelSetting.address(), 0x00);
        assert_eq!(Command::PowerSetting.address(), 0x01);
        assert_eq!(Command::BoosterSoftStart.address(), 0x06);
        assert_eq!(Command::DataStartTransmission.address(), 0x13);
        assert_eq!(Command::Refresh.address(), 0x17);
        assert_eq!(Command::LutVcom.address(), 0x20);
        assert_eq!(Command::LutBlue.address(), 0x21);
        assert_eq!(Command::LutWhite.address(), 0x22);
        assert_eq!(Command::LutGray1.address(), 0x23);
        assert_eq!(Command::LutGray2.address(), 0x24);
        assert_eq!(Command::PllControl.address(), 0x30);
        assert_eq!(Command::VcomDataSetting.address(), 0x50);
        assert_eq!(Command::TconSetting.address(), 0x60);
        assert_eq!(Command::ResolutionSetting.address(), 0x61);
        assert_eq!(Command::VcomDcSetting.address(), 0x82);
        assert_eq!(Command::PowerSaving.address(), 0xE3);
        assert_eq!(Command::Sleep.address(), 0x07);
    }
}
