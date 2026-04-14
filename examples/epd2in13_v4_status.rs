//! System status display for Pi Zero 2W on Waveshare 2.13" V4 (SSD1680).
//! Reads real system data via /proc + pisugar socket and renders to e-paper.
//!
//! Uses partial refresh on subsequent runs to minimize e-paper wear.
//! Two state files in `/tmp` (cleared on reboot) control the sequence:
//!
//! 1. First run  — full refresh, creates `epd_status_initialized`
//! 2. Second run — `display_part_base_image` (establishes base in both RAM
//!    banks with one full refresh), creates `epd_status_base_set`
//! 3. Third+ runs — `display_partial` only (true partial waveform, no flashing)
//!
//! Rotation changes automatically trigger a full refresh cycle
//! to clear ghosting from the previous orientation.

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use epd_waveshare::{
    epd2in13_v4::{Display2in13, Epd2in13, HEIGHT, WIDTH},
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

const STATE_DIR: &str = "/var/lib/epd-status";
const STATE_FILE: &str = "/var/lib/epd-status/initialized";
const BASE_FILE: &str = "/var/lib/epd-status/base_set";
const ROTATION_FILE: &str = "/var/lib/epd-status/rotation";

// ---- Data collection ----

struct StatusData {
    hostname: String,
    ip: String,
    datetime: String,
    uptime: String,
    cpu: String,
    memory: String,
    disk: String,
    battery: String,
    voltage: String,
    refresh_mode: String,
}

impl StatusData {
    fn collect() -> Self {
        let cpu = Self::read_cpu();
        let (battery, voltage) = Self::read_battery();

        let refresh_mode = if !Path::new(STATE_FILE).exists() {
            "full".into()
        } else if !Path::new(BASE_FILE).exists() {
            "base".into()
        } else {
            "partial".into()
        };

        StatusData {
            hostname: Self::read_hostname(),
            ip: Self::read_ip(),
            datetime: Self::read_datetime(),
            uptime: Self::read_uptime(),
            cpu,
            memory: Self::read_memory(),
            disk: Self::read_disk(),
            battery,
            voltage,
            refresh_mode,
        }
    }

    fn read_hostname() -> String {
        std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_uppercase()
    }

    fn read_ip() -> String {
        // UDP connect() sends no packet; it just picks the outbound interface
        // so local_addr() reports this host's routable IP toward 8.8.8.8.
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if sock.connect("8.8.8.8:53").is_ok() {
                if let Ok(addr) = sock.local_addr() {
                    return addr.ip().to_string();
                }
            }
        }
        "no IP".into()
    }

    fn read_datetime() -> String {
        let secs = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) if d.as_secs() > 1_600_000_000 => d.as_secs(),
            _ => {
                eprintln!("epd-waveshare: system clock not synced (pre-2020), displaying CLK?");
                return "CLK? unsynced".into();
            }
        };
        let days_since_epoch = secs / 86400;
        let time_of_day = secs % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let (year, month, day) = days_to_ymd(days_since_epoch);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            year, month, day, hours, minutes
        )
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
                let total = secs as u64;
                let days = total / 86400;
                let hours = (total % 86400) / 3600;
                let mins = (total % 3600) / 60;
                if days > 0 {
                    format!("Up: {}d {}h {}m", days, hours, mins)
                } else {
                    format!("Up: {}h {}m", hours, mins)
                }
            })
            .unwrap_or_else(|| "Up: ??".into())
    }

    fn read_cpu() -> String {
        // CPU usage via /proc/stat delta
        let usage = read_cpu_percent().map(|p| p as f32).unwrap_or(0.0);
        let temp = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map(|t| format!("{:.1}C", t as f64 / 1000.0))
            .unwrap_or_else(|| "??C".into());
        format!("CPU: {:.0}% {}", usage, temp)
    }

    fn read_memory() -> String {
        let content = match std::fs::read_to_string("/proc/meminfo") {
            Ok(c) => c,
            Err(_) => return "RAM: ??".into(),
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
        let used_mb = total_kb.saturating_sub(avail_kb) / 1024;
        let total_mb = total_kb / 1024;
        format!("RAM: {}/{}MB", used_mb, total_mb)
    }

    fn read_disk() -> String {
        let output = match std::process::Command::new("df").args(["-k", "/"]).output() {
            Ok(o) => o,
            Err(_) => return "DSK: ??".into(),
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = match stdout.lines().nth(1) {
            Some(l) => l,
            None => return "DSK: ??".into(),
        };
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 5 {
            return "DSK: ??".into();
        }
        let total_kb: f64 = fields[1].parse().unwrap_or(0.0);
        let used_kb: f64 = fields[2].parse().unwrap_or(0.0);
        let total_gb = total_kb / 1_048_576.0;
        let used_gb = used_kb / 1_048_576.0;
        format!("DSK: {:.1}/{:.1}GB", used_gb, total_gb)
    }

    fn read_battery() -> (String, String) {
        let bat_pct = query_pisugar("get battery").and_then(|r| parse_pisugar_float(&r));
        let charging = query_pisugar("get battery_charging")
            .map(|r| r.contains("true"))
            .unwrap_or(false);

        let bat_line = match bat_pct {
            Some(pct) => {
                let indicator = if charging { " CHG" } else { "" };
                format!("BAT: {:.0}%{}", pct, indicator)
            }
            None => "BAT: N/A".into(),
        };

        let volt_line = query_pisugar("get battery_v")
            .and_then(|r| parse_pisugar_float(&r))
            .map(|v| format!("VOLT: {:.2}V", v))
            .unwrap_or_else(|| "VOLT: N/A".into());

        (bat_line, volt_line)
    }

    fn summary(&self, rotation_degrees: u16, color_invert: bool, reoriented: bool) -> String {
        let reoriented_str = if reoriented { " | REORIENTED" } else { "" };
        format!(
            "{} | ROT:{} | Inv:{}{} | {} | {} | {} | {} | {} | {} | {} | {}",
            self.refresh_mode,
            rotation_degrees,
            color_invert,
            reoriented_str,
            self.hostname,
            self.ip,
            self.datetime,
            self.uptime,
            self.cpu,
            self.memory,
            self.disk,
            self.battery
        )
    }
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

// ---- Rendering ----

fn render(
    display: &mut Display2in13,
    data: &StatusData,
    w: u32,
    _h: u32,
) -> Result<(), core::convert::Infallible> {
    let fill_black = PrimitiveStyle::with_fill(Color::Black);

    let white_on_black = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Color::White)
        .background_color(Color::Black)
        .build();
    let black_on_white = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();
    let right_align = TextStyleBuilder::new()
        .alignment(Alignment::Right)
        .baseline(Baseline::Top)
        .build();

    // Header bar: hostname left, IP right
    Rectangle::new(Point::new(0, 0), Size::new(w, 13))
        .into_styled(fill_black)
        .draw(display)?;
    Text::with_baseline(
        &data.hostname,
        Point::new(2, 2),
        white_on_black,
        Baseline::Top,
    )
    .draw(display)?;
    Text::with_text_style(
        &data.ip,
        Point::new((w - 2) as i32, 2),
        white_on_black,
        right_align,
    )
    .draw(display)?;

    let mut y = 15;

    // Date/time
    Text::with_baseline(
        &data.datetime,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)?;
    y += 12;

    // Uptime
    Text::with_baseline(
        &data.uptime,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)?;
    y += 12;

    // CPU
    Text::with_baseline(&data.cpu, Point::new(2, y), black_on_white, Baseline::Top)
        .draw(display)?;
    y += 12;

    // RAM
    Text::with_baseline(
        &data.memory,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)?;
    y += 12;

    // Disk
    Text::with_baseline(&data.disk, Point::new(2, y), black_on_white, Baseline::Top)
        .draw(display)?;
    y += 12;

    // Battery
    Text::with_baseline(
        &data.battery,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)?;
    y += 12;

    // Voltage
    Text::with_baseline(
        &data.voltage,
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)?;
    y += 12;

    // Refresh mode
    Text::with_baseline(
        &format!("RFR: {}", data.refresh_mode),
        Point::new(2, y),
        black_on_white,
        Baseline::Top,
    )
    .draw(display)?;

    Ok(())
}

// ---- Main ----

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(STATE_DIR)?;

    let config = parse_config();
    let (logical_w, logical_h) = match config.rotation {
        DisplayRotation::Rotate0 | DisplayRotation::Rotate180 => (WIDTH, HEIGHT),
        DisplayRotation::Rotate90 | DisplayRotation::Rotate270 => (HEIGHT, WIDTH),
    };
    let rotation_degrees: u16 = match config.rotation {
        DisplayRotation::Rotate0 => 0,
        DisplayRotation::Rotate90 => 90,
        DisplayRotation::Rotate180 => 180,
        DisplayRotation::Rotate270 => 270,
    };

    // Detect rotation changes and force full refresh if needed
    let current_rotation = rotation_degrees.to_string();
    let reoriented = if Path::new(ROTATION_FILE).exists() {
        let last_rotation = std::fs::read_to_string(ROTATION_FILE)
            .unwrap_or_default()
            .trim()
            .to_string();
        if last_rotation != current_rotation {
            eprintln!(
                "epd-waveshare: rotation changed {} -> {}, forcing full refresh",
                last_rotation, current_rotation
            );
            true
        } else {
            false
        }
    } else {
        false
    };

    let data = StatusData::collect();

    // EPD setup
    let mut spi =
        SpidevDevice::open("/dev/spidev0.0").map_err(|e| format!("open /dev/spidev0.0: {e}"))?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(4_000_000)
        .mode(spidev::SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)
        .map_err(|e| format!("configure /dev/spidev0.0: {e}"))?;

    let mut chip = Chip::new("/dev/gpiochip0").map_err(|e| format!("open /dev/gpiochip0: {e}"))?;
    let busy = CdevPin::new(
        chip.get_line(24)
            .map_err(|e| format!("claim GPIO24 (BUSY): {e}"))?
            .request(LineRequestFlags::INPUT, 0, "epd-busy")
            .map_err(|e| format!("request GPIO24 (BUSY) as input: {e}"))?,
    )?;
    let dc = CdevPin::new(
        chip.get_line(25)
            .map_err(|e| format!("claim GPIO25 (DC): {e}"))?
            .request(LineRequestFlags::OUTPUT, 0, "epd-dc")
            .map_err(|e| format!("request GPIO25 (DC) as output: {e}"))?,
    )?;
    let rst = CdevPin::new(
        chip.get_line(17)
            .map_err(|e| format!("claim GPIO17 (RST): {e}"))?
            .request(LineRequestFlags::OUTPUT, 1, "epd-rst")
            .map_err(|e| format!("request GPIO17 (RST) as output: {e}"))?,
    )?;
    let pwr = CdevPin::new(
        chip.get_line(18)
            .map_err(|e| format!("claim GPIO18 (PWR): {e}"))?
            .request(LineRequestFlags::OUTPUT, 0, "epd-pwr")
            .map_err(|e| format!("request GPIO18 (PWR) as output: {e}"))?,
    )?;

    let mut delay = Delay;
    let mut epd = Epd2in13::new_with_pwr(&mut spi, busy, dc, rst, &mut delay, None, pwr)
        .map_err(|e| format!("EPD init (SSD1680): {e}"))?;

    // Render to framebuffer
    let mut display = Display2in13::default();
    display.set_rotation(config.rotation);
    display.clear(Color::White).ok();
    render(&mut display, &data, logical_w, logical_h)?;

    let final_buffer: Vec<u8> = if config.color_invert {
        display.buffer().iter().map(|&b| !b).collect()
    } else {
        display.buffer().to_vec()
    };
    let buf = final_buffer.as_slice();
    let initialized = !reoriented && Path::new(STATE_FILE).exists();
    let base_set = !reoriented && Path::new(BASE_FILE).exists();

    if !initialized {
        // First run since boot (or rotation change) — full refresh
        epd.update_frame(&mut spi, buf, &mut delay)?;
        epd.display_frame(&mut spi, &mut delay)?;
        let _ = std::fs::remove_file(BASE_FILE);
        std::fs::write(STATE_FILE, "")?;
    } else if !base_set {
        // Second run — establish partial base (writes both RAM banks, one full refresh)
        epd.display_part_base_image(&mut spi, buf, &mut delay)?;
        std::fs::write(BASE_FILE, "")?;
    } else {
        // All subsequent runs — true partial refresh only
        epd.display_partial(&mut spi, buf, &mut delay)?;
    }

    let _ = std::fs::write(ROTATION_FILE, &current_rotation);

    epd.sleep(&mut spi, &mut delay)?;

    println!(
        "{}",
        data.summary(rotation_degrees, config.color_invert, reoriented)
    );

    Ok(())
}
