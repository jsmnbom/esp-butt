#!/usr/bin/env -S uv run --script

# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "esp_idf_size",
#     "textual",
#     "rapidfuzz",
# ]
# ///
"""
Interactive TUI firmware symbol size analyzer.

Usage:
    uv run size_tui.py [MAP_FILE] [--diff OTHER_MAP]

Defaults to target/xtensa-esp32s3-espidf/debug/linker.map.

Keys
----
  F1  Archive view         F2  Object view
  F3  Symbol view (default) F4  Type / Trait grouping
  1-5 Sort column          /   Live search
  d   Toggle diff view     e   Export CSV
  Enter  Drill down        Esc Up / close search
  q   Quit

Search
------
  /alloc    — substring match (case-insensitive)
  /~alloc   — fuzzy match  (or prefix query with ~)
  Tab       — toggle substring / fuzzy while search bar is open

Fully vibe-coded: no guarantees on stability. Contributions welcome.
"""

from __future__ import annotations

import argparse
import csv
import os
import re
import shutil
import subprocess
import sys
import textwrap
from datetime import datetime
from pathlib import Path
from typing import Any

from rich import box
from rich.table import Table
from rapidfuzz import fuzz
from rapidfuzz import process as rfprocess
from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.geometry import Size
from textual.message import Message
from textual.screen import ModalScreen
from textual.widgets import Footer, Header, Input, Label, Static

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
# Constants
# ---------------------------------------------------------------------------

DEFAULT_MAP = "target/xtensa-esp32s3-espidf/debug/linker.map"

# Memory type names as used by esp_idf_size -> field name in our entry dict
MEM_TYPE_FIELD: dict[str, str] = {
    "Flash Code": "flash",
    "IRAM": "iram",
    "DIRAM": "diram",
    "Flash Data": "flash_data",
}

VIEW_SYMBOL = "symbol"
VIEW_ARCHIVE = "archive"
VIEW_OBJECT = "object"
VIEW_TYPE = "type"

SORT_COL_KEYS = ["total", "flash", "iram", "diram", "flash_data"]
SORT_COL_LABELS = ["Total", "Flash Code", "IRAM", "DIRAM", "Flash Data"]

# ---------------------------------------------------------------------------
# Helpers (ported from size_report.py)
# ---------------------------------------------------------------------------


def find_rustfilt() -> str | None:
    found = shutil.which("rustfilt")
    if found:
        return found
    cargo_home = os.environ.get("CARGO_HOME")
    candidates: list[Path] = []
    if cargo_home:
        candidates.append(Path(cargo_home) / "bin" / "rustfilt")
    candidates.append(Path.home() / ".local" / "share" / "cargo" / "bin" / "rustfilt")
    for p in candidates:
        if p.exists():
            return str(p)
    return None


def find_elf(directory: Path) -> str | None:
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
    if not symbols:
        return []
    result = subprocess.run(
        [rustfilt],
        input="\n".join(symbols),
        capture_output=True,
        text=True,
    )
    lines = result.stdout.splitlines()
    while len(lines) < len(symbols):
        lines.append("")
    return lines


def shorten_obj(obj_path: str) -> str:
    stem = Path(obj_path).stem
    if stem.endswith(".rcgu"):
        first = stem.split(".")[0]
        return re.sub(r"-[0-9a-f]{16}$", "", first)
    return stem


def sym_to_raw(sym_name: str) -> str:
    name = sym_name[:-2] if sym_name.endswith("()") else sym_name
    m = re.match(r"\.[\w.]+\.(_(?:ZN|R)\S*)", name)
    if m:
        return m.group(1)
    return name


def build_addr_map(map_file: Any) -> dict[tuple[str, str], int]:
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


def build_file_offset_map(elf_obj: Any) -> list[tuple[int, int, int, int]]:
    segments = []
    for phdr in elf_obj.phdrs:
        if phdr.p_type != PT_LOAD or phdr.p_filesz == 0:
            continue
        segments.append(
            (phdr.p_vaddr, phdr.p_vaddr + phdr.p_filesz, phdr.p_offset, phdr.p_filesz)
        )
    return segments


def vaddr_to_file_offset(vaddr: int, segments: list[tuple[int, int, int, int]]) -> int:
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


def _fmt_delta(n: int) -> str:
    if n == 0:
        return "-"
    sign = "+" if n > 0 else ""
    if abs(n) < 1024:
        return f"{sign}{n}B"
    if abs(n) < 1024 * 1024:
        return f"{sign}{n / 1024:.1f}K"
    return f"{sign}{n / 1024 / 1024:.2f}M"


# ---------------------------------------------------------------------------
# Data loading
# ---------------------------------------------------------------------------


def _blank_entry(raw_sym: str, archive: str, obj: str, addr: int, foff: int) -> dict:
    return {
        "raw_sym": raw_sym,
        "archive": archive,
        "obj": obj,
        "total": 0,
        "flash": 0,
        "iram": 0,
        "diram": 0,
        "flash_data": 0,
        "address": addr,
        "file_offset": foff,
        "delta_total": 0,
        "delta_flash": 0,
        "delta_iram": 0,
        "delta_diram": 0,
        "delta_flash_data": 0,
    }


def load_symbols(
    map_path: Path,
    rustfilt: str,
) -> tuple[list[dict], dict]:
    """Parse map file and return ``(entries, memory_map)``.

    Raises ``ValueError`` with a human-readable message on failure.
    """
    if not map_path.exists():
        raise ValueError(f"Map file not found: {map_path}")

    elf_obj = None
    file_offset_segments: list[tuple[int, int, int, int]] = []
    elf_path = find_elf(map_path.parent)
    if elf_path:
        elf_obj = ElfParser(elf_path)
        file_offset_segments = build_file_offset_map(elf_obj)

    map_file_obj = esp_mapfile.MapFile(str(map_path))
    try:
        memory_map = memorymap.get(str(map_path), elf=elf_obj, map_file=map_file_obj)
    except memorymap.MemMapException as exc:
        raise ValueError(f"Failed to parse map file: {exc}") from exc

    addr_map = build_addr_map(map_file_obj)

    entries_by_key: dict[str, dict] = {}
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
            raw_sym = sym_to_raw(sym_name)
            addr = addr_map.get((obj_file_name, sym_name), 0)
            entries_by_key[entry_key] = _blank_entry(
                raw_sym,
                Path(archive_name).name,
                shorten_obj(obj_file_name),
                addr,
                vaddr_to_file_offset(addr, file_offset_segments),
            )

        e = entries_by_key[entry_key]
        e["total"] += size
        field = MEM_TYPE_FIELD.get(mem_type_name)
        if field:
            e[field] += size

    entries = list(entries_by_key.values())
    demangled = demangle_batch(rustfilt, [e["raw_sym"] for e in entries])
    for entry, name in zip(entries, demangled):
        entry["name"] = name.strip() or entry["raw_sym"]

    return entries, memory_map


def load_diff_entries(
    map_path_cur: Path,
    map_path_ref: Path,
    rustfilt: str,
) -> list[dict]:
    """Return entries for *map_path_cur* with delta fields populated via
    ``memorymap.diff()``.  Raises ``ValueError`` on failure.
    """
    if not map_path_ref.exists():
        raise ValueError(f"Diff map file not found: {map_path_ref}")

    map_file_cur = esp_mapfile.MapFile(str(map_path_cur))
    map_file_ref = esp_mapfile.MapFile(str(map_path_ref))

    elf_cur: Any = None
    elf_path = find_elf(map_path_cur.parent)
    if elf_path:
        elf_cur = ElfParser(elf_path)

    elf_ref: Any = None
    elf_path_ref = find_elf(map_path_ref.parent)
    if elf_path_ref:
        elf_ref = ElfParser(elf_path_ref)

    mmap_cur = memorymap.get(str(map_path_cur), elf=elf_cur, map_file=map_file_cur)
    mmap_ref = memorymap.get(str(map_path_ref), elf=elf_ref, map_file=map_file_ref)
    mmap_diff = memorymap.diff(mmap_cur, mmap_ref)

    entries_by_key: dict[str, dict] = {}
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
    ) in memorymap.walk(mmap_diff):
        size = sym_info["size"]
        size_diff = sym_info.get("size_diff", 0)
        if not size and not size_diff:
            continue

        entry_key = f"{archive_name}\x00{obj_file_name}\x00{sym_name}"
        if entry_key not in entries_by_key:
            raw_sym = sym_to_raw(sym_name)
            entries_by_key[entry_key] = _blank_entry(
                raw_sym,
                Path(archive_name).name,
                shorten_obj(obj_file_name),
                0,
                0,
            )

        e = entries_by_key[entry_key]
        e["total"] += size
        e["delta_total"] += size_diff
        field = MEM_TYPE_FIELD.get(mem_type_name)
        if field:
            e[field] += size
            e[f"delta_{field}"] += size_diff

    entries = list(entries_by_key.values())
    demangled = demangle_batch(rustfilt, [e["raw_sym"] for e in entries])
    for entry, name in zip(entries, demangled):
        entry["name"] = name.strip() or entry["raw_sym"]

    return entries


# ---------------------------------------------------------------------------
# Aggregation / grouping helpers
# ---------------------------------------------------------------------------

_NUMERIC_FIELDS = (
    "total", "flash", "iram", "diram", "flash_data",
    "delta_total", "delta_flash", "delta_iram", "delta_diram", "delta_flash_data",
)


def aggregate(entries: list[dict], key: str) -> list[dict]:
    """Group entries by *key* field, summing all numeric fields."""
    groups: dict[str, dict] = {}
    for e in entries:
        k = e[key]
        if k not in groups:
            groups[k] = {"name": k, "count": 0, **{f: 0 for f in _NUMERIC_FIELDS}}
        g = groups[k]
        g["count"] += 1
        for f in _NUMERIC_FIELDS:
            g[f] += e[f]
    return list(groups.values())


_TRAIT_RE = re.compile(r"<(.+?) as (.+?)>::")


def _type_group_key(name: str) -> str:
    """Heuristic type/trait group key from a demangled Rust symbol name.

    Detects ``<Type as Trait>::method`` patterns first; falls back to stripping
    the last ``::segment`` to get the parent module/type path.

    Best-effort only — not guaranteed to cover every Rust naming pattern.
    """
    m = _TRAIT_RE.search(name)
    if m:
        return f"<{m.group(1)} as {m.group(2)}>"
    idx = name.rfind("::")
    if idx > 0:
        return name[:idx]
    return name


def group_by_type(entries: list[dict]) -> list[dict]:
    groups: dict[str, dict] = {}
    for e in entries:
        key = _type_group_key(e["name"])
        if key not in groups:
            groups[key] = {
                "name": key,
                "count": 0,
                "_member_names": [],
                **{f: 0 for f in _NUMERIC_FIELDS},
            }
        g = groups[key]
        g["count"] += 1
        g["_member_names"].append(e["name"])
        for f in _NUMERIC_FIELDS:
            g[f] += e[f]
    return list(groups.values())


# ---------------------------------------------------------------------------
# Diff path ModalScreen
# ---------------------------------------------------------------------------


class DiffInputScreen(ModalScreen[str | None]):
    """Modal prompt for the reference map file path."""

    DEFAULT_CSS = """
    DiffInputScreen {
        align: center middle;
    }
    #diff-dialog {
        width: 64;
        height: auto;
        border: thick $accent;
        background: $surface;
        padding: 1 2;
    }
    #diff-label {
        margin-bottom: 1;
    }
    """

    def compose(self) -> ComposeResult:
        with Vertical(id="diff-dialog"):
            yield Label("Enter path to reference map file:", id="diff-label")
            yield Input(placeholder="path/to/other.map", id="diff-input")

    @on(Input.Submitted, "#diff-input")
    def _on_submit(self, event: Input.Submitted) -> None:
        self.dismiss(event.value.strip() or None)

    def on_key(self, event: Any) -> None:
        if event.key == "escape":
            self.dismiss(None)


# ---------------------------------------------------------------------------
# Memory bar widget
# ---------------------------------------------------------------------------


class MemoryBar(Static):
    """Renders a compact progress-bar row for every memory type returned by the
    parsed memory map.  Capacities come directly from
    ``memory_map['memory_types'][name]['size']``—no hard-coded defaults.
    """

    _BAR_WIDTH = 22
    _FULL = "█"
    _EMPTY = "░"

    def __init__(self, memory_types: dict, **kwargs: Any) -> None:
        super().__init__("", **kwargs)
        self._memory_types = memory_types

    def on_mount(self) -> None:
        self._refresh()

    def refresh_memory(self, memory_types: dict) -> None:
        self._memory_types = memory_types
        self._refresh()

    def _refresh(self) -> None:
        lines: list[str] = []
        for name, info in self._memory_types.items():
            total = info.get("size", 0)
            used = info.get("used", 0)
            if not total:
                continue
            pct = used / total
            filled = round(pct * self._BAR_WIDTH)
            bar = self._FULL * filled + self._EMPTY * (self._BAR_WIDTH - filled)
            # Only warn in amber/red for Flash regions — RAM regions naturally
            # fill to 100% as the linker spills into DIRAM, so 100% IRAM is normal.
            is_flash = "flash" in name.lower()
            colour = (
                "red" if (pct >= 0.95 and is_flash)
                else "yellow" if (pct >= 0.80 and is_flash)
                else "cyan" if not is_flash
                else "green"
            )
            lines.append(
                f"[bold]{name:<12}[/bold] [{colour}]{bar}[/{colour}]"
                f"  [dim]{fmt_bytes(used)} / {fmt_bytes(total)}  {pct:.0%}[/dim]"
            )
        self.update("\n".join(lines) if lines else "[dim]No memory data[/dim]")


class WrappedDataTable(Static, can_focus=True):
    """Minimal keyboard-focused table with fixed column widths and wrapped text."""

    class RowSelected(Message):
        def __init__(self, table: "WrappedDataTable", cursor_row: int) -> None:
            self._table = table
            self.cursor_row = cursor_row
            super().__init__()

        @property
        def control(self) -> "WrappedDataTable":
            return self._table

    class RowHighlighted(Message):
        def __init__(self, table: "WrappedDataTable", cursor_row: int) -> None:
            self._table = table
            self.cursor_row = cursor_row
            super().__init__()

        @property
        def control(self) -> "WrappedDataTable":
            return self._table

    DEFAULT_CSS = """
    WrappedDataTable {
        height: 1fr;
        overflow-y: hidden;
    }
    """

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        kwargs.pop("cursor_type", None)
        super().__init__(*args, **kwargs)
        self._columns: list[dict[str, Any]] = []
        self._rows: list[list[str]] = []
        self.cursor_row = 0
        self._top_row = 0
        self._batch_depth = 0
        self._dirty = False

    def begin_batch(self) -> None:
        self._batch_depth += 1

    def end_batch(self) -> None:
        if self._batch_depth == 0:
            return
        self._batch_depth -= 1
        if self._batch_depth == 0 and self._dirty:
            self._dirty = False
            self._render_table()

    def _request_render(self) -> None:
        if self._batch_depth > 0:
            self._dirty = True
            return
        self._render_table()

    def clear(self, columns: bool = False) -> None:
        if columns:
            self._columns.clear()
        self._rows.clear()
        self.cursor_row = 0
        self._top_row = 0
        self._request_render()

    def add_column(self, label: str, key: str | None = None, width: int | None = None) -> None:
        min_width = max(8, len(label) + 2)
        self._columns.append(
            {
                "label": label,
                "key": key or label,
                "width": width,
                "min_width": min_width,
            }
        )
        self._request_render()

    def add_row(self, *values: str) -> None:
        row = [str(v) for v in values]
        self._rows.append(row)
        if self.cursor_row >= len(self._rows):
            self.cursor_row = max(0, len(self._rows) - 1)
        self._request_render()

    def on_resize(self, _: Size) -> None:
        self._request_render()

    def on_focus(self) -> None:
        self._request_render()

    def on_blur(self) -> None:
        self._request_render()

    def on_key(self, event: Any) -> None:
        if not self._rows:
            return

        moved = False
        if event.key == "up":
            if self.cursor_row > 0:
                self.cursor_row -= 1
                moved = True
            event.prevent_default()
        elif event.key == "down":
            if self.cursor_row < len(self._rows) - 1:
                self.cursor_row += 1
                moved = True
            event.prevent_default()
        elif event.key == "pageup":
            step = max(1, self.size.height - 4)
            self.cursor_row = max(0, self.cursor_row - step)
            moved = True
            event.prevent_default()
        elif event.key == "pagedown":
            step = max(1, self.size.height - 4)
            self.cursor_row = min(len(self._rows) - 1, self.cursor_row + step)
            moved = True
            event.prevent_default()
        elif event.key == "home":
            self.cursor_row = 0
            moved = True
            event.prevent_default()
        elif event.key == "end":
            self.cursor_row = len(self._rows) - 1
            moved = True
            event.prevent_default()
        elif event.key == "enter":
            self.post_message(self.RowSelected(self, self.cursor_row))
            event.prevent_default()

        if moved:
            self._ensure_cursor_visible()
            self._request_render()
            self.post_message(self.RowHighlighted(self, self.cursor_row))

    def _ensure_cursor_visible(self) -> None:
        visible_rows = max(1, self.size.height - 2)
        if self.cursor_row < self._top_row:
            self._top_row = self.cursor_row
        elif self.cursor_row >= self._top_row + visible_rows:
            self._top_row = self.cursor_row - visible_rows + 1

    def _wrap_cell(self, text: str, width: int) -> list[str]:
        if width <= 1:
            return [text]
        wrapped = textwrap.wrap(
            text,
            width=width,
            break_long_words=True,
            break_on_hyphens=False,
            replace_whitespace=False,
            drop_whitespace=False,
        )
        return wrapped or [""]

    def _render_table(self) -> None:
        if not self._columns:
            self.update("")
            return

        total_width = max(20, self.size.width)
        marker_width = 2
        sep = " | "
        sep_width = len(sep)

        fixed_width_sum = 0
        flex_indices: list[int] = []
        widths: list[int] = []
        for idx, col in enumerate(self._columns):
            if col["width"] is None:
                flex_indices.append(idx)
                widths.append(int(col["min_width"]))
            else:
                w = max(1, int(col["width"]))
                widths.append(w)
                fixed_width_sum += w

        available = total_width - marker_width - sep_width * (len(self._columns) - 1)
        available = max(len(self._columns), available)

        min_flex_sum = sum(widths[i] for i in flex_indices)
        remaining = available - fixed_width_sum - min_flex_sum
        if flex_indices and remaining > 0:
            per_col = remaining // len(flex_indices)
            extra = remaining % len(flex_indices)
            for i, col_idx in enumerate(flex_indices):
                widths[col_idx] += per_col + (1 if i < extra else 0)

        table = Table(
            box=box.SIMPLE_HEAVY,
            expand=True,
            show_header=True,
            show_lines=False,
            header_style="bold",
            pad_edge=False,
            collapse_padding=True,
        )
        for col, width in zip(self._columns, widths):
            table.add_column(
                str(col["label"]),
                width=width,
                min_width=max(1, int(col["min_width"])),
                no_wrap=False,
                overflow="fold",
            )

        height = max(3, self.size.height)
        usable = max(1, height - 2)

        self._ensure_cursor_visible()
        end_row = min(len(self._rows), self._top_row + usable)

        for row_idx in range(self._top_row, end_row):
            row = self._rows[row_idx]
            cells: list[str] = []
            for col_idx, width in enumerate(widths):
                cell = row[col_idx] if col_idx < len(row) else ""
                # Keep explicit wrapping so very long symbol names remain responsive.
                cells.append("\n".join(self._wrap_cell(cell, width)))

            if row_idx == self.cursor_row:
                selected_style = "reverse" if self.has_focus else "bold"
                table.add_row(*cells, style=selected_style)
            else:
                table.add_row(*cells)

        self.update(table)


# ---------------------------------------------------------------------------
# TUI App
# ---------------------------------------------------------------------------

APP_CSS = """
Screen { layout: vertical; }

#breadcrumb {
    height: 1;
    background: $primary-darken-2;
    color: $text;
    padding: 0 1;
    text-overflow: ellipsis;
}

WrappedDataTable { height: 1fr; }

MemoryBar {
    height: auto;
    background: $surface-darken-1;
    padding: 0 1;
    border-top: tall $primary;
}

#detail-bar {
    height: 1;
    background: $surface;
    color: $text-muted;
    padding: 0 1;
    overflow-x: hidden;
}

#search-input {
    height: 3;
    display: none;
}

#search-input.visible {
    display: block;
}
"""


class SizeApp(App):
    CSS = APP_CSS
    TITLE = "esp firmware size"

    BINDINGS = [
        Binding("f1", "view_archive", "Archives"),
        Binding("f2", "view_object", "Objects"),
        Binding("f3", "view_symbol", "Symbols"),
        Binding("f4", "view_type", "Types"),
        Binding("1", "sort_col('total')", "Sort:Total", show=False),
        Binding("2", "sort_col('flash')", "Sort:Flash", show=False),
        Binding("3", "sort_col('iram')", "Sort:IRAM", show=False),
        Binding("4", "sort_col('diram')", "Sort:DIRAM", show=False),
        Binding("5", "sort_col('flash_data')", "Sort:Data", show=False),
        Binding("/", "open_search", "Search"),
        Binding("d", "toggle_diff", "Diff"),
        Binding("e", "export_csv", "Export CSV"),
        Binding("escape", "go_up", "Up/Close", show=False),
        Binding("q", "quit", "Quit"),
    ]

    def __init__(
        self,
        map_path: Path,
        diff_path: Path | None,
        rustfilt: str,
        **kwargs: Any,
    ) -> None:
        super().__init__(**kwargs)
        self._map_path = map_path
        self._diff_path = diff_path
        self._rustfilt = rustfilt

        # Loaded data
        self._all_entries: list[dict] = []
        self._diff_entries: list[dict] = []
        self._memory_map: dict = {}

        # View state
        self._view = VIEW_SYMBOL
        # Drill stack: list of (field, value, previous_view)
        self._drill_stack: list[tuple[str, str, str]] = []
        self._sort_col = "total"
        self._sort_reverse = True
        self._search_text = ""
        self._fuzzy_mode = False
        self._diff_mode = False
        self._loading = True

    # ------------------------------------------------------------------
    # Layout
    # ------------------------------------------------------------------

    def compose(self) -> ComposeResult:
        yield Header()
        yield Label("Loading…", id="breadcrumb")
        yield WrappedDataTable(id="main-table")
        yield Static("", id="detail-bar")
        yield MemoryBar({}, id="membar")
        yield Input(
            placeholder="  /query  or  ~fuzzy  |  Tab toggles mode  |  Esc closes",
            id="search-input",
        )
        yield Footer()

    def on_mount(self) -> None:
        self._load_data()

    # ------------------------------------------------------------------
    # Background data loading
    # ------------------------------------------------------------------

    @work(thread=True)
    def _load_data(self) -> None:
        try:
            entries, memory_map = load_symbols(self._map_path, self._rustfilt)
        except Exception as exc:
            self.call_from_thread(
                self.notify, str(exc), severity="error", timeout=15
            )
            return

        diff_entries: list[dict] = []
        if self._diff_path:
            try:
                diff_entries = load_diff_entries(
                    self._map_path, self._diff_path, self._rustfilt
                )
            except Exception as exc:
                self.call_from_thread(
                    self.notify, f"Diff load failed: {exc}", severity="warning"
                )

        self.call_from_thread(self._on_data_loaded, entries, memory_map, diff_entries)

    def _on_data_loaded(
        self,
        entries: list[dict],
        memory_map: dict,
        diff_entries: list[dict],
    ) -> None:
        self._all_entries = entries
        self._memory_map = memory_map
        self._diff_entries = diff_entries
        self._loading = False

        membar = self.query_one(MemoryBar)
        membar.refresh_memory(memory_map.get("memory_types", {}))

        if diff_entries:
            self._diff_mode = True

        self._rebuild_table()
        self.notify(
            f"Loaded {len(entries):,} symbols",
            timeout=3,
        )

    # ------------------------------------------------------------------
    # Actions — view switching
    # ------------------------------------------------------------------

    def action_view_archive(self) -> None:
        self._view = VIEW_ARCHIVE
        self._drill_stack.clear()
        self._rebuild_table()

    def action_view_object(self) -> None:
        self._view = VIEW_OBJECT
        self._drill_stack.clear()
        self._rebuild_table()

    def action_view_symbol(self) -> None:
        self._view = VIEW_SYMBOL
        self._drill_stack.clear()
        self._rebuild_table()

    def action_view_type(self) -> None:
        self._view = VIEW_TYPE
        self._drill_stack.clear()
        self._rebuild_table()

    # ------------------------------------------------------------------
    # Actions — sort
    # ------------------------------------------------------------------

    def action_sort_col(self, col: str) -> None:
        if self._sort_col == col:
            self._sort_reverse = not self._sort_reverse
        else:
            self._sort_col = col
            self._sort_reverse = True
        self._rebuild_table()

    # ------------------------------------------------------------------
    # Actions — search
    # ------------------------------------------------------------------

    def action_open_search(self) -> None:
        inp = self.query_one("#search-input", Input)
        inp.add_class("visible")
        inp.focus()

    @on(Input.Changed, "#search-input")
    def _on_search_changed(self, event: Input.Changed) -> None:
        self._search_text = event.value
        self._rebuild_table()

    def on_key(self, event: Any) -> None:
        """Toggle fuzzy/substring mode with Tab while search is open and focused."""
        if event.key == "tab":
            inp = self.query_one("#search-input", Input)
            if "visible" in inp.classes and self.focused is inp:
                event.prevent_default()
                self._fuzzy_mode = not self._fuzzy_mode
                mode = "fuzzy" if self._fuzzy_mode else "substring"
                inp.placeholder = (
                    f"  [{mode}]  /query  or  ~fuzzy  |  Tab toggles  |  Esc closes"
                )
                self._rebuild_table()

    # ------------------------------------------------------------------
    # Actions — drill down / up
    # ------------------------------------------------------------------

    @on(WrappedDataTable.RowSelected, "#main-table")
    def _on_row_selected(self, event: WrappedDataTable.RowSelected) -> None:
        if self._loading:
            return
        visible = self._get_visible_entries()
        idx = event.cursor_row
        if idx >= len(visible):
            return
        entry = visible[idx]

        if self._view == VIEW_ARCHIVE:
            self._drill_stack.append(("archive", entry["name"], VIEW_ARCHIVE))
            self._view = VIEW_OBJECT
            self._rebuild_table()

        elif self._view == VIEW_OBJECT:
            self._drill_stack.append(("obj", entry["name"], VIEW_OBJECT))
            self._view = VIEW_SYMBOL
            self._rebuild_table()

        elif self._view == VIEW_TYPE:
            self._drill_stack.append(("_type_group", entry["name"], VIEW_TYPE))
            self._view = VIEW_SYMBOL
            self._rebuild_table()

        # VIEW_SYMBOL — no further drill

    def action_go_up(self) -> None:
        # If search is open, close it first
        inp = self.query_one("#search-input", Input)
        if "visible" in inp.classes:
            inp.remove_class("visible")
            inp.value = ""
            self._search_text = ""
            self._fuzzy_mode = False
            self._rebuild_table()
            self.query_one(WrappedDataTable).focus()
            return

        # Pop drill stack — restore previous view
        if self._drill_stack:
            _, _, prev_view = self._drill_stack.pop()
            self._view = prev_view
            self._rebuild_table()

    # ------------------------------------------------------------------
    # Actions — diff
    # ------------------------------------------------------------------

    def action_toggle_diff(self) -> None:
        if not self._diff_entries:
            if self._diff_path:
                self.notify("Diff map did not load successfully.", severity="warning")
            else:
                self.push_screen(DiffInputScreen(), self._handle_diff_path_result)
            return
        self._diff_mode = not self._diff_mode
        self.notify(f"Diff mode {'ON' if self._diff_mode else 'OFF'}", timeout=2)
        self._rebuild_table()

    def _handle_diff_path_result(self, path_str: str | None) -> None:
        if not path_str:
            return
        self._diff_path = Path(path_str)
        self.notify(f"Loading diff map…", timeout=2)
        self._load_diff_only(self._diff_path)

    @work(thread=True)
    def _load_diff_only(self, diff_path: Path) -> None:
        try:
            entries = load_diff_entries(self._map_path, diff_path, self._rustfilt)
        except Exception as exc:
            self.call_from_thread(self.notify, str(exc), severity="error")
            return
        self.call_from_thread(self._on_diff_loaded, entries)

    def _on_diff_loaded(self, entries: list[dict]) -> None:
        self._diff_entries = entries
        self._diff_mode = True
        self._rebuild_table()
        self.notify("Diff loaded.", timeout=2)

    # ------------------------------------------------------------------
    # Actions — export
    # ------------------------------------------------------------------

    def action_export_csv(self) -> None:
        if self._loading:
            return
        visible = self._get_visible_entries()
        ts = datetime.now().strftime("%Y%m%d_%H%M%S")
        fname = f"size_report_{ts}.csv"

        if self._view == VIEW_SYMBOL:
            base_fields = [
                "name", "archive", "obj",
                "total", "flash", "iram", "diram", "flash_data",
                "address", "file_offset",
            ]
        else:
            base_fields = ["name", "count", "total", "flash", "iram", "diram", "flash_data"]

        if self._diff_mode:
            base_fields += [
                "delta_total", "delta_flash", "delta_iram", "delta_diram", "delta_flash_data"
            ]

        with open(fname, "w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=base_fields, extrasaction="ignore")
            writer.writeheader()
            writer.writerows(visible)

        self.notify(f"Exported {len(visible):,} rows → {fname}")

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _get_base_entries(self) -> list[dict]:
        if self._diff_mode and self._diff_entries:
            return self._diff_entries
        return self._all_entries

    def _apply_drill(self, entries: list[dict]) -> list[dict]:
        for field, value, _ in self._drill_stack:
            if field == "_type_group":
                entries = [e for e in entries if _type_group_key(e["name"]) == value]
            else:
                entries = [e for e in entries if e.get(field) == value]
        return entries

    def _apply_search(self, entries: list[dict]) -> list[dict]:
        q = self._search_text
        if not q:
            return entries

        # ~ prefix activates fuzzy regardless of the toggle
        fuzzy = self._fuzzy_mode or q.startswith("~")
        query = q.lstrip("~").strip()
        if not query:
            return entries

        names = [e.get("name", "") for e in entries]
        if fuzzy:
            results = rfprocess.extract(
                query, names, scorer=fuzz.WRatio, score_cutoff=60, limit=None
            )
            matched = {idx for _, _, idx in results}
            return [e for i, e in enumerate(entries) if i in matched]
        else:
            ql = query.lower()
            return [e for e in entries if ql in (e.get("name") or "").lower()]

    def _get_visible_entries(self) -> list[dict]:
        base = self._get_base_entries()
        drilled = self._apply_drill(base)

        if self._view == VIEW_ARCHIVE:
            aggregated: list[dict] = aggregate(drilled, "archive")
        elif self._view == VIEW_OBJECT:
            aggregated = aggregate(drilled, "obj")
        elif self._view == VIEW_TYPE:
            aggregated = group_by_type(drilled)
        else:
            aggregated = drilled

        filtered = self._apply_search(aggregated)
        filtered.sort(key=lambda e: e.get(self._sort_col, 0), reverse=self._sort_reverse)
        return filtered

    def _sort_label(self, col_key: str, label: str) -> str:
        if col_key == self._sort_col:
            arrow = "▼" if self._sort_reverse else "▲"
            return f"{label} {arrow}"
        return label

    @on(WrappedDataTable.RowHighlighted, "#main-table")
    def _on_row_highlighted(self, event: WrappedDataTable.RowHighlighted) -> None:
        idx = event.cursor_row
        if hasattr(self, "_visible") and 0 <= idx < len(self._visible):
            name = self._visible[idx].get("name", "")
            self.query_one("#detail-bar", Static).update(name)

    def _rebuild_table(self) -> None:
        if self._loading:
            return
        visible = self._get_visible_entries()
        self._visible = visible
        table = self.query_one(WrappedDataTable)
        table.begin_batch()
        try:
            table.clear(columns=True)

            diff = self._diff_mode

            if self._view == VIEW_SYMBOL:
                table.add_column("#", key="row_num", width=5)
                table.add_column("Object", key="obj", width=18)
                table.add_column("Function", key="name")
                table.add_column(self._sort_label("total", "Total"), key="total", width=8)
                table.add_column(self._sort_label("flash", "Code"), key="flash", width=8)
                table.add_column(self._sort_label("flash_data", "Data"), key="flash_data", width=8)
                table.add_column(self._sort_label("iram", "IRAM"), key="iram", width=8)
                table.add_column(self._sort_label("diram", "DIRAM"), key="diram", width=8)
                table.add_column("Address", key="address", width=10)
                table.add_column("File Range", key="file_range", width=21)
                if diff:
                    table.add_column("Δ Total", key="d_total", width=8)
                    table.add_column("Δ Flash", key="d_flash", width=8)
                    table.add_column("Δ IRAM", key="d_iram", width=8)
                    table.add_column("Δ DIRAM", key="d_diram", width=8)

                for i, e in enumerate(visible, 1):
                    addr = e.get("address", 0)
                    addr_str = f"0x{addr:08x}" if addr else "-"
                    foff = e.get("file_offset", 0)
                    foff_str = (
                        f"0x{foff:07x}-0x{foff + e['total']:07x}" if foff else "-"
                    )
                    row: list[str] = [
                        str(i),
                        e["obj"],
                        e["name"],
                        fmt_bytes(e["total"]),
                        fmt_bytes(e["flash"]),
                        fmt_bytes(e["flash_data"]),
                        fmt_bytes(e["iram"]),
                        fmt_bytes(e["diram"]),
                        addr_str,
                        foff_str,
                    ]
                    if diff:
                        row += [
                            _fmt_delta(e.get("delta_total", 0)),
                            _fmt_delta(e.get("delta_flash", 0)),
                            _fmt_delta(e.get("delta_iram", 0)),
                            _fmt_delta(e.get("delta_diram", 0)),
                        ]
                    table.add_row(*row)

            elif self._view in (VIEW_ARCHIVE, VIEW_OBJECT):
                col_label = "Archive" if self._view == VIEW_ARCHIVE else "Object"
                table.add_column(col_label, key="name")
                table.add_column("Syms", key="count", width=6)
                table.add_column(self._sort_label("total", "Total"), key="total", width=8)
                table.add_column(self._sort_label("flash", "Code"), key="flash", width=8)
                table.add_column(self._sort_label("flash_data", "Data"), key="flash_data", width=8)
                table.add_column(self._sort_label("iram", "IRAM"), key="iram", width=8)
                table.add_column(self._sort_label("diram", "DIRAM"), key="diram", width=8)
                if diff:
                    table.add_column("Δ Total", key="d_total", width=8)
                    table.add_column("Δ Flash", key="d_flash", width=8)

                for e in visible:
                    row = [
                        e["name"],
                        str(e["count"]),
                        fmt_bytes(e["total"]),
                        fmt_bytes(e["flash"]),
                        fmt_bytes(e["flash_data"]),
                        fmt_bytes(e["iram"]),
                        fmt_bytes(e["diram"]),
                    ]
                    if diff:
                        row += [
                            _fmt_delta(e.get("delta_total", 0)),
                            _fmt_delta(e.get("delta_flash", 0)),
                        ]
                    table.add_row(*row)

            else:  # VIEW_TYPE
                table.add_column("Type / Trait", key="name")
                table.add_column("Syms", key="count", width=6)
                table.add_column(self._sort_label("total", "Total"), key="total", width=8)
                table.add_column(self._sort_label("flash", "Code"), key="flash", width=8)
                table.add_column(self._sort_label("flash_data", "Data"), key="flash_data", width=8)
                table.add_column(self._sort_label("iram", "IRAM"), key="iram", width=8)
                table.add_column(self._sort_label("diram", "DIRAM"), key="diram", width=8)
                if diff:
                    table.add_column("Δ Total", key="d_total", width=8)

                for e in visible:
                    row = [
                        e["name"],
                        str(e["count"]),
                        fmt_bytes(e["total"]),
                        fmt_bytes(e["flash"]),
                        fmt_bytes(e["flash_data"]),
                        fmt_bytes(e["iram"]),
                        fmt_bytes(e["diram"]),
                    ]
                    if diff:
                        row += [_fmt_delta(e.get("delta_total", 0))]
                    table.add_row(*row)
        finally:
            table.end_batch()

        self._update_breadcrumb(len(visible))

    def _update_breadcrumb(self, shown: int) -> None:
        total = len(self._get_base_entries())

        view_label = {
            VIEW_SYMBOL: "Symbols",
            VIEW_ARCHIVE: "Archives",
            VIEW_OBJECT: "Objects",
            VIEW_TYPE: "Types",
        }[self._view]

        crumb_parts = ["All"] + [v for _, v, _ in self._drill_stack]
        crumb = " › ".join(crumb_parts)

        sort_display = dict(zip(SORT_COL_KEYS, SORT_COL_LABELS)).get(
            self._sort_col, self._sort_col
        )
        arrow = "▼" if self._sort_reverse else "▲"
        diff_tag = "  [DIFF ON]" if self._diff_mode else ""

        search_q = self._search_text.lstrip("~").strip()
        mode_tag = " [fuzzy]" if (self._fuzzy_mode or self._search_text.startswith("~")) else ""
        search_tag = f"  /{search_q}{mode_tag}" if search_q else ""

        self.query_one("#breadcrumb", Label).update(
            f"{crumb}  [{view_label}]  ·  {shown:,}/{total:,}"
            f"  sort:{sort_display}{arrow}{diff_tag}{search_tag}"
        )


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


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
        "--diff",
        metavar="OTHER_MAP",
        default=None,
        help="Reference map file to diff against",
    )
    args = parser.parse_args()

    if not _HAS_ESP_IDF_SIZE:
        print(
            "esp_idf_size not found. Install it or activate the project venv.",
            file=sys.stderr,
        )
        sys.exit(1)

    rustfilt = find_rustfilt()
    if rustfilt is None:
        print(
            "rustfilt not found. Install with: cargo install rustfilt",
            file=sys.stderr,
        )
        sys.exit(1)

    app = SizeApp(
        map_path=Path(args.map_file),
        diff_path=Path(args.diff) if args.diff else None,
        rustfilt=rustfilt,
    )
    app.run()


if __name__ == "__main__":
    main()
