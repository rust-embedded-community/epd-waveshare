//! Raw SPI/GPIO test for Waveshare 2.13" V4 (SSD1680).
//! Bypasses the entire epd-waveshare driver — sends the exact Python init
//! sequence byte-by-byte to isolate hardware vs driver issues.

use linux_embedded_hal::gpio_cdev::{Chip, LineHandle, LineRequestFlags};
use linux_embedded_hal::spidev::{SpiModeFlags, SpidevOptions};
use linux_embedded_hal::SpidevDevice;
use std::io::Write;
use std::thread;
use std::time::Duration;

struct RawEpd {
    spi: SpidevDevice,
    dc: LineHandle,
    rst: LineHandle,
    pwr: LineHandle,
    busy: LineHandle,
}

impl RawEpd {
    fn send_cmd(&mut self, cmd: u8) {
        self.dc.set_value(0).unwrap();
        self.spi.write(&[cmd]).unwrap();
    }

    fn send_data(&mut self, data: u8) {
        self.dc.set_value(1).unwrap();
        self.spi.write(&[data]).unwrap();
    }

    fn send_data_bulk(&mut self, data: &[u8]) {
        self.dc.set_value(1).unwrap();
        // Write in chunks of 4096 (Linux SPI limit)
        for chunk in data.chunks(4096) {
            self.spi.write(chunk).unwrap();
        }
    }

    fn wait_busy(&self) {
        // SSD1680: BUSY pin HIGH = busy, LOW = idle
        let mut count = 0u32;
        while self.busy.get_value().unwrap() == 1 {
            thread::sleep(Duration::from_millis(10));
            count += 1;
            if count > 500 {
                println!("  WARN: busy timeout after 5s");
                return;
            }
        }
        if count > 0 {
            println!("  busy waited {}ms", count * 10);
        }
    }

    fn reset(&mut self) {
        self.rst.set_value(1).unwrap();
        thread::sleep(Duration::from_millis(20));
        self.rst.set_value(0).unwrap();
        thread::sleep(Duration::from_millis(2));
        self.rst.set_value(1).unwrap();
        thread::sleep(Duration::from_millis(20));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- SPI setup ---
    let mut spi = SpidevDevice::open("/dev/spidev0.0")?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(4_000_000)
        .mode(SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)?;

    // --- GPIO setup ---
    let mut chip = Chip::new("/dev/gpiochip0")?;
    let dc = chip
        .get_line(25)?
        .request(LineRequestFlags::OUTPUT, 0, "epd-dc")?;
    let rst = chip
        .get_line(17)?
        .request(LineRequestFlags::OUTPUT, 1, "epd-rst")?;
    let pwr = chip
        .get_line(18)?
        .request(LineRequestFlags::OUTPUT, 0, "epd-pwr")?;
    let busy = chip
        .get_line(24)?
        .request(LineRequestFlags::INPUT, 0, "epd-busy")?;

    let mut epd = RawEpd {
        spi,
        dc,
        rst,
        pwr,
        busy,
    };

    // --- Step 1: Power on ---
    println!("1. PWR HIGH");
    epd.pwr.set_value(1)?;
    thread::sleep(Duration::from_millis(10));

    // --- Step 2: Reset ---
    println!("2. Reset");
    epd.reset();

    // --- Step 3: Wait busy ---
    println!("3. Wait busy after reset");
    epd.wait_busy();

    // --- Step 4: SWRESET ---
    println!("4. SWRESET (0x12)");
    epd.send_cmd(0x12);
    println!("5. Wait busy after SWRESET");
    epd.wait_busy();

    // --- Step 5: Driver output control ---
    println!("6. Driver output control (0x01 + F9 00 00)");
    epd.send_cmd(0x01);
    epd.send_data(0xF9);
    epd.send_data(0x00);
    epd.send_data(0x00);

    // --- Step 6: Data entry mode ---
    println!("7. Data entry mode (0x11 + 03)");
    epd.send_cmd(0x11);
    epd.send_data(0x03);

    // --- Step 7: Set window ---
    println!("8. Set window (0x44/0x45)");
    // SetWindow(0, 0, 121, 249)
    epd.send_cmd(0x44);
    epd.send_data(0x00); // x_start >> 3 = 0
    epd.send_data(0x0F); // x_end >> 3 = 121 >> 3 = 15
    epd.send_cmd(0x45);
    epd.send_data(0x00); // y_start low
    epd.send_data(0x00); // y_start high
    epd.send_data(0xF9); // y_end low = 249
    epd.send_data(0x00); // y_end high

    // --- Step 8: Set cursor ---
    println!("9. Set cursor (0x4E/0x4F)");
    // SetCursor(0, 0)
    epd.send_cmd(0x4E);
    epd.send_data(0x00);
    epd.send_cmd(0x4F);
    epd.send_data(0x00);
    epd.send_data(0x00);

    // --- Step 9: Border waveform ---
    println!("10. Border waveform (0x3C + 05)");
    epd.send_cmd(0x3C);
    epd.send_data(0x05);

    // --- Step 10: Display update control 1 ---
    println!("11. Display update control 1 (0x21 + 00 80)");
    epd.send_cmd(0x21);
    epd.send_data(0x00);
    epd.send_data(0x80);

    // --- Step 11: Temperature sensor ---
    println!("12. Temperature sensor (0x18 + 80)");
    epd.send_cmd(0x18);
    epd.send_data(0x80);

    // --- Step 12: Wait busy ---
    println!("13. Wait busy after init");
    epd.wait_busy();

    println!("=== Init complete ===");

    // --- Clear: send 0xFF to RAM ---
    println!("14. Clear: write 4000 bytes of 0xFF to RAM (0x24)");
    epd.send_cmd(0x24);
    let white_buf = vec![0xFFu8; 4000];
    epd.send_data_bulk(&white_buf);

    // --- Turn on display ---
    println!("15. Turn on display (0x22+F7, 0x20)");
    epd.send_cmd(0x22);
    epd.send_data(0xF7);
    epd.send_cmd(0x20);
    println!("16. Wait busy for display refresh...");
    epd.wait_busy();

    println!("=== Display refresh complete ===");
    println!("Display should show all white.");
    println!("Waiting 5s then exiting (no sleep, PWR stays high)...");
    thread::sleep(Duration::from_secs(5));

    Ok(())
}
