# esp-butt

Bluetooth intimate hardware (sex toys, etc) controller powered by [buttplug.io](https://buttplug.io/).

Heavily work in progress!

This file will eventually explain things and stuffs but for now have this:

## Quick render of PCB

![pcb](pcb/render.png)


# Compilation

## Real hardware (esp32s3)

Change target to `xtensa-esp32s3-espidf` in `.cargo/config.toml` (and `.vscode/settings.json` if you want).

Install toolchain using:
```
> cargo install espup --locked
> espup -t esp32s3 -s --toolchain-version 1.93.0.0
```

Then you can build using:
```
> cargo build
```

Or build and flash using:
```
> cargo run
```

## Mock/Testing without real hardware

I've added support for quick testing of the code on any (most?) linux machines. To do this, change the target to `x86_64-unknown-linux-gnu` in `.cargo/config.toml` (and `.vscode/settings.json` if you want).

You will want the `kitty` terminal emulator. It is used to render the display output as an image and also show sliders as mouse interactable elements. 

Then simply run:
```
> cargo run
```

The logging output will be in the terminal you ran the command in, and the display output will be in a separate kitty window. You can interact with the sliders in the kitty window to simulate input from the device, and use keyboard (arrow key up + down + enter) to simulate button presses.




# License and thanks
Code
CAD
PCB

Thanks to @qdot and the rest of the Buttplug.io contributors doing all the hard work of reverse engineering and protocol design, and also for being generally awesome.
