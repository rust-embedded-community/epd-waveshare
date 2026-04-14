# EPD Status Display

Systemd service and timer for the Waveshare 2.13" V4 e-paper status display on Pi Zero 2W nodes.

## What it does

Runs `epd2in13_v4_status` on a schedule to display system info on the e-paper:
hostname, IP, date/time, uptime, CPU/temp, RAM, disk, and PiSugar battery status.

## Hardware

- Waveshare 2.13" e-Paper HAT V4 (SSD1680 controller)
- GPIO: PWR=18, RST=17, DC=25, BUSY=24, SPI CE0
- SPI: `/dev/spidev0.0`, 4MHz, mode 0

## Build

From the repo root on the build host:

```bash
cargo build --target aarch64-unknown-linux-gnu --example epd2in13_v4_status --features epd2in13_v4 --release
```

## Install

Copy the repo (or at minimum `scripts/` and the built binary) to the Pi, then:

```bash
sudo ./scripts/install_epd_status.sh
```

This installs all five components:

1. The `epd2in13_v4_status` binary to `/usr/local/bin/`
2. `epd-status.service` and `epd-status.timer` to `/etc/systemd/system/` (the periodic refresh)
3. `epd-status-boot.service` to `/etc/systemd/system/` (boot-time state reset, see below)
4. `journald-volatile.conf` to `/etc/systemd/journald.conf.d/volatile.conf` (SD card wear protection, see below)

The timer is enabled and started, the boot service is enabled, and `systemd-journald` is restarted to pick up the volatile config.

## Boot state reset

`epd-status-boot.service` is a `oneshot` unit that runs before `epd-status.timer` on every boot and removes the state files under `/var/lib/epd-status/` (`initialized`, `base_set`, `rotation`).

Why: the e-paper's controller RAM is cleared on power cycle, but the state files on the SD card survive across reboots. Without the reset, the next run after a cold boot would skip the full refresh and try to resume partial-refresh updates against an uninitialized display, leaving garbage on screen. Clearing the state files forces the correct full → base → partial refresh cycle on the first run after every boot.

The service is ordered `Before=epd-status.timer` and wanted by `sysinit.target`, so it always completes before the first scheduled refresh.

## Journald volatile config

`journald-volatile.conf` switches journald to RAM-only storage with a 10MB cap:

```ini
[Journal]
Storage=volatile
RuntimeMaxUse=10M
```

Why: on Pi Zero 2W nodes the root filesystem lives on an SD card, and persistent journald writes are a meaningful source of write amplification over long deployments. Volatile storage keeps logs in `/run/log/journal` (tmpfs) so day-to-day logging never touches the card. Logs are lost on reboot, which is acceptable for these unattended status-display nodes.

**Not recommended for development machines** (build hosts, Pi 5, anywhere with an SSD or where you debug across reboots) — persistent logs are valuable there. This config is specifically a deployment-node tradeoff.

## Timing

| Setting | Default | Description |
|---------|---------|-------------|
| `OnBootSec` | 45s | Delay after boot before first update (wait for network) |
| `OnUnitActiveSec` | 5min | Interval between updates |
| `AccuracySec` | 30s | Allows systemd to batch timer wakeups for power efficiency |

To change the refresh interval, edit `/etc/systemd/system/epd-status.timer`:

```bash
sudo systemctl edit epd-status.timer
```

Add an override:

```ini
[Timer]
OnUnitActiveSec=10min
```

Then reload: `sudo systemctl daemon-reload`

## Commands

```bash
# Run immediately (don't wait for timer)
sudo systemctl start epd-status.service

# Check timer status
systemctl status epd-status.timer

# View recent logs
journalctl -u epd-status.service -n 20

# Stop the timer
sudo systemctl stop epd-status.timer

# Disable (won't start on boot)
sudo systemctl disable epd-status.timer

# Uninstall everything
sudo systemctl disable epd-status.timer epd-status-boot.service
sudo rm /etc/systemd/system/epd-status.{service,timer}
sudo rm /etc/systemd/system/epd-status-boot.service
sudo rm /etc/systemd/journald.conf.d/volatile.conf
sudo rm /usr/local/bin/epd2in13_v4_status
sudo systemctl daemon-reload
sudo systemctl restart systemd-journald
```
