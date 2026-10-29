# buzsak-well-pump

Rust firmware for an Industrial Shields ESP32 PLC 14 IO (`14iosplc_v`, 16 MB flash) that controls a well pump and a tank pump. It is the Rust port of an Arduino/C++ firmware; see [backlog.md](backlog.md) for the plan and what is done.

Current state: Wi-Fi and a safe LAN OTA update path (rollback on an unhealthy image). The pump control logic and I/O are not ported yet.

## Layout

| Path | Purpose |
|---|---|
| `src/main.rs` | Initialisation and orchestration |
| `src/wifi.rs` | Station Wi-Fi with reconnect |
| `src/ota.rs`, `src/ota/` | OTA server, upload, image vetting, auth, confirm/rollback |
| `src/secrets.rs` | Credentials (gitignored, see below) |
| `partitions.csv`, `sdkconfig.defaults` | Flash layout and ESP-IDF config (rollback enabled) |
| `scripts/make-ota-image.ps1` | Builds the release firmware and the OTA `.bin` with its SHA-256 |

## Setup

1. Install the ESP Rust toolchain (`espup`), `ldproxy` and `espflash`.
2. Copy `src/secrets.rs.example` to `src/secrets.rs` and fill it in.
3. Build (PowerShell): `. $env:USERPROFILE\export-esp.ps1; cargo build`

Notes:
- `.cargo/config.toml` sets `target-dir = "C:/bw"` because esp-idf-sys rejects long output paths on Windows; adjust it for your machine.
- There is deliberately no cargo `runner`, so `cargo run` never flashes a device.
- ESP-IDF v5.5.3 is installed globally by the build.

## OTA image

`scripts/make-ota-image.ps1` writes `C:\bw\ota\buzsak_well.bin` and `.sha256`. An upload is `POST /ota` with Basic auth, `Content-Length` and an `X-Sha256` header; `GET /ota/status` reports the running slot and its state. A new image is only confirmed after a healthy soak, otherwise the bootloader rolls back.
