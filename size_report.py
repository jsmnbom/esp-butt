#!/usr/bin/env python3
"""
Demangle and display esp_idf_size JSON2 output as a readable table.

Usage:
    python3 size_report.py [SIZE_JSON] [--top N] [--filter PATTERN] [--sort {total,flash,iram,diram,data}]

Defaults to size.json in the current directory, top 50 entries sorted by total size.

Fully vibe-coded, no gaurantees on code quality or stability. Use at your own risk. Contributions welcome.
"""

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

from rich import box
from rich.console import Console
from rich.table import Table
from rich.text import Text

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

CARGO_BIN = Path.home() / ".local" / "share" / "cargo" / "bin" / "rustfilt"

def find_rustfilt() -> str | None:
    found = shutil.which("rustfilt")
    if found:
        return found
    if CARGO_BIN.exists():
        return str(CARGO_BIN)
    return None


def extract_raw_symbol(key: str) -> str:
    """Pull the mangled symbol out of a size.json key.

    Key format:  <archive_path>:<object_file>:<section>.<mangled_symbol>
    e.g.         .../libbuttplug_server.rlib:file.rcgu.o:.text._ZN15foo...E
    """
    # Split only on the last colon-separated segment that isn't a path
    # (paths won't contain colons on Linux, so a simple split works)
    parts = key.split(":")
    section_sym = parts[-1]  # ".text._ZNfooE" or ".rodata._ZN…" etc.

    # Prefer a proper mangled symbol (_ZN… legacy or _R… v0)
    m = re.search(r"(_(?:ZN|R)\S+)", section_sym)
    if m:
        return m.group(1)

    # Fallback: strip leading section prefix (.text., .rodata., …) if present
    m2 = re.match(r"\.\w+\.(.*)", section_sym)
    if m2:
        return m2.group(1)

    return section_sym


def demangle_batch(rustfilt: str, symbols: list[str]) -> list[str]:
    """Demangle many symbols in one rustfilt call (very fast)."""
    if not symbols:
        return []
    result = subprocess.run(
        [rustfilt],
        input="\n".join(symbols),
        capture_output=True,
        text=True,
    )
    lines = result.stdout.splitlines()
    # Pad in case rustfilt drops blank lines
    while len(lines) < len(symbols):
        lines.append("")
    return lines


def shorten_obj(obj_path: str) -> str:
    """Return a short human-readable label for the CGU / object file."""
    # e.g. "buttplug_server.1bpou0oabx1zi686vjx3qkdx4.09266vr.rcgu.o"
    #  --> "buttplug_server … rcgu"
    stem = Path(obj_path).stem          # strip .o
    parts = stem.split(".")
    if len(parts) >= 2:
        crate = parts[0]
        cgu   = parts[-1]               # "rcgu" or similar
        return f"{crate} ({cgu})"
    return stem


def fmt_bytes(n: int) -> str:
    if n == 0:
        return "-"
    if n < 1024:
        return f"{n} B"
    if n < 1024 * 1024:
        return f"{n / 1024:.1f} KiB"
    return f"{n / 1024 / 1024:.2f} MiB"


def fmt_plain(n: int) -> str:
    return f"{n:,}" if n else "-"


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("size_json", nargs="?", default="size.json",
                        help="Path to the JSON2 size file (default: size.json)")
    parser.add_argument("--top", "-n", type=int, default=50, metavar="N",
                        help="Number of rows to show (default: 50, 0 = all)")
    parser.add_argument("--filter", "-f", default="", metavar="PATTERN",
                        help="Case-insensitive substring filter on demangled name")
    parser.add_argument("--sort", "-s",
                        choices=["total", "flash", "iram", "diram", "data"],
                        default="total",
                        help="Column to sort by (default: total)")
    parser.add_argument("--bytes", action="store_true",
                        help="Show raw byte counts instead of human-readable sizes")
    parser.add_argument("--wide", "-w", action="store_true",
                        help="Show full demangled names (default: strip leading crate:: prefix)")
    parser.add_argument("--width", type=int, default=None, metavar="COLS",
                        help="Override terminal width for the table")
    args = parser.parse_args()

    console = Console(width=args.width)

    # --- locate rustfilt ---
    rustfilt = find_rustfilt()
    if rustfilt is None:
        console.print("[red]rustfilt not found.[/red] Install it with:\n  cargo install rustfilt")
        sys.exit(1)

    # --- load data ---
    json_path = Path(args.size_json)
    if not json_path.exists():
        console.print(f"[red]File not found:[/red] {json_path}")
        sys.exit(1)

    with open(json_path) as fh:
        data: dict = json.load(fh)

    console.print(f"[dim]Loaded {len(data):,} entries from {json_path}[/dim]")

    # --- parse entries ---
    entries = []
    for key, val in data.items():
        total_size = val.get("size", 0)
        if total_size == 0:
            continue

        mem   = val.get("memory_types", {})
        flash = mem.get("Flash Code", {}).get("size", 0)
        iram  = mem.get("IRAM",       {}).get("size", 0)
        diram = mem.get("DIRAM",      {}).get("size", 0)
        fdata = mem.get("Flash Data", {}).get("size", 0)

        key_parts = key.split(":")
        archive = Path(key_parts[0]).name if key_parts else ""
        obj     = key_parts[1]            if len(key_parts) > 1 else ""

        entries.append({
            "raw_sym":    extract_raw_symbol(key),
            "archive":    archive,
            "obj":        shorten_obj(obj),
            "total":      total_size,
            "flash":      flash,
            "iram":       iram,
            "diram":      diram,
            "flash_data": fdata,
        })

    console.print(f"[dim]Non-zero entries: {len(entries):,}[/dim]\n")

    # --- demangle in one batch ---
    with console.status("[bold cyan]Demangling symbols…[/bold cyan]"):
        demangled = demangle_batch(rustfilt, [e["raw_sym"] for e in entries])

    for entry, name in zip(entries, demangled):
        full_name = name.strip() or entry["raw_sym"]
        if not args.wide:
            # Strip the leading crate path segment to save horizontal space
            # e.g. "buttplug_server::device::foo" -> "device::foo"
            parts = full_name.split("::", 1)
            display_name = parts[1] if len(parts) == 2 and not full_name.startswith("<") else full_name
        else:
            display_name = full_name
        entry["name"] = display_name
        entry["full_name"] = full_name

    # --- filter ---
    filt = args.filter.lower()
    if filt:
        entries = [e for e in entries if filt in e.get("full_name", e["name"]).lower()]
        console.print(f"[dim]After filter '{args.filter}': {len(entries):,} entries[/dim]\n")

    # --- sort ---
    sort_key = {
        "total": "total",
        "flash": "flash",
        "iram":  "iram",
        "diram": "diram",
        "data":  "flash_data",
    }[args.sort]
    entries.sort(key=lambda e: e[sort_key], reverse=True)

    # --- truncate ---
    top_n   = args.top if args.top > 0 else len(entries)
    visible = entries[:top_n]

    # --- build table ---
    fmt = fmt_bytes if not args.bytes else fmt_plain

    table = Table(
        title=f"Symbol sizes — top {len(visible):,} of {len(entries):,} (sorted by {args.sort})",
        box=box.SIMPLE_HEAD,
        show_header=True,
        header_style="bold cyan",
        highlight=True,
    )
    table.add_column("#",          style="dim",         justify="right", no_wrap=True)
    table.add_column("Function",                        min_width=40, max_width=90, overflow="fold")
    table.add_column("Total",      style="bold yellow", justify="right", no_wrap=True)
    table.add_column("Flash Code",                      justify="right", no_wrap=True)
    table.add_column("IRAM",                            justify="right", no_wrap=True)
    table.add_column("DIRAM",                           justify="right", no_wrap=True)
    table.add_column("Flash Data",                      justify="right", no_wrap=True)
    table.add_column("Object",      style="dim",        max_width=35,   overflow="ellipsis", no_wrap=True)

    for i, e in enumerate(visible, 1):
        name      = e["name"]
        full_name = e.get("full_name", name)
        # Highlight closures / async state machines in italic
        if "{{closure}}" in full_name or "{{impl}}" in full_name or "poll" in full_name.lower():
            name_text = Text(name, style="italic")
        else:
            name_text = Text(name)

        table.add_row(
            str(i),
            name_text,
            fmt(e["total"]),
            fmt(e["flash"]),
            fmt(e["iram"]),
            fmt(e["diram"]),
            fmt(e["flash_data"]),
            e["obj"],
        )

    console.print(table)

    # --- totals footer ---
    tot_total = sum(e["total"]      for e in entries)
    tot_flash = sum(e["flash"]      for e in entries)
    tot_iram  = sum(e["iram"]       for e in entries)
    tot_diram = sum(e["diram"]      for e in entries)
    tot_fdata = sum(e["flash_data"] for e in entries)

    console.print(
        f"[bold]Totals (all {len(entries):,} non-zero symbols):[/bold]  "
        f"Total [yellow]{fmt(tot_total)}[/yellow]  "
        f"Flash Code {fmt(tot_flash)}  "
        f"IRAM {fmt(tot_iram)}  "
        f"DIRAM {fmt(tot_diram)}  "
        f"Flash Data {fmt(tot_fdata)}"
    )
    console.print(
        f"\n[dim]Tip: --top 0 (all rows)  --filter <name>  "
        f"--sort flash/iram/diram/data  --wide (full names)  --width COLS[/dim]"
    )


if __name__ == "__main__":
    main()
