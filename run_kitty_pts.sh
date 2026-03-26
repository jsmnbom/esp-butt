#!/usr/bin/env bash
set -euo pipefail

if ! command -v kitty >/dev/null 2>&1; then
  echo "kitty not found in PATH" >&2
  exit 1
fi

APP_COLS=40

project_dir="${1:-$PWD}"
project_dir="$(cd "$project_dir" && pwd)"
# tty_file="$(mktemp /tmp/esp-butt-log-tty.XXXXXX)"
socket="unix:/tmp/esp-butt-kitty-${USER:-user}-$$.sock"

cleanup() {
  rm -f "${socket#unix:}"
}
trap cleanup EXIT

kitty --detach --single-instance=no -o allow_remote_control=socket-only \
  --listen-on "$socket" --directory "$project_dir"

sleep 0.25

if ! kitty @ --to "$socket" ls >/dev/null 2>&1; then
  echo "failed to connect to kitty remote control socket: $socket" >&2
  exit 1
fi

kitty @ --to "$socket" send-text "tty\n"
sleep 0.1
tty=$(kitty @ --to "$socket" get-text --extent last_cmd_output)

# Extract /dev/pts/X from captured command output.
tty=$(printf '%s\n' "$tty" | grep -Eo '/dev/pts/[0-9]+' | tail -n1 || true)
if [[ -z "$tty" ]]; then
  echo "failed to determine log tty" >&2
  exit 1
fi

# Ensure splits layout so vsplit is left/right.
kitty @ --to "$socket" goto-layout splits

kitty @ --to "$socket" launch --location vsplit --cwd "$project_dir" --title "esp-butt app"

columns=$(kitty @ --to "$socket" ls --match "title:\"esp-butt app\"" | jq ".[0].tabs[0].windows[0].columns")

delta=$((APP_COLS - columns))
echo "current log pane columns: $columns, target: $APP_COLS, delta: $delta"
if [[ $delta -ne 0 ]]; then
  kitty @ --to "$socket" resize-window --match "title:\"esp-butt app\"" --axis horizontal --increment "$delta"
fi

kitty @ --to "$socket" send-text --match "title:\"esp-butt app\"" "cargo run 2>'$tty'; echo '--- exited ---'; read\n"

# Right pane: run app, send stderr to the left pane tty.
# kitty @ --to "$socket" launch \
#   --location vsplit \
#   --cwd "$project_dir" \
#   --title "esp-butt app" \
#   sh -lc "target_cols=\${APP_COLS:-20}; delta=\$((target_cols - COLUMNS)); kitty @ --to '$socket' resize-window --self --axis horizontal --increment \"\$delta\"; cargo run 2>'$tty'; echo '--- exited ---'; read"

