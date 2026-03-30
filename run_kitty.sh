#!/usr/bin/env bash
set -euo pipefail

TARGET="$1"

if ! command -v kitty >/dev/null 2>&1; then
  echo "kitty not found in PATH" >&2
  exit 1
fi

ORIG_TTY=$(tty)

kitty --single-instance=no \
  -o remember_window_size=no \
  -o initial_window_width=32c \
  -o initial_window_height=40c \
  env TARGET="$TARGET" ORIG_TTY="$ORIG_TTY" \
  bash -c '"$TARGET" 2>"$ORIG_TTY"; echo; read -rp "--- exited (press Enter to close) ---"'
