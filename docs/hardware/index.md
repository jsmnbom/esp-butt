# Hardware Overview

esp-butt is a handheld Bluetooth controller built around the ESP32-S3. It connects to BLE intimate devices running the [Buttplug protocol](https://buttplug.io/) and lets you control them through a small physical interface: an OLED display, a rotary encoder, and three analog sliders.

## Components

| Component | Details |
|---|---|
| **MCU** | ESP32-S3 (Xtensa dual-core, 240 MHz, Wi-Fi + BLE) |
| **Display** | 128×64 OLED, I²C (SH1106) |
| **Encoder** | Rotary encoder with push button — menu navigation and selection |
| **Sliders** × 2 | 10 kΩ linear potentiometers — analog control of device features (intensity, oscillation, etc.) |
| **Power** | LiPo battery |

## Sections

- **[Bill of Materials](/hardware/bom)** — Full parts list with quantities and purchase links.
- **[Schematic](/hardware/schematic)** — Circuit diagram for the controller board.
- **[PCB](/hardware/pcb)** — KiCad PCB layout and 3D preview.
- **[3D Models](/hardware/models)** — Printable case, knobs, and power switch cap.
