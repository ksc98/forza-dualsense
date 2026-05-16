# Forza DualSense

Adaptive trigger feedback for *Forza Horizon* on the Sony DualSense controller.

Forza DualSense is a small Windows utility that consumes the game's "Data Out" UDP telemetry stream and drives the DualSense adaptive triggers in response. The brake trigger applies progressive resistance proportional to brake input, with a configurable wall at full press. The throttle trigger applies a tunable counter-force and pulses at the rev limiter. ABS engagement, gear changes, and the handbrake produce discrete haptic events. The utility writes only the trigger fields of the DualSense output report, so Steam Input (or any other process) retains control of the rumble motors.

## Features

- Single self-contained executable; no external runtime dependencies.
- Native desktop UI and an embedded web UI at `http://127.0.0.1:5301/`, both backed by the same live state and settings.
- USB and Bluetooth controller support, with automatic reconnect and the correct CRC32 framing required for Bluetooth output reports.
- 250 Hz HID update loop driven by Forza's telemetry stream (UDP port 5300 by default).
- Launch-time update check against GitHub Releases, configurable in settings or via a command-line flag.

## Install — one PowerShell command

```powershell
iwr -useb https://raw.githubusercontent.com/ksc98/forza-dualsense/main/install.ps1 | iex
```

The installer downloads the latest release binary, places it under `%LOCALAPPDATA%\Programs\ForzaDualSense\`, and adds a Start Menu shortcut named **Forza DualSense**. If no release is published yet, the installer falls back to building from source; the Rust toolchain is installed silently via `rustup` if it is not already present.

## Uninstall — one PowerShell command

```powershell
iwr -useb https://raw.githubusercontent.com/ksc98/forza-dualsense/main/uninstall.ps1 | iex
```

The uninstaller stops any running instance, removes the installed binary at `%LOCALAPPDATA%\Programs\ForzaDualSense\`, deletes the Start Menu shortcut, and removes the persisted settings directory at `%APPDATA%\forza-dualsense\`. No registry changes are made by either script.

## In-game setup

Forza Horizon → **Settings → HUD and Gameplay** → scroll to the bottom:

| Setting | Value |
|---|---|
| Data Out | **ON** |
| Data Out IP Address | **127.0.0.1** |
| Data Out IP Port | **5300** |

Connect the DualSense via USB or pair it over Bluetooth. The application detects the controller automatically and will reconnect on disconnection.

## Command-line flags

```text
forza-dualsense [--host 127.0.0.1] [--port 5300] [--web-port 5301]
                [--no-gui] [--no-web] [--no-update] [--debug]
```

- `--no-gui` — run headless, web UI only.
- `--no-web` — disable the embedded web server.
- `--no-update` — skip the launch-time update check (also configurable in settings).
- `--debug` — verbose tracing.

## Settings

All tunable parameters — pedal force curves, deadzones, ABS thresholds, gear-shift pulse parameters, rev-limit thresholds, and trigger wall zones — are exposed in both user interfaces and persisted to disk:

- Windows: `%APPDATA%\forza-dualsense\settings.json`
- macOS: `~/Library/Application Support/forza-dualsense/settings.json`
- Linux: `~/.config/forza-dualsense/settings.json`

Changes made in one interface take effect on the next HID tick and are reflected in the other.

## Auto-update

At startup the application queries the GitHub Releases API for the most recent published version. When a newer release is available, the matching archive is downloaded, the binary is replaced in place, and the user is prompted to restart. The check is performed on a background task, runs once per launch, and can be disabled in **Settings → System → Check for updates on launch** or with the `--no-update` flag.

## Building from source

```bash
git clone https://github.com/ksc98/forza-dualsense
cd forza-dualsense
cargo build --release
```

A stable Rust toolchain is required. On Windows, `hidapi` uses the system `Hid.dll` backend, requiring no additional runtime files. On Linux, `hidapi-rs` statically links libusb.

## Architecture

```
┌─────────────┐   UDP 5300    ┌────────────────┐   HID 250 Hz  ┌─────────────┐
│ Forza       │ ────────────► │  forza-        │ ────────────► │  DualSense  │
│  Data Out   │   324 bytes   │  dualsense     │  trigger bits │  USB / BT   │
└─────────────┘               │                │  only         └─────────────┘
                              │  Tokio runtime │
                              │  ▲    ▲    ▲   │
                              │  │    │    └── egui native window
                              │  │    └─────── axum web UI (127.0.0.1:5301)
                              │  └──────────── shared AppState
                              └────────────────┘
```

A single Tokio runtime hosts three cooperating tasks: a UDP listener that drains the operating-system receive queue on each iteration, a dedicated HID thread that maintains the DualSense connection at 250 Hz and reconnects on disconnection, and an `axum` HTTP and WebSocket server that streams state to the web UI at 30 Hz. The native UI polls the same shared `AppState`. Because all three tasks observe the same state, a configuration change made through either UI is applied on the next HID tick.

## Project layout

```
.
├── Cargo.toml
├── install.ps1                 ← one-command Windows installer
├── build.rs                    ← Windows resource (icon, version info)
├── assets/
│   ├── icon.ico                ← application icon
│   └── web/index.html          ← embedded web UI
└── src/
    ├── main.rs                 ← runtime + GUI bootstrap
    ├── settings.rs             ← every tunable, persisted to disk
    ├── telemetry.rs            ← 324-byte Forza packet parser
    ├── udp.rs                  ← async UDP listener
    ├── triggers.rs             ← effect primitives (raw HID frames)
    ├── controller.rs           ← per-tick effect chain (L2/R2 priority)
    ├── hid.rs                  ← DualSense HID layer (USB + BT + CRC32)
    ├── hid_task.rs             ← reconnect loop, 250 Hz tick
    ├── state.rs                ← shared AppState + JSON snapshot
    ├── gui.rs                  ← native egui UI
    ├── update.rs               ← self-update against GitHub Releases
    └── web.rs                  ← axum HTTP + WebSocket server
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual-licensed as above, without any additional terms or conditions.

*Forza Horizon* and *DualSense* are trademarks of their respective owners; this project is an independent, unaffiliated work that consumes a documented public telemetry stream.
