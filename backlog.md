# Backlog — buzsak_well (Rust port of Garden_IS_ESP32_PLC_14_IO)

Target: Industrial Shields ESP32 PLC 14 IO, board `14iosplc_v` (0–10 V variant, hardware v4), ESP32, 16 MB flash (confirm with `esptool flash_id`).
Reference project: `C:\Scripts\Embedded\Garden_IS_ESP32_PLC_14_IO` (C++/PlatformIO/Arduino).

Rules:
- Nothing is implemented or flashed without explicit consent.
- Device is remote; every update must be recoverable without physical access.
- Pumps must be OFF in any uncertain state.

## Decisions so far

| Topic | Decision |
|---|---|
| Stack | `esp-idf-svc` (std Rust on ESP-IDF), Xtensa `esp` toolchain via `espup` |
| OTA transport | Simple LAN push to a device HTTP endpoint; no server, no HTTPS/signing initially |
| OTA safety | ESP-IDF app rollback (pending-verify + mark-valid), two OTA slots, no factory partition |
| Migration | Preferred: first Rust image pushed through the existing ElegantOTA (bootloader rollback is probably already enabled, see Findings); USB session only as fallback. Must be proven on a dev board first |
| Bench hardware | No spare PLC; use a cheap plain ESP32 dev board (>= 4 MB flash) for OTA/rollback tests |

## Phase 0 — Understand the original

- [x] Review original OTA flow (ArduinoOTA + ElegantOTA + `scripts/http_ota.py`)
- [x] Review IO map, calibration constants, control logic overview (README, `io_config.h`)
- [x] Read `logic.cpp` in detail (selector modes, refill, hysteresis, fault handling)
- [x] Read `diag.cpp` and decide whether diagnostics are ported (proposal: no, see Findings)
- [x] Inventory HTTP API (`/api/state`, `/api/control`, `/`) so the companion app keeps working
- [x] Verify whether the current Industrial Shields bootloader has app rollback enabled (config yes; binary still to confirm on device)
- [x] Note: original repo has plaintext Wi-Fi/OTA credentials in source and `http_ota.py`; do not copy

### Phase 0 findings

**Hardware facts (verified in the installed framework `framework-industrialshields-esp32` 2.3.1-hf)**
- Board `14iosplc_v`: 0–10 V analog front end, 16 MB flash, flash mode qout @ 80 MHz, partitions `default_16MB.csv`.
- README is wrong about the analog inputs: `I0_7` = GPIO 34 and `I0_8` = GPIO 35 (GPIO 32/33 are the `_MA` variant). The 4–20 mA sensors use an external 470 ohm shunt into the 0–10 V input; board divider 3.82 (matches `pressureMvToBar`).
- `AC7_EN` (GPIO 15) and `AC8_EN` (GPIO 2) are driven LOW at core init for the V variant. The Rust firmware must do the same (both are strapping pins: do not change their boot-time levels).
- ADC set up by core: 12-bit, clock div 1, attenuation 3 (11 dB). The app uses `analogReadMilliVolts`, which is the raw `esp_adc_cal` path (eFuse calibration) and bypasses the V-variant correction (+171 / x1.295) used by `analogRead`. The Rust ADC must match this (oneshot + calibration) and be validated against C++ readings.
- Outputs `Q0_0..3` = GPIO 12, 25, 26, 27. Mode pins `S0..S3_24v` live on the MCP23017 (0x200F, 0x200E, 0x2001, 0x2000); core drives them LOW (5 V) at boot, app sets HIGH (24 V) before enabling the outputs. GPIO 12 is a strapping pin (flash voltage): keep it LOW at boot.
- MCP23017 at I2C 0x20, bus 0 (SDA 21, SCL 22). Pin code `0x20nn` = expander bit `nn` (0-7 port A, 8-15 port B). Digital inputs `I0_0..6` and relay `R0_0` (0x2007) are on it. Init: IOCON = SEQOP | ODR on both ports, all other registers cleared, GPPU = 0 (no pull-ups, board provides them), 100 ms delay after power-up before talking to it.
- Inputs used: `I0_0` = 0x200B (right), `I0_1` = 0x200A (left), active LOW.

**Control logic (`logic.cpp`, scan every 50 ms)**
- Selector: both contacts LOW = invalid (-1), right = AUTO (1), left = MANUAL (2), none = OFF (0).
- OFF / invalid: well pump, tank pump, switch LED all OFF; AUTO state reset.
- AUTO: switch LED solid. Well pump hysteresis in liters: ON at <= 300 L, OFF at >= 1000 L, forced OFF when water-level mV == 0 (no signal). Tank pump hysteresis in bar: ON below 3.0, OFF at >= 4.0, forced OFF when pressure <= 0.1 bar.
- MANUAL: well pump ON continuously, tank pump OFF, switch LED blinks at 500 ms. Water level is NOT checked in manual mode (no overflow protection).
- Refill: remote request only accepted in AUTO and not already active; button flag high for 500 ms; refill runs well pump until >= 1000 L, stops on selector change or missing water-level signal; refill takes priority over level hysteresis.
- Possible behavior gaps to decide on (do not silently copy or change): no pump run-time limit or dry-run protection; no sensor-stuck detection; Wi-Fi LED pin is configured but pumps do not depend on Wi-Fi (good, keep it that way); `plcLoop` writes outputs every cycle.

**HTTP API (must stay compatible with the dashboard and the companion app)**
- `GET /` dashboard (`assets/index.html`, embedded at build via `scripts/embed_assets.py`); it polls `/api/state` every 2 s and links to `/update`.
- `GET /api/state` -> `{firmware, ip, switch_mode (-1|0|1|2), refill_button, refill_active, outputs:[{name,label,state,controllable}], analog:[{name,label,mv,signal, bar? | liters?,low?}]}`. `bar` only for pressure when valid (> 512 mV); `liters` and `low` for water level; `signal` for water level is `mv > 0`.
- `POST /api/control` (form `name=refill&state=1|0|true|false`): 400 missing/invalid, 403 other names, 409 not AUTO or already requested/active, 200 `{ok:true, refill_button}`. `state=0` is a no-op returning the current button state.
- No authentication on `/` or the API (only `/update` has Basic auth). Decide whether the Rust version adds auth for `POST /api/control`.
- Other consumers (Flask server / app) are not in the repo: confirm field names with the user before changing anything.

**Diagnostics (`diag.cpp`)**
- Temporary probe: samples GPIO 32-35 (raw/calibrated/mV) every 2 s to LittleFS CSV, `/diag`, `/diag.csv`, `/diag/clear` (unauthenticated, GET clears the log). It also forces `AC7_EN/AC8_EN` LOW and is marked in the README as to-be-disabled. Proposal: not ported; replace later with a small optional debug endpoint if needed.

**OTA / bootloader**
- Existing OTA: ArduinoOTA (password) + ElegantOTA (`/update`, Basic auth) + `scripts/http_ota.py`. ArduinoOTA start forces outputs OFF; the ElegantOTA path does not.
- The framework sdkconfig has `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` (anti-rollback and secure boot off, flash encryption off). So the bootloader very likely supports pending-verify and rollback already.
- BUT the Arduino core auto-confirms the running image in `initArduino()` before `setup()` (`esp_ota_mark_app_valid_cancel_rollback()`), so the C++ firmware gets no benefit from it today.
- Partition table `default_16MB.csv`: `nvs` 0x9000, `otadata` 0xE000, `app0`/`app1` 0x10000 and 0x650000 (6.25 MB each), `spiffs` 0xC90000 (3.4 MB), no factory partition. This already has the layout needed for safe OTA, and the current C++ firmware is ~0.9 MB.
- Consequence: a Rust image uploaded via the existing ElegantOTA should boot as pending-verify, and roll back to the C++ firmware unless the Rust app confirms itself. Remaining unknowns: the prebuilt bootloader binary on the device actually has the flag (confirm on a dev board with the same bootloader and by reading the device bootloader/otadata), and an IDF 5.x app image booting from this older IDF 4.4 bootloader.
- Flash mode on the board is qout @ 80 MHz; the Rust image header must be compatible (check `espflash`/image flash-mode settings).

**Security findings in the original repo (do not copy to Rust)**
- Wi-Fi SSID/password, ArduinoOTA password and ElegantOTA credentials are hardcoded in `src/wifi_network.cpp`; the ElegantOTA credentials are also in `scripts/http_ota.py`. Both files are tracked in git and the repo has a GitHub remote, so these secrets are in history. Recommend rotating them if the remote is public or shared. README claims they are only in the gitignored `login.md`, which is not true.
- OTA and API are plain HTTP, no signing; MD5 only guards against transfer errors.

## Phase 1 — OTA safety prototype (dev board, no pump logic)

- [x] Install toolchain: `esp` Rust toolchain (espup) and `ldproxy` present; ESP-IDF v5.5.3 reused from `~/.espressif`
- [x] Install `espflash` 4.6.0 (host tool, used offline to create the OTA `.bin`)
- [x] Scaffold `esp-idf-svc` project in this repo (target, `sdkconfig.defaults`, `.cargo/config.toml`); `cargo build` succeeds with zero warnings, nothing was run or flashed
- [x] Partition table identical to the device (`partitions.csv`, same as `default_16MB.csv`); adapt sizes for a smaller dev board
- [ ] Confirm the device bootloader has `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` (framework config says yes; our own build has it on); on the dev board use the same prebuilt bootloader from `framework-industrialshields-esp32/tools/sdk/esp32/bin`
- [x] Rust app does NOT auto-confirm at startup (confirms only after a healthy soak)
- [x] Wi-Fi connect with credentials from `src/secrets.rs` (gitignored); never gates the control logic
- [x] HTTP upload endpoint `POST /ota` (Basic auth, Content-Length and size limit, streaming write via `EspOta`) and `GET /ota/status`
- [x] Pre-activation checks: SHA-256 header (`X-Sha256`), ESP image magic, chip id, project name, version strictly newer, image fits slot; slot is only activated after validation and hash match
- [x] Boot-time check of OTA state; confirmation thread runs only when pending-verify
- [x] Soak (120 s healthy, 600 s deadline) then `mark_running_slot_valid`; on deadline `mark_running_slot_invalid_and_reboot`; current health = Wi-Fi up only
- [ ] Add real health checks to the soak (I2C expander answers, ADC plausible, scan loop running) once the I/O layer exists
- [ ] Outputs forced OFF before flashing and until self-test passes (hook `before_flash` exists in `main.rs`, drives nothing yet)
- [x] Software rollback fallback: NVS boot counter (3 unconfirmed boots, then switch to the previous slot); decision logic is host-tested in `well-core`, the slot switch is built but not exercised on a device
- [x] PC-side upload script `scripts/upload-ota.ps1` (parse-checked only, never run against a device)
- [x] `UpdateHooks` and `HealthSource` interfaces in `well-core`; the I/O layer implements them later (current `UpdateHooks` is a logging stand-in)
- [x] Shared HTTP server (`http.rs`); OTA registers its routes on it so other features can attach
- [x] `CONFIG_ESP_TASK_WDT_PANIC=y`: a hung app reboots, and rolls back while unconfirmed
- [ ] HTTP-aware soak: dropped for now, cannot be verified without running on a device; revisit on the dev board
- [ ] PC-side upload tool (small Rust CLI or script): compute SHA-256, POST to `/ota`, wait for reboot, poll `/ota/status` until `valid`
- [x] Produce the OTA image with `scripts/make-ota-image.ps1` (release build, `espflash save-image`, size check, SHA-256): `C:\bw\ota\buzsak_well.bin`, 1,060,736 bytes (16.2% of the 6.25 MB slot); header verified against the C++ image: magic 0xE9, ESP32, dio, 16 MB, 80 MHz (identical flash bytes), project `buzsak_well`, version 0.1.0. `sdkconfig.defaults` was switched to dio afterwards, so the next script run rebuilds ESP-IDF once (~17 min)
- [x] Commit a `secrets.rs.example` so a fresh clone builds (copy to `secrets.rs`)
- [x] Host-testable crate `crates/well-core` with `cargo test` (OTA policy and interfaces only; control logic stays out until its phase)
- [ ] Dev-board variant: separate `sdkconfig`/partition table for a smaller flash (the current config targets the 16 MB PLC)

### Code layout

- Workspace root: `crates/well-core` (host-testable) and `firmware/` (ESP32 build).
- `firmware/src/main.rs`: initialisation and orchestration only.
- `firmware/src/platform/`: `device.rs` (Wi-Fi, production timing) and `sim.rs` (QEMU Ethernet, short timing, pre-release images accepted), selected by the `qemu` feature; `link.rs` is the shared link-state view. `http.rs`: the shared HTTP server.
- `firmware/src/web/` (`init.rs`, `run.rs`): placeholders for the web page, not wired into any logic.
- `sim/`: QEMU harness (`Sim.psm1`, `sdkconfig.qemu`) and `tests/Run-OtaTests.ps1` (7 cases, no fault injection).
- `crates/well-core/src/interfaces/plc.rs`: the PLC's terminals and pins (`I0_x`, `Q0_x`, `S0_x_24v`, `R0_0`, I2C, RS-485, Ethernet SPI) as structs and constants; no production logic.
- `firmware/src/ota.rs` with `ota/`: `server.rs` (routes), `upload.rs` (streaming write, SHA-256, activation), `image.rs` (header vetting), `auth.rs`, `confirm.rs` (soak, confirm, rollback), `boot_guard.rs` (NVS boot counter), `error.rs`.
- `firmware/src/secrets.rs`: gitignored credentials.
- Dependencies: `anyhow`, `base64`, `esp-idf-svc`, `log`, `sha2`, `well-core`, `embuild`. The ADC will need `esp-idf-hal` with `adc-oneshot-legacy` (matches the Arduino `esp_adc_cal` path) when the I/O phase starts.

### Build notes

- Firmware: `cd firmware; . $env:USERPROFILE\export-esp.ps1; cargo build`. Host tests: `cargo test` at the repo root.
- Build output goes to `C:/bw` (`firmware/.cargo/config.toml` `target-dir`): esp-idf-sys refuses long output paths on Windows. The ELF is `C:\bw\xtensa-esp32-espidf\debug\buzsak_well`.
- `ESP_IDF_GLOB_PT_*` in `firmware/.cargo/config.toml` copies `partitions.csv` into the generated ESP-IDF project.
- No cargo `runner` is configured on purpose, so `cargo run` cannot flash anything.
- OTA image: `powershell -File scripts/make-ota-image.ps1` (release build takes ~17 min from scratch, incremental builds are much faster).

## Phase 2 — OTA failure testing (QEMU first, then dev board)

QEMU simulator (`sim/tests/Run-OtaTests.ps1`) passes the no-fault cases; fault injection is not built into the test run yet.

- [x] Good update confirmed and kept (QEMU)
- [x] Wrong-hash upload rejected (QEMU)
- [x] Not-newer and non-image uploads rejected (QEMU)
- [ ] QEMU: software reset (`esp_restart`) panics in early init after the first boot; harness power-cycles instead. Find out whether this is QEMU only (likely) before trusting reboot behaviour on a device
- [ ] Corrupt/truncated upload rejected, old image keeps running
- [ ] Image that panics on boot rolls back
- [ ] Image that boot-loops rolls back
- [ ] Image that hangs (watchdog) rolls back
- [ ] Image that runs but never confirms rolls back on next reset
- [ ] Power cut mid-upload leaves old image intact
- [ ] Power cut during first boot of new image rolls back
- [ ] Wi-Fi loss after update (self-test requires reachability) rolls back

## Phase 3 — Port firmware logic

- [ ] Hardware-free logic crate/module: selector decoding, hysteresis, refill, fault stop, with `cargo test`
- [ ] Calibration conversions (pressure bar, water level liters) with tests against C++ constants
- [ ] MCP23017 I2C expander driver and pin map (inputs `I0_x`, outputs `Q0_x`, 24 V mode pins)
- [ ] ADC reading on GPIO 34/35 (ADC1, 12-bit, 11 dB, eFuse calibration) matching `analogReadMilliVolts`; drive `AC7_EN`/`AC8_EN` LOW; validate against C++ readings on the real device
- [ ] Replicate boot-time pin states: Q outputs LOW, S mode pins set before outputs, GPIO 2/12/15 strapping safety
- [ ] Decide on pump safety gaps found in the original (manual mode ignores water level, no run-time limit / dry-run protection)
- [ ] Known C++ defect: leaving manual mode can leave the well pump running without level control (the original never re-evaluates the output); do not copy
- [ ] Scan loop at 50 ms; fail-safe on sensor loss / invalid selector
- [ ] Status LED behaviour (Wi-Fi LED, switch LED blink)
- [ ] HTTP API parity: `/api/state`, `POST /api/control` (refill only), dashboard `/`
- [ ] Wi-Fi watchdog / reconnect
- [ ] Task watchdog configured; pumps OFF on panic/reset

## Phase 4 — Deployment

- [ ] Primary path: push the Rust image through the existing ElegantOTA `/update`; confirm it boots, self-tests and confirms (C++ firmware stays in the other slot as fallback)
- [ ] Fallback path: on-site USB session (checklist: backup, read partition table, flash, verify)
- [ ] Full flash backup of the existing device before anything else, if physical access is ever available: `esptool read_flash 0 0x1000000 backup.bin`
- [ ] Read existing partition table and record flash ID/size when on site
- [ ] Verify on LAN: version, sensor values, selector behaviour, pump truth table
- [ ] Perform a second OTA (Rust to Rust) on the installed device and confirm it
- [ ] Document restore procedure (re-flash `backup.bin`, or OTA the C++ `firmware.bin` back)

## Open questions

- Rotate the Wi-Fi/OTA credentials that are committed in the original repo? Is the GitHub remote public?
- Does the companion app / Flask server depend on exact JSON field names?
- Add auth to `POST /api/control` in the Rust version?
- Is diagnostics (`/diag`, CSV logging) needed in the Rust version?
- How long should the post-update soak window be before marking valid?
- Ethernet port: use it, or Wi-Fi only?
- Should manual mode (left) get a low-level cutoff or run-time limit?

## Later / optional

- Ed25519-signed images
- HTTPS or pull-based updates from a server
- Secure boot / flash encryption (irreversible; evaluate carefully)
- Remote logging
