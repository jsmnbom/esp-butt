#!/usr/bin/env bash
# Export schematic and PCB SVGs for the docs site.
#
# Uses KICAD_CONFIG_HOME to point at the project-local kicad config dir so
# that the Witch Hazel color theme is self-contained and doesn't depend on
# anything in ~/.config/kicad.
#
# Override the config dir with:
#   KICAD_CONFIG_HOME=/path/to/config ./export_svg.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/esp-butt"
OUT_DIR="$SCRIPT_DIR/../docs/svg"

# Point kicad-cli at our bundled config unless the caller already set it.
export KICAD_CONFIG_HOME="${KICAD_CONFIG_HOME:-$SCRIPT_DIR/kicad-config}"

# Theme name (filename stem) for the bundled Witch Hazel dark theme.
# kicad-cli resolves this against <KICAD_CONFIG_HOME>/10.0/colors/<name>.json
DARK_THEME="witchhazel"

SCH="$PROJECT_DIR/esp-butt.kicad_sch"
PCB="$PROJECT_DIR/esp-butt.kicad_pcb"

mkdir -p \
  "$OUT_DIR/schematic" \
  "$OUT_DIR/front" \
  "$OUT_DIR/back"

# Crop an SVG to its drawing bounds using Inkscape.
# Usage: crop_svg <file>
crop_svg() {
  local file="$1"
  local tmp
  tmp="$(mktemp --suffix=.svg)"
  inkscape --pipe --export-type=svg --export-area-drawing --export-plain-svg \
    < "$file" > "$tmp" 2>/dev/null
  mv "$tmp" "$file"
}

echo "Using KICAD_CONFIG_HOME=$KICAD_CONFIG_HOME"
echo "Output: $OUT_DIR"
echo

# ---------------------------------------------------------------------------
# Schematic
# ---------------------------------------------------------------------------
echo "==> Schematic (light)..."
kicad-cli sch export svg \
  --no-background-color \
  --exclude-drawing-sheet \
  --output "$OUT_DIR/schematic/" \
  "$SCH"
mv "$OUT_DIR/schematic/esp-butt.svg" "$OUT_DIR/schematic/light.svg"
crop_svg "$OUT_DIR/schematic/light.svg"

echo "==> Schematic (dark / Witch Hazel)..."
kicad-cli sch export svg \
  --no-background-color \
  --exclude-drawing-sheet \
  --theme "$DARK_THEME" \
  --output "$OUT_DIR/schematic/" \
  "$SCH"
mv "$OUT_DIR/schematic/esp-butt.svg" "$OUT_DIR/schematic/dark.svg"
crop_svg "$OUT_DIR/schematic/dark.svg"

# ---------------------------------------------------------------------------
# PCB – front
# ---------------------------------------------------------------------------
FRONT_LAYERS="F.Cu,F.Mask,F.Silkscreen,Edge.Cuts"

echo "==> PCB front (light)..."
kicad-cli pcb export svg \
  --layers "$FRONT_LAYERS" \
  --mode-single \
  --fit-page-to-board \
  --exclude-drawing-sheet \
  --output "$OUT_DIR/front/light.svg" \
  "$PCB"
crop_svg "$OUT_DIR/front/light.svg"

echo "==> PCB front (dark / Witch Hazel)..."
kicad-cli pcb export svg \
  --layers "$FRONT_LAYERS" \
  --mode-single \
  --fit-page-to-board \
  --exclude-drawing-sheet \
  --theme "$DARK_THEME" \
  --output "$OUT_DIR/front/dark.svg" \
  "$PCB"
crop_svg "$OUT_DIR/front/dark.svg"

# ---------------------------------------------------------------------------
# PCB – back
# ---------------------------------------------------------------------------
BACK_LAYERS="B.Cu,B.Mask,B.Silkscreen,Edge.Cuts"

echo "==> PCB back (light)..."
kicad-cli pcb export svg \
  --layers "$BACK_LAYERS" \
  --mode-single \
  --fit-page-to-board \
  --mirror \
  --exclude-drawing-sheet \
  --output "$OUT_DIR/back/light.svg" \
  "$PCB"
crop_svg "$OUT_DIR/back/light.svg"

echo "==> PCB back (dark / Witch Hazel)..."
kicad-cli pcb export svg \
  --layers "$BACK_LAYERS" \
  --mode-single \
  --fit-page-to-board \
  --mirror \
  --exclude-drawing-sheet \
  --theme "$DARK_THEME" \
  --output "$OUT_DIR/back/dark.svg" \
  "$PCB"
crop_svg "$OUT_DIR/back/dark.svg"

echo
echo "Done. SVGs written to $OUT_DIR/"
