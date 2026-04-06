# esp-butt Project Guidelines

## What This Is

ESP32-S3 embedded Rust firmware for a physical Buttplug.io controller: OLED display + rotary encoder + sliders that discover and control Bluetooth LE devices via the Buttplug protocol.
 
## Copilot guidance

- **Do not use release builds:** When suggesting commands, edits, or CI changes, never use `--release` or otherwise recommend building or flashing release artifacts unless explicitly asked. Default to debug/desktop builds (`cargo build`, non-release runners) for development, testing, and examples.

## Build and Run

### Prerequisites
```bash
cargo install espup --locked
espup -t esp32s3 -s --toolchain-version 1.93.0.0
```
Toolchain: `esp` channel (Espressif's Xtensa fork). Target triple: `xtensa-esp32s3-espidf`. Custom tool location configured via `mise.toml` → `ESP_IDF_TOOLS_INSTALL_DIR`.

### Build
```bash
# ESP32-S3 firmware
cargo build
# Output: target/xtensa-esp32s3-espidf/debug/esp-butt

# Desktop mock UI (no device needed)
cargo build
```

### Switching build target
The active target is set in two places — keep them in sync:
- `.cargo/config.toml` — `[build] target = ...`
- `.vscode/settings.json` — `"rust-analyzer.cargo.target": "..."`

ESP32-S3 target: `xtensa-esp32s3-espidf`. Desktop: omit / use host triple.

### Tests
No `cargo test`. Testing uses the desktop mock mode (`cargo build` without `espidf` target) which runs a Ratatui TUI with the same app logic. Same `src/app/` code path; hardware swapped via `cfg(target_os = "espidf")`.

### Size analysis
```bash
./size_report.py [path/to/linker.map]
```
Uses `#!/usr/bin/env -S uv run --script` — dependencies are declared inline in the script, no manual install needed.

### Flash and monitor
```bash
cargo run   # flashes via espflash and opens serial monitor
```
Configured in `.cargo/config.toml`: ESP target runner is `espflash flash --monitor`; desktop target runner is `./run_kitty.sh`.

## Architecture

### Module layout
- `src/app/` — state machine, UI screens, event loop. **Platform-independent.**
- `src/ble/` — NimBLE wrapper: scanning, GAP/GATT client. **ESP only.**
- `src/buttplug/` — Buttplug server + in-process client, device config loading, ESP async manager, deferred connection approval. **Mostly ESP, some shared logic.**
- `src/hw/` — display (I2C OLED), encoder (interrupts), sliders (ADC), ticker. Split: `hw/esp/` and `hw/mock/`.
- `src/utils/` — FreeRTOS task spawning, stream helpers, PSRAM heap, logging.
- `src/img.rs` — images baked in at build time by `build.rs`.

### App state machine
`AppState`: `Idle → DeviceList → DeviceControl`. Single async event loop in `app.rs` selects over merged streams: Buttplug events, hardware input (`AppEvent::Navigation`, `AppEvent::Slider`), timer ticks, and UI draw events.

### CPU core affinity
Per `sdkconfig.defaults` and `task-layout.md`:
- **Core 0 (Pro)**: NimBLE BLE host task
- **Core 1 (App)**: main task (app logic, display, coordination)

`utils::task::spawn()` takes an explicit `core` argument. Keep BLE work on Core 0 and app logic on Core 1.

### Async runtime
Tokio is used, but **async tasks are FreeRTOS tasks** on the device. `EspAsyncManager` (`src/buttplug/async_manager.rs`) maps Buttplug's span-based task creation to `hal::task::create()`. Stack sizes are hardcoded per task type:
- Device manager: 20 KB
- Device comm: 8 KB

Do not use `tokio::spawn` directly on ESP — use `utils::task::spawn()` or the patterns established in `buttplug/async_manager.rs`.

Stack sizes are determined empirically: set high, run, read the FreeRTOS high-water mark from the task report output (`src/utils/report.rs`), then set to observed peak + headroom. Do not guess stack sizes.

### Memory
- PSRAM (external RAM, octane 80 MHz) for large allocations (device config database, etc.) via `utils/heap.rs` external allocator.
- Buttplug device config is compiled into flash as compressed binary by `build.rs` and decompressed into PSRAM at startup.
- Images are compiled as 1-bit monochrome binary by `build.rs`; no runtime decompression.

## Conventions

### Conditional compilation
All hardware-specific code is gated on `#[cfg(target_os = "espidf")]`. Desktop mock equivalents live in `hw/mock/`. Keep this boundary clean — `src/app/` must remain platform-independent.

### BLE device discovery flow
BLE scan finds device → `DiscoveredDevice` event sent to app → user selects in `DeviceList` screen → app notifies via `Notify` (in `DeferredCommunicationManager`) → BLE connects. Do not bypass the deferred approval step.

`DeferredCommunicationManager` exists because Buttplug greedily connects to every discovered device. Our wrapper holds devices in a pending state until the user explicitly selects one.

### Slider → device output mapping
Slider index mapped to `ClientDeviceFeature` via `LiteMap` in `device_control.rs`. Raw 12-bit ADC (0–4095) is scaled to the feature's step range.

### BLE stack
NimBLE only (not Bluedroid). MTU 23 bytes, max 2 simultaneous connections. Security: basic, no MITM/bonding. See `sdkconfig.defaults` for all NimBLE knobs.

### Panic strategy
`panic = "abort"` in all profiles. No unwinding.

### Buttplug library forks
The `buttplug_*` crates are forked and developed locally in parallel with this firmware until changes are upstreamed. They are referenced as local path dependencies in `Cargo.toml`. Do not update them from crates.io.

## Non-firmware contents

- `cad/` — CadQuery notebooks and scripts generating the 3D-printed case and knobs. Uses `ocp-cad-viewer` for previewing models.
- `pcb/` — KiCad 8 schematic and PCB layout for the controller board.

### Python / CAD environment
Python tooling uses `uv` + a `.venv`. The `bernhard-42.ocp-cad-viewer` VS Code extension currently breaks when a venv is created by `uv` (it detects the `uv` marker in `pyvenv.cfg`). Workaround: after creating the venv, manually edit `.venv/pyvenv.cfg` and remove the `uv`-specific lines so the extension treats it as a plain venv.
