#!/usr/bin/env -S uv run --script

# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "esp_idf_size",
#     "rich",
# ]
# ///
"""
Demangle and display firmware symbol sizes using esp_idf_size.

Usage:
    python3 size_report.py [LINKER_MAP] [--top N] [--filter PATTERN] [--sort {total,flash,iram,diram,data}]

Defaults to target/xtensa-esp32s3-espidf/debug/linker.map, top 50 entries sorted by total size.
The ELF file is auto-detected from the same directory as the map file.

Fully vibe-coded, no gaurantees on code quality or stability. Use at your own risk. Contributions welcome.
"""

import argparse
import csv
import re
import shutil
import subprocess
import sys
import dataclasses
from pathlib import Path

from rich import box
from rich.console import Console, RenderableType
from rich.table import Table
from rich.text import Text

try:
    from esp_idf_size import mapfile as esp_mapfile
    from esp_idf_size import memorymap
    from esp_idf_size.elf import Elf as ElfParser

    try:
        from esp_idf_size.elf import PT_LOAD  # type: ignore[attr-defined]
    except ImportError:
        PT_LOAD = 1  # standard ELF constant

    _HAS_ESP_IDF_SIZE = True
except ImportError:
    _HAS_ESP_IDF_SIZE = False
    PT_LOAD = 1

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

DEFAULT_MAP = "target/xtensa-esp32s3-espidf/debug/linker.map"

# Memory type names as used by esp_idf_size -> SymbolEntry field name
MEM_TYPE_FIELD: dict[str, str] = {
    "Flash Code": "flash",
    "IRAM": "iram",
    "DIRAM": "diram",
    "Flash Data": "flash_data",
}

SORT_FIELD = {
    "total": "total",
    "flash": "flash",
    "iram": "iram",
    "diram": "diram",
    "data": "flash_data",
}


def find_rustfilt() -> str | None:
    found = shutil.which("rustfilt")
    if found:
        return found
    import os

    cargo_home = os.environ.get("CARGO_HOME")
    candidates = []
    if cargo_home:
        candidates.append(Path(cargo_home) / "bin" / "rustfilt")
    candidates.append(Path.home() / ".local" / "share" / "cargo" / "bin" / "rustfilt")
    for p in candidates:
        if p.exists():
            return str(p)
    return None


def find_elf(directory: Path) -> str | None:
    """Return the path to the first ELF file found in directory."""
    for p in sorted(directory.iterdir()):
        if not p.is_file():
            continue
        try:
            with open(p, "rb") as f:
                if f.read(4) == b"\x7fELF":
                    return str(p)
        except OSError:
            continue
    return None


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
    stem = Path(obj_path).stem  # strip last extension (.o or .obj)
    if stem.endswith(".rcgu"):
        # Rust CGU: {crate}-{16hexhash}.{...}.rcgu.o
        # Take the first dot-component and strip the trailing -<hexhash>
        first = stem.split(".")[0]
        crate = re.sub(r"-[0-9a-f]{16}$", "", first)
        return f"{crate}"
    # C / other object: partition_target.c.obj -> partition_target.c
    return stem


def sym_to_raw(sym_name: str) -> str:
    """Extract the lookup key from a symbol name as stored in the memory map.

    esp_idf_size appends '()' to STT_FUNC symbols and uses the input section
    name (e.g. '.text._ZN...E') as a fallback when no ELF symbol was matched.
    Both need to be normalised to the raw mangled name for addr_map lookup.
    """
    # Strip trailing '()' added by esp_idf_size for STT_FUNC symbols
    name = sym_name[:-2] if sym_name.endswith("()") else sym_name
    # Strip section prefix: ".text._ZN...", ".rodata._ZN...", etc.
    m = re.match(r"\.[\w.]+\.(_(?:ZN|R)\S*)", name)
    if m:
        return m.group(1)
    return name


def build_addr_map(map_file) -> dict[tuple[str, str], int]:
    """Build (object_file, symbol_name) -> address from a processed MapFile."""
    result: dict[tuple[str, str], int] = {}
    for section in map_file.sections:
        for isec in section.get("input_sections", []):
            obj = isec.get("object_file", "")
            for sym in isec.get("symbols", []):
                addr = sym.get("address", 0)
                name = sym.get("name", "")
                if addr and name:
                    result[(obj, name)] = addr
    return result


def build_file_offset_map(elf_obj: "ElfParser") -> list[tuple[int, int, int, int]]:
    """Return a list of (vaddr_start, vaddr_end, p_offset, p_filesz) for PT_LOAD
    segments, used to convert a virtual address to a file offset.
    Segments where p_filesz == 0 (pure BSS) are excluded.
    """
    segments = []
    for phdr in elf_obj.phdrs:
        if phdr.p_type != PT_LOAD or phdr.p_filesz == 0:
            continue
        segments.append(
            (
                phdr.p_vaddr,
                phdr.p_vaddr + phdr.p_filesz,
                phdr.p_offset,
                phdr.p_filesz,
            )
        )
    return segments


def vaddr_to_file_offset(vaddr: int, segments: list[tuple[int, int, int, int]]) -> int:
    """Convert a virtual address to a file offset using PT_LOAD segments.
    Returns 0 if the address is not backed by a file segment (e.g. BSS).
    """
    for vstart, vend, p_offset, _ in segments:
        if vstart <= vaddr < vend:
            return vaddr - vstart + p_offset
    return 0


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


@dataclasses.dataclass(slots=True)
class SymbolEntry:
    raw_sym: str
    archive: str
    obj: str
    address: int = 0
    total: int = 0
    flash: int = 0
    iram: int = 0
    diram: int = 0
    flash_data: int = 0
    file_offset: int = 0
    name: str = ""

    @classmethod
    def from_symbol(
        cls,
        archive_name: str,
        obj_file_name: str,
        sym_name: str,
        addr_map: dict[tuple[str, str], int],
        file_offset_segments: list[tuple[int, int, int, int]],
    ) -> "SymbolEntry":
        address = addr_map.get((obj_file_name, sym_name), 0)
        return cls(
            raw_sym=sym_to_raw(sym_name),
            archive=Path(archive_name).name,
            obj=shorten_obj(obj_file_name),
            address=address,
            file_offset=vaddr_to_file_offset(address, file_offset_segments),
        )

    def add_size(self, mem_type_name: str, size: int) -> None:
        self.total += size
        field_name = MEM_TYPE_FIELD.get(mem_type_name)
        if field_name is None:
            return
        setattr(self, field_name, getattr(self, field_name) + size)

    def finalize_name(self, demangled_name: str) -> None:
        self.name = demangled_name.strip() or self.raw_sym

    def sort_metric(self, sort_field: str) -> int:
        return getattr(self, sort_field)

    def format_address(self, blank: str = "-") -> str:
        return f"0x{self.address:08x}" if self.address else blank

    def format_file_range(self, blank: str = "-") -> str:
        if not self.file_offset:
            return blank
        file_offset_end = self.file_offset + self.total
        return f"0x{self.file_offset:07x}-0x{file_offset_end:07x}"

    def table_row(self, index: int) -> list[RenderableType]:
        return [
            str(index),
            self.obj,
            Text(self.name),
            fmt_bytes(self.total),
            fmt_bytes(self.flash),
            fmt_bytes(self.iram),
            fmt_bytes(self.diram),
            fmt_bytes(self.flash_data),
            self.format_address(),
            self.format_file_range(),
        ]

    def csv_row(self, index: int) -> list[str | int]:
        return [
            index,
            self.archive,
            self.obj,
            self.name or self.raw_sym,
            self.total,
            self.flash,
            self.iram,
            self.diram,
            self.flash_data,
            self.format_address(blank=""),
            self.format_file_range(blank=""),
        ]


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "map_file",
        nargs="?",
        default=DEFAULT_MAP,
        help=f"Path to the linker map file (default: {DEFAULT_MAP})",
    )
    parser.add_argument(
        "--top",
        "-n",
        type=int,
        default=50,
        metavar="N",
        help="Number of rows to show (default: 50, 0 = all)",
    )
    parser.add_argument(
        "--filter",
        "-f",
        default="",
        metavar="PATTERN",
        help="Case-insensitive substring filter on demangled name",
    )
    parser.add_argument(
        "--sort",
        "-s",
        choices=["total", "flash", "iram", "diram", "data"],
        default="total",
        help="Column to sort by (default: total)",
    )
    parser.add_argument(
        "--bytes",
        action="store_true",
        help="Show raw byte counts instead of human-readable sizes",
    )
    parser.add_argument(
        "--width",
        type=int,
        default=None,
        metavar="COLS",
        help="Override terminal width for the table",
    )
    parser.add_argument(
        "--csv",
        "-c",
        metavar="FILE",
        default=None,
        help="Write all rows to a CSV file (raw byte counts)",
    )
    args = parser.parse_args()

    console = Console(width=args.width)

    if not _HAS_ESP_IDF_SIZE:
        console.print(
            "[red]esp_idf_size not found.[/red] Install it or activate the project venv."
        )
        sys.exit(1)

    # --- locate rustfilt ---
    rustfilt = find_rustfilt()
    if rustfilt is None:
        console.print(
            "[red]rustfilt not found.[/red] Install it with:\n  cargo install rustfilt"
        )
        sys.exit(1)

    # --- validate map file ---
    map_path = Path(args.map_file)
    if not map_path.exists():
        console.print(f"[red]Map file not found:[/red] {map_path}")
        sys.exit(1)

    # --- find ELF in same directory as map file ---
    elf_obj = None
    file_offset_segments: list[tuple[int, int, int, int]] = []
    elf_path = find_elf(map_path.parent)
    if elf_path:
        with console.status("[bold cyan]Loading ELF…[/bold cyan]"):
            elf_obj = ElfParser(elf_path)
            file_offset_segments = build_file_offset_map(elf_obj)
        console.print(
            f"[dim]ELF: {elf_path} ({len(file_offset_segments)} LOAD segments)[/dim]"
        )
    else:
        console.print(
            "[yellow]No ELF found next to map file — addresses will be unavailable.[/yellow]"
        )

    # --- build memory map (pass a MapFile so we can read addresses back out) ---
    map_file_obj = esp_mapfile.MapFile(str(map_path))
    with console.status("[bold cyan]Parsing linker map…[/bold cyan]"):
        try:
            memory_map = memorymap.get(
                str(map_path), elf=elf_obj, map_file=map_file_obj
            )
        except memorymap.MemMapException as exc:
            console.print(f"[red]Failed to parse map file:[/red] {exc}")
            sys.exit(1)

    # Build (object_file, symbol_name) -> address from the processed map file.
    # This covers both ELF symbols and rodata/bss section-name fallbacks.
    addr_map = build_addr_map(map_file_obj)

    # --- walk memory map and collect per-symbol entries ---
    entries_by_key: dict[str, SymbolEntry] = {}
    for (
        mem_type_name,
        _,
        _,
        _,
        archive_name,
        _,
        obj_file_name,
        _,
        sym_name,
        sym_info,
    ) in memorymap.walk(memory_map):
        size = sym_info["size"]
        if not size:
            continue

        entry_key = f"{archive_name}\x00{obj_file_name}\x00{sym_name}"
        if entry_key not in entries_by_key:
            entries_by_key[entry_key] = SymbolEntry.from_symbol(
                archive_name,
                obj_file_name,
                sym_name,
                addr_map,
                file_offset_segments,
            )

        e = entries_by_key[entry_key]
        e.add_size(mem_type_name, size)

    entries = list(entries_by_key.values())
    console.print(f"[dim]Loaded {len(entries):,} symbols from {map_path}[/dim]\n")

    # --- demangle in one batch ---
    with console.status("[bold cyan]Demangling symbols…[/bold cyan]"):
        demangled = demangle_batch(rustfilt, [e.raw_sym for e in entries])

    for entry, name in zip(entries, demangled):
        entry.finalize_name(name)

    # --- filter ---
    filt = args.filter.lower()
    if filt:
        entries = [e for e in entries if filt in e.name.lower()]
        console.print(
            f"[dim]After filter '{args.filter}': {len(entries):,} entries[/dim]\n"
        )

    # --- sort ---
    sort_key = SORT_FIELD[args.sort]
    entries.sort(key=lambda entry: entry.sort_metric(sort_key), reverse=True)

    # --- truncate ---
    top_n = args.top if args.top > 0 else len(entries)
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
    table.add_column("#", style="dim", justify="right", no_wrap=True)
    table.add_column(
        "Object", style="dim", max_width=20, overflow="ellipsis", no_wrap=True
    )
    table.add_column("Function", min_width=40, max_width=None, overflow="fold")
    table.add_column("Total", style="bold yellow", justify="right", no_wrap=True)
    table.add_column("Flash Code", justify="right", no_wrap=True)
    table.add_column("IRAM", justify="right", no_wrap=True)
    table.add_column("DIRAM", justify="right", no_wrap=True)
    table.add_column("Flash Data", justify="right", no_wrap=True)
    table.add_column("Address", style="cyan", justify="right", no_wrap=True)
    table.add_column("File Range", style="magenta", justify="left", no_wrap=True)

    for i, e in enumerate(visible, 1):
        table.add_row(
            *e.table_row(i),
        )

    console.print(table)

    # --- optional CSV export ---
    if args.csv:
        csv_path = Path(args.csv)
        try:
            with csv_path.open("w", newline="") as fh:
                writer = csv.writer(fh)
                writer.writerow(
                    [
                        "#",
                        "Archive",
                        "Object",
                        "Function",
                        "Total",
                        "Flash Code",
                        "IRAM",
                        "DIRAM",
                        "Flash Data",
                        "Address",
                        "File Range",
                    ]
                )
                for i, e in enumerate(entries, 1):
                    writer.writerow(e.csv_row(i))
            console.print(f"[green]Wrote CSV:[/green] {csv_path}")
        except OSError as exc:
            console.print(f"[red]Failed to write CSV:[/red] {exc}")

    # --- totals footer ---
    tot_total = sum(e.total for e in entries)
    tot_flash = sum(e.flash for e in entries)
    tot_iram = sum(e.iram for e in entries)
    tot_diram = sum(e.diram for e in entries)
    tot_fdata = sum(e.flash_data for e in entries)

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
        f"--sort flash/iram/diram/data  --width COLS[/dim]"
    )


if __name__ == "__main__":
    main()
