//! epd3in52_status — Waveshare 3.52" e-paper status display
//!
//! Target: Raspberry Pi 5 (aarch64, Raspberry Pi OS)
//! Display: Waveshare 3.52" HAT, UC8253 controller, 240x360px
//! GPIO: gpio_cdev backend (Pi 5 uses /dev/gpiochip0, no BCM offset)
//! BUSY polarity: active-low (IS_BUSY_LOW = true)
//!
//! IMPORTANT: Python never refreshes between display_NUM and display.
//! The Rust example must NOT call display_frame() between clear_frame()
//! and update_frame() — doing so advances lut_flag, causing the image
//! refresh to use swapped R22/R23 LUTs which inverts colors.
//!
//! Build and deploy:
//!   cargo build --example epd3in52_status \
//!       --target aarch64-unknown-linux-gnu --release
//!   scp target/aarch64-unknown-linux-gnu/release/examples/epd3in52_status \
//!       <target>:~/
//!   ssh <target> "sudo ./epd3in52_status"

use embedded_graphics::{
    mono_font::{ascii::FONT_8X13, MonoTextStyleBuilder},
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use epd_waveshare::{
    color::Color,
    epd3in52::{Display3in52, Epd3in52, HEIGHT, WIDTH},
    graphics::DisplayRotation,
    prelude::*,
};
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{self, SpidevOptions},
    CdevPin, Delay, SpidevDevice,
};
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// -- GPIO pin numbers (BCM, verified from epdconfig.py) ---------------
const PIN_BUSY: u32 = 24;
const PIN_RST: u32 = 17;
const PIN_DC: u32 = 25;

// -- SPI device ---------------------------------------------------------------
const SPI_DEVICE: &str = "/dev/spidev0.0";
const SPI_SPEED_HZ: u32 = 10_000_000;

struct StatusData {
    hostname: String,
    ip: String,
    timestamp: String,
    temp: Option<f32>,
    uptime: String,
    used_mb: u64,
    total_mb: u64,
    cpu_percent: Option<u32>,
    disk_used_gb: Option<f64>,
    disk_total_gb: Option<f64>,
    disk_percent: Option<u32>,
    batt_percent: Option<f64>,
    batt_voltage: Option<f64>,
    batt_charging: bool,
}

impl StatusData {
    fn collect() -> Self {
        let cpu_percent = read_cpu_percent();
        let (used_mb, total_mb) = read_ram_usage();
        let (disk_used_gb, disk_total_gb, disk_percent) = read_disk_usage();
        let (batt_percent, batt_voltage, batt_charging) = read_battery();
        StatusData {
            hostname: read_hostname(),
            ip: read_local_ip(),
            timestamp: format_timestamp(),
            temp: read_cpu_temp(),
            uptime: read_uptime(),
            used_mb,
            total_mb,
            cpu_percent,
            disk_used_gb,
            disk_total_gb,
            disk_percent,
            batt_percent,
            batt_voltage,
            batt_charging,
        }
    }
}

struct EpdConfig {
    rotation: DisplayRotation,
    color_invert: bool,
}

fn parse_config() -> EpdConfig {
    if !Path::new("/etc/epd-waveshare.conf").exists() {
        eprintln!("epd-waveshare: no config at /etc/epd-waveshare.conf, using defaults");
    }
    let mut rotation = DisplayRotation::Rotate0;
    let mut color_invert = false;
    let content = std::fs::read_to_string("/etc/epd-waveshare.conf").unwrap_or_default();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "rotation" => match value.trim() {
                    "0" => rotation = DisplayRotation::Rotate0,
                    "90" => rotation = DisplayRotation::Rotate90,
                    "180" => rotation = DisplayRotation::Rotate180,
                    "270" => rotation = DisplayRotation::Rotate270,
                    _ => eprintln!(
                        "epd-waveshare: invalid rotation value '{}', using 0",
                        value.trim()
                    ),
                },
                "color_invert" => {
                    color_invert = value.trim() == "true";
                }
                _ => {}
            }
        }
    }
    EpdConfig {
        rotation,
        color_invert,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("epd3in52_status -- Waveshare 3.52\"");

    const EXPECTED_BUF_LEN: usize = 240 / 8 * 360;

    // -- Read config ----------------------------------------------------------
    let config = parse_config();
    let (logical_w, _logical_h) = match config.rotation {
        DisplayRotation::Rotate0 | DisplayRotation::Rotate180 => (WIDTH, HEIGHT),
        DisplayRotation::Rotate90 | DisplayRotation::Rotate270 => (HEIGHT, WIDTH),
    };
    let rotation_degrees: u16 = match config.rotation {
        DisplayRotation::Rotate0 => 0,
        DisplayRotation::Rotate90 => 90,
        DisplayRotation::Rotate180 => 180,
        DisplayRotation::Rotate270 => 270,
    };

    // -- Collect system stats (CPU read takes ~500ms) -------------------------
    let data = StatusData::collect();

    // -- SPI setup (SpidevDevice, not Spidev) ---------------------------------
    let mut spi =
        SpidevDevice::open(SPI_DEVICE).map_err(|e| format!("open /dev/spidev0.0: {e}"))?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(SPI_SPEED_HZ)
        .mode(spidev::SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)
        .map_err(|e| format!("configure /dev/spidev0.0: {e}"))?;

    // -- GPIO setup (gpio_cdev on Pi 5) ---------------------------------------
    let mut chip = Chip::new("/dev/gpiochip0").map_err(|e| format!("open /dev/gpiochip0: {e}"))?;

    let busy = CdevPin::new(
        chip.get_line(PIN_BUSY)
            .map_err(|e| format!("claim GPIO24 (BUSY): {e}"))?
            .request(LineRequestFlags::INPUT, 0, "epd3in52-busy")
            .map_err(|e| format!("request GPIO24 (BUSY) as input: {e}"))?,
    )?;
    let dc = CdevPin::new(
        chip.get_line(PIN_DC)
            .map_err(|e| format!("claim GPIO25 (DC): {e}"))?
            .request(LineRequestFlags::OUTPUT, 0, "epd3in52-dc")
            .map_err(|e| format!("request GPIO25 (DC) as output: {e}"))?,
    )?;
    let rst = CdevPin::new(
        chip.get_line(PIN_RST)
            .map_err(|e| format!("claim GPIO17 (RST): {e}"))?
            .request(LineRequestFlags::OUTPUT, 1, "epd3in52-rst")
            .map_err(|e| format!("request GPIO17 (RST) as output: {e}"))?,
    )?;

    let mut delay = Delay;

    // -- 1. Init display → lut_flag=false -------------------------------------
    println!("Initialising display...");
    let mut epd = Epd3in52::new(&mut spi, busy, dc, rst, &mut delay, None)
        .map_err(|e| format!("EPD init (UC8253): {e}"))?;

    // -- 2. Clear display RAM (no refresh!) -----------------------------------
    println!("Clearing display RAM...");
    epd.clear_frame(&mut spi, &mut delay)?;

    let test_pattern = std::env::var("EPD_TEST_PATTERN").is_ok();
    if test_pattern {
        println!("Sending test pattern (top white / bottom black)...");
        let mut buf = vec![0xFFu8; EXPECTED_BUF_LEN];
        for b in buf[5400..].iter_mut() {
            *b = 0x00;
        }
        epd.update_frame(&mut spi, &buf, &mut delay)?;
    } else {
        // -- 3. Build frame buffer --------------------------------------------
        println!("Rendering...");
        let mut display = Display3in52::default();
        display.set_rotation(config.rotation);

        assert_eq!(
            display.buffer().len(),
            EXPECTED_BUF_LEN,
            "buffer length must be 240/8 * 360 = 10800"
        );

        display.clear(Color::White).ok();
        assert!(
            display.buffer().iter().all(|&b| b == 0xFF),
            "buffer must be all-white (0xFF) after clear"
        );

        draw_status(&mut display, &data, logical_w, _logical_h)?;

        println!("Sending frame...");
        let final_buffer: Vec<u8> = if config.color_invert {
            display.buffer().iter().map(|&b| !b).collect()
        } else {
            display.buffer().to_vec()
        };
        epd.update_frame(&mut spi, &final_buffer, &mut delay)?;
    }

    // -- 4. Single refresh (lut_flag=false, matching Python Flag=0) -----------
    epd.display_frame(&mut spi, &mut delay)?;

    // -- 5. Sleep -------------------------------------------------------------
    epd.sleep(&mut spi, &mut delay)?;

    // -- Summary line ---------------------------------------------------------
    let temp_str = data
        .temp
        .map(|t| format!("{:.1}", t))
        .unwrap_or_else(|| "--".to_string());
    let cpu_str = data
        .cpu_percent
        .map(|p| format!("{}%", p))
        .unwrap_or_else(|| "--".to_string());
    let disk_str = match (data.disk_used_gb, data.disk_total_gb) {
        (Some(u), Some(t)) => format!("{:.0}/{:.0}GB", u, t),
        _ => "--".to_string(),
    };
    let batt_str = data
        .batt_percent
        .map(|p| format!("{:.0}%", p))
        .unwrap_or_else(|| "--".to_string());
    println!(
        "[{}] Display updated. Rotation {}° Inv:{} Temp {}°C  CPU {}  RAM {}/{}MB  Disk {}  Batt {}  Up {}",
        data.timestamp,
        rotation_degrees,
        config.color_invert,
        temp_str,
        cpu_str,
        data.used_mb,
        data.total_mb,
        disk_str,
        batt_str,
        data.uptime
    );

    Ok(())
}

// -- Frame content ------------------------------------------------------------
fn draw_status(
    display: &mut Display3in52,
    data: &StatusData,
    w: u32,
    _h: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = MonoTextStyleBuilder::new()
        .font(&FONT_8X13)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();
    let header_title = MonoTextStyleBuilder::new()
        .font(&FONT_8X13)
        .text_color(Color::White)
        .background_color(Color::Black)
        .build();

    // ── Header bar (full width, 28px tall) ───────────────────────────────────
    Rectangle::new(Point::new(0, 0), Size::new(w, 28))
        .into_styled(PrimitiveStyle::with_fill(Color::Black))
        .draw(display)?;
    Text::with_baseline(
        "Raspberry Pi 5  8GB",
        Point::new(6, 8),
        header_title,
        Baseline::Top,
    )
    .draw(display)?;
    let ts_short = format!(
        "{} UTC",
        data.timestamp.get(..16).unwrap_or(&data.timestamp)
    );
    let ts_x = (w as i32) - (ts_short.len() as i32 * 8) - 6;
    Text::with_baseline(&ts_short, Point::new(ts_x, 8), header_title, Baseline::Top)
        .draw(display)?;

    // ── Stats (18px line spacing, all FONT_8X13) ─────────────────────────────
    let x = 6;

    // y=30: UP
    Text::with_baseline(
        &format!("UP    {}", data.uptime),
        Point::new(x, 30),
        body,
        Baseline::Top,
    )
    .draw(display)?;

    // y=48: TEMP
    let temp_str = match data.temp {
        Some(t) => format!("TEMP  {:.1} C", t),
        None => "TEMP  --".to_string(),
    };
    Text::with_baseline(&temp_str, Point::new(x, 48), body, Baseline::Top).draw(display)?;

    // y=66: CPU
    let cpu_str = data
        .cpu_percent
        .map(|p| format!("CPU   {}%", p))
        .unwrap_or_else(|| "CPU   --%".to_string());
    Text::with_baseline(&cpu_str, Point::new(x, 66), body, Baseline::Top).draw(display)?;

    // y=84: RAM
    Text::with_baseline(
        &format!("RAM   {} / {} MB", data.used_mb, data.total_mb),
        Point::new(x, 84),
        body,
        Baseline::Top,
    )
    .draw(display)?;

    // y=102: DISK
    let disk_str = match (data.disk_used_gb, data.disk_total_gb, data.disk_percent) {
        (Some(u), Some(t), Some(p)) => format!("DISK  {:.1} / {:.1} GB  {}%", u, t, p),
        _ => "DISK  --".to_string(),
    };
    Text::with_baseline(&disk_str, Point::new(x, 102), body, Baseline::Top).draw(display)?;

    // y=120: HOST
    Text::with_baseline(
        &format!("HOST  {}", data.hostname),
        Point::new(x, 120),
        body,
        Baseline::Top,
    )
    .draw(display)?;

    // y=138: IP
    Text::with_baseline(
        &format!("IP    {}", data.ip),
        Point::new(x, 138),
        body,
        Baseline::Top,
    )
    .draw(display)?;

    // y=156: BATT
    let batt_str = match (data.batt_percent, data.batt_voltage) {
        (Some(pct), Some(v)) => {
            let indicator = if data.batt_charging { "  +" } else { "" };
            format!("BATT  {:.0}%  {:.2}V{}", pct, v, indicator)
        }
        _ => "BATT  unavailable".to_string(),
    };
    Text::with_baseline(&batt_str, Point::new(x, 156), body, Baseline::Top).draw(display)?;

    // y=170: battery progress bar (only if battery data available)
    if let Some(pct) = data.batt_percent {
        let bar_x = x;
        let bar_y = 170;
        let bar_w = w / 2;
        let bar_h = 10u32;
        // Outline
        Rectangle::new(Point::new(bar_x, bar_y), Size::new(bar_w, bar_h))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(display)?;
        // Fill
        let fill_w = ((bar_w - 2) as f64 * pct.clamp(0.0, 100.0) / 100.0) as u32;
        if fill_w > 0 {
            Rectangle::new(
                Point::new(bar_x + 1, bar_y + 1),
                Size::new(fill_w, bar_h - 2),
            )
            .into_styled(PrimitiveStyle::with_fill(Color::Black))
            .draw(display)?;
        }
    }

    Ok(())
}

// -- System info helpers (read from /proc, no external deps) ------------------

fn read_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string()
}

fn read_local_ip() -> String {
    use std::net::UdpSocket;
    // UDP connect() sends no packet; it just picks the outbound interface
    // so local_addr() reports this host's routable IP toward 8.8.8.8.
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:53")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "no network".to_string())
}

fn format_timestamp() -> String {
    let secs = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) if d.as_secs() > 1_600_000_000 => d.as_secs(),
        _ => {
            eprintln!("epd-waveshare: system clock not synced (pre-2020), displaying CLK?");
            return "CLK? unsynced".to_string();
        }
    };
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, month, day, h, m, s
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }
    let leap = is_leap(year);
    let md: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for &m in &md {
        if days < m {
            break;
        }
        days -= m;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn read_cpu_temp() -> Option<f32> {
    std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .map(|v| v as f32 / 1000.0)
}

fn read_ram_usage() -> (u64, u64) {
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        } else if line.starts_with("MemAvailable:") {
            avail_kb = line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
    }
    let used_mb = (total_kb.saturating_sub(avail_kb)) / 1024;
    let total_mb = total_kb / 1024;
    (used_mb, total_mb)
}

fn read_uptime() -> String {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .map(|secs| {
            let secs = secs as u64;
            let d = secs / 86400;
            let h = (secs % 86400) / 3600;
            let m = (secs % 3600) / 60;
            if d > 0 {
                format!("{}d {:02}h {:02}m", d, h, m)
            } else {
                format!("{:02}h {:02}m", h, m)
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Read CPU utilization by sampling /proc/stat twice with a 500ms gap.
fn read_cpu_percent() -> Option<u32> {
    let parse_cpu_line = |s: &str| -> Option<(u64, u64)> {
        let fields: Vec<u64> = s
            .split_whitespace()
            .skip(1) // skip "cpu"
            .take(7) // user nice system idle iowait irq softirq
            .filter_map(|v| v.parse().ok())
            .collect();
        if fields.len() < 7 {
            return None;
        }
        let idle = fields[3] + fields[4]; // idle + iowait
        let total: u64 = fields.iter().sum();
        Some((total, idle))
    };

    let read_first_line = || -> Option<String> {
        std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|s| s.lines().next().map(String::from))
    };

    let line1 = read_first_line()?;
    let (total1, idle1) = parse_cpu_line(&line1)?;

    std::thread::sleep(std::time::Duration::from_millis(500));

    let line2 = read_first_line()?;
    let (total2, idle2) = parse_cpu_line(&line2)?;

    let dt = total2.saturating_sub(total1);
    let di = idle2.saturating_sub(idle1);
    if dt == 0 {
        return Some(0);
    }
    Some((100 * dt.saturating_sub(di) / dt) as u32)
}

/// Read root filesystem usage by spawning `df -k /` and parsing output.
fn read_disk_usage() -> (Option<f64>, Option<f64>, Option<u32>) {
    let output = match std::process::Command::new("df").args(["-k", "/"]).output() {
        Ok(o) => o,
        Err(_) => return (None, None, None),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = match stdout.lines().nth(1) {
        Some(l) => l,
        None => return (None, None, None),
    };
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 5 {
        return (None, None, None);
    }
    let total_kb: f64 = fields[1].parse().unwrap_or(0.0);
    let used_kb: f64 = fields[2].parse().unwrap_or(0.0);
    let pct: u32 = fields[4].trim_end_matches('%').parse().unwrap_or(0);
    let total_gb = total_kb / 1_048_576.0;
    let used_gb = used_kb / 1_048_576.0;
    (Some(used_gb), Some(total_gb), Some(pct))
}

/// Query pisugar-server via unix socket. Returns (percent, voltage, charging).
fn read_battery() -> (Option<f64>, Option<f64>, bool) {
    let batt_pct = query_pisugar("get battery").and_then(|r| parse_pisugar_float(&r));
    let batt_v = query_pisugar("get battery_v").and_then(|r| parse_pisugar_float(&r));
    let charging = query_pisugar("get battery_charging")
        .map(|r| r.contains("true"))
        .unwrap_or(false);
    (batt_pct, batt_v, charging)
}

fn query_pisugar(cmd: &str) -> Option<String> {
    let mut stream = UnixStream::connect("/tmp/pisugar-server.sock").ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    writeln!(stream, "{}", cmd).ok()?;
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).ok()?;
    Some(response.trim().to_string())
}

fn parse_pisugar_float(response: &str) -> Option<f64> {
    response.rsplit_once(':')?.1.trim().parse().ok()
}
