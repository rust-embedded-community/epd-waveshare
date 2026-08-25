//! SPI Commands for the Waveshare 4.2" (B/C) tri-color E-Ink Display
use crate::traits;
/// EPD4IN2BC commands
///
/// Should rarely (never?) be needed directly.
///
/// For more infos about the addresses and what they are doing look into the pdfs
///
/// The description of the single commands is mostly taken from IL0398.pdf
///
/// This is the same UC8176-class command set as the black/white [`crate::epd4in2`]. It is
/// duplicated rather than shared, matching how the other tri-color variants of existing displays
/// are structured (see [`crate::epd5in83b_v2`] and [`crate::epd7in5b_v2`]).
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub(crate) enum Command {
    /// Set Resolution, LUT selection, BWR pixels, gate scan direction, source shift direction, booster switch, soft reset
    /// One Byte of Data:
    ///     0x0F Red Mode, LUT from OTP
    ///     0x1F B/W Mode, LUT from OTP
    ///     0x2F Red Mode, LUT set by registers
    ///     0x3F B/W Mode, LUT set by registers
    ///
    /// This driver uses 0x0F: tri-color, waveform from OTP.
    PanelSetting = 0x00,
    /// selecting internal and external power
    ///    self.send_data(0x03)?; //VDS_EN, VDG_EN
    ///    self.send_data(0x00)?; //VCOM_HV, VGHL_LV[1], VGHL_LV[0]
    ///    self.send_data(0x2b)?; //VDH
    ///    self.send_data(0x2b)?; //VDL
    ///    self.send_data(0xff)?; //VDHR
    PowerSetting = 0x01,
    /// After the Power Off command, the driver will power off following the Power Off Sequence. This command will turn off charge
    /// pump, T-con, source driver, gate driver, VCOM, and temperature sensor, but register data will be kept until VDD becomes OFF.
    /// Source Driver output and Vcom will remain as previous condition, which may have 2 conditions: floating.
    PowerOff = 0x02,
    /// Setting Power OFF sequence
    PowerOffSequenceSetting = 0x03,
    /// Turning On the Power
    PowerOn = 0x04,
    /// This command enables the internal bandgap, which will be cleared by the next POF.
    PowerOnMeasure = 0x05,
    /// Starting data transmission
    ///     3-times: self.send_data(0x17)?; //07 0f 17 1f 27 2F 37 2f
    BoosterSoftStart = 0x06,
    /// After this command is transmitted, the chip would enter the deep-sleep mode to save power.
    ///
    /// The deep sleep mode would return to standby by hardware reset.
    ///
    /// The only one parameter is a check code, the command would be excuted if check code = 0xA5.
    DeepSleep = 0x07,
    /// This command starts transmitting data and write them into SRAM. To complete data transmission, command DSP (Data
    /// transmission Stop) must be issued. Then the chip will start to send data/VCOM for panel.
    ///
    /// - In B/W mode, this command writes “OLD” data to SRAM.
    /// - In B/W/Red mode, this command writes “B/W” data to SRAM.
    /// - In Program mode, this command writes “OTP” data to SRAM for programming.
    DataStartTransmission1 = 0x10,
    /// Stopping data transmission
    DataStop = 0x11,
    /// While user sent this command, driver will refresh display (data/VCOM) according to SRAM data and LUT.
    ///
    /// After Display Refresh command, BUSY_N signal will become “0” and the refreshing of panel starts.
    DisplayRefresh = 0x12,
    /// This command starts transmitting data and write them into SRAM. To complete data transmission, command DSP (Data
    /// transmission Stop) must be issued. Then the chip will start to send data/VCOM for panel.
    /// - In B/W mode, this command writes “NEW” data to SRAM.
    /// - In B/W/Red mode, this command writes “RED” data to SRAM.
    DataStartTransmission2 = 0x13,

    /// This command stores VCOM Look-Up Table with 7 groups of data. Each group contains information for one state and is stored
    /// with 6 bytes, while the sixth byte indicates how many times that phase will repeat.
    ///
    /// from IL0373
    LutForVcom = 0x20,
    /// This command stores White-to-White Look-Up Table with 7 groups of data. Each group contains information for one state and is
    /// stored with 6 bytes, while the sixth byte indicates how many times that phase will repeat.
    ///
    /// from IL0373
    LutWhiteToWhite = 0x21,
    /// This command stores Black-to-White Look-Up Table with 7 groups of data. Each group contains information for one state and is
    /// stored with 6 bytes, while the sixth byte indicates how many times that phase will repeat.
    ///
    /// from IL0373
    LutBlackToWhite = 0x22,
    /// This command stores White-to-Black Look-Up Table with 7 groups of data. Each group contains information for one state and is
    /// stored with 6 bytes, while the sixth byte indicates how many times that phase will repeat.
    ///
    /// from IL0373
    LutWhiteToBlack = 0x23,
    /// This command stores Black-to-Black Look-Up Table with 7 groups of data. Each group contains information for one state and is
    /// stored with 6 bytes, while the sixth byte indicates how many times that phase will repeat.
    ///
    /// from IL0373
    LutBlackToBlack = 0x24,
    /// The command controls the PLL clock frequency.
    PllControl = 0x30,
    /// This command reads the temperature sensed by the temperature sensor.
    ///
    /// Doesn't work! Waveshare doesn't connect the read pin
    TemperatureSensor = 0x40,
    /// Selects the Internal or External temperature sensor and offset
    TemperatureSensorSelection = 0x41,
    /// Write External Temperature Sensor
    TemperatureSensorWrite = 0x42,
    /// Read External Temperature Sensor
    ///
    /// Doesn't work! Waveshare doesn't connect the read pin
    TemperatureSensorRead = 0x43,
    /// This command indicates the interval of Vcom and data output. When setting the vertical back porch, the total blanking will be kept (20 Hsync)
    VcomAndDataIntervalSetting = 0x50,
    /// This command indicates the input power condition. Host can read this flag to learn the battery condition.
    LowPowerDetection = 0x51,
    /// This command defines non-overlap period of Gate and Source.
    TconSetting = 0x60,
    /// This command defines alternative resolution and this setting is of higher priority than the RES\[1:0\] in R00H (PSR).
    ResolutionSetting = 0x61,
    /// This command defines the Fist Active Gate and First Active Source of active channels.
    GsstSetting = 0x65,
    /// The LUT_REV / Chip Revision is read from OTP address = 0x001.
    ///
    /// Doesn't work! Waveshare doesn't connect the read pin
    Revision = 0x70,
    /// Read Flags. This command reads the IC status
    /// PTL, I2C_ERR, I2C_BUSY, DATA, PON, POF, BUSY
    ///
    /// Doesn't work! Waveshare doesn't connect the read pin
    GetStatus = 0x71,
    /// Automatically measure VCOM. This command reads the IC status
    AutoMeasurementVcom = 0x80,
    /// This command gets the VCOM value
    ///
    /// Doesn't work! Waveshare doesn't connect the read pin
    ReadVcomValue = 0x81,
    /// Set VCM_DC
    VcmDcSetting = 0x82,
    /// This command sets partial window
    PartialWindow = 0x90,
    /// This command makes the display enter partial mode
    PartialIn = 0x91,
    /// This command makes the display exit partial mode and enter normal mode
    PartialOut = 0x92,
    /// After this command is issued, the chip would enter the program mode.
    ///
    /// After the programming procedure completed, a hardware reset is necessary for leaving program mode.
    ///
    /// The only one parameter is a check code, the command would be excuted if check code = 0xA5.
    ProgramMode = 0xA0,
    /// After this command is transmitted, the programming state machine would be activated.
    ///
    /// The BUSY flag would fall to 0 until the programming is completed.
    ActiveProgramming = 0xA1,
    /// The command is used for reading the content of OTP for checking the data of programming.
    ///
    /// The value of (n) is depending on the amount of programmed data, tha max address = 0xFFF.
    ReadOtp = 0xA2,
    /// This command is set for saving power during fresh period. If the output voltage of VCOM / Source is from negative to positive or
    /// from positive to negative, the power saving mechanism will be activated. The active period width is defined by the following two
    /// parameters.
    PowerSaving = 0xE3,
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
        assert_eq!(Command::PowerSaving.address(), 0xE3);

        assert_eq!(Command::PanelSetting.address(), 0x00);

        assert_eq!(Command::DisplayRefresh.address(), 0x12);
    }
}
