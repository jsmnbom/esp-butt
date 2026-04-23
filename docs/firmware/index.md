# Firmware

## Flashing Pre-built Firmware

Pre-built firmware binaries are published with each [GitHub release](https://github.com/jsmnbom/esp-butt/releases). Download the latest `.bin` file from there.

### Flashing with espflash

Install [espflash](https://github.com/esp-rs/espflash):

```sh
cargo install espflash --locked
```

Then flash the downloaded binary:

```sh
espflash flash --chip esp32s3 esp-butt.bin
```

## Building from Source

### Prerequisites

Install the Espressif toolchain:

```sh
cargo install espup --locked
espup -t esp32s3 -s --toolchain-version 1.93.0.0
```

The toolchain uses the `esp` channel (Espressif's Xtensa fork). Target triple: `xtensa-esp32s3-espidf`.

### Build and Flash

Switch the build target to `xtensa-esp32s3-espidf` in `.cargo/config.toml` (and `.vscode/settings.json` for rust-analyzer):

```sh
cargo build          # debug build
cargo run            # build, flash, and open serial monitor
```

`cargo run` flashes via `espflash flash --monitor` as configured in `.cargo/config.toml`.

### Desktop Mock (no device needed)

Switch the target to `x86_64-unknown-linux-gnu`:

```sh
cargo run
```

Requires the [kitty](https://sw.kovidgoyal.net/kitty/) terminal emulator. The display output renders in a separate kitty window. Use mouse to interact with sliders and keyboard arrows + Enter to simulate button presses.

---

## Architecture

- `src/app/` — platform-independent state machine and UI screens
- `src/ble/` — NimBLE BLE scanning and GATT client (ESP only)
- `src/buttplug/` — Buttplug server/client, device config, deferred connection approval
- `src/hw/` — display (I2C OLED), encoder, sliders, ticker; split into `hw/esp/` and `hw/mock/`
- `src/utils/` — FreeRTOS task spawning, PSRAM heap, logging

CPU affinity: BLE on Core 0, app logic on Core 1.
