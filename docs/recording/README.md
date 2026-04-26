# Recording artifacts

This directory holds the raw session recording used to build the hero animation.

## How to populate

1. Run the firmware with recording enabled:
   ```bash
   ESP_BUTT_RECORD_SESSION=1 cargo run
   ```
2. Copy the recording files here:
   ```bash
   cp /tmp/esp-butt-session.ndjson docs/recording/session.ndjson
   cp -r /tmp/esp-butt-record-frames/ docs/recording/frames/
   ```
3. Build the animation assets:
   ```bash
   ./docs_build.py animation
   ```
   This produces `docs/public/models/screen-atlas.png` and `docs/public/models/recording.json`.

## Files

- `session.ndjson` — raw event log (NDJSON, one event per line)
- `frames/` — OLED frame PNG snapshots referenced by `session.ndjson`
