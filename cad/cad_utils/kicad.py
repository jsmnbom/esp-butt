import subprocess
from pathlib import Path
import csv
import os

from anytree import LevelOrderIter
from build123d import Color, Compound, Vector, import_step
from build123d.topology.shape_core import Shape
from build123d.geometry import ColorLike

from OCP.TDocStd import TDocStd_Document

from .doc import import_step_doc

from .utils import fast_copy

KICAD_CLI = os.getenv("KICAD_CLI", "kicad-cli")


def kicad_run_pcb_export(pcb_path: str | Path, export_format: str, output: str | Path, *args, export_type="pcb"):
  p = subprocess.run(
    [KICAD_CLI, export_type, "export", export_format, "--output", str(output), *args, str(pcb_path)],
    capture_output=True,
  )
  if p.returncode != 0:
    print(p.stdout.decode())
    raise RuntimeError(f"Failed to export PCB: {p.stderr.decode()}")


def kicad_export_step(pcb_path: str | Path, output: str | Path):
  kicad_run_pcb_export(
    pcb_path,
    "step",
    output,
    "--force",
    "--subst-models",
    "--include-silkscreen",
  )


def kicad_export_pos(pcb_path: str | Path, output: str | Path):
  kicad_run_pcb_export(
    pcb_path,
    "pos",
    output,
    "--side",
    "both",
    "--format",
    "csv",
    "--units",
    "mm",
  )


def kicad_is_export_needed(pcb_dir: str | Path, exports: list[Path]) -> bool:
  # check if any export is missing or older than any files in the pcb_dir
  pcb_dir = Path(pcb_dir)
  pcb_files = list(pcb_dir.glob("**/*"))
  if not pcb_files:
    raise ValueError(f"No files found in PCB directory '{pcb_dir}'")
  pcb_files.sort(key=lambda f: f.stat().st_mtime, reverse=True)
  latest_pcb_time = pcb_files[0].stat().st_mtime
  for export in exports:
    if not export.exists() or export.stat().st_mtime < latest_pcb_time:
      return True
  return False


class PCBCompound(Compound):
  def __init__(self, name: str = "", children: tuple[Shape, ...] = ()):
    self.name = name
    super().__init__(label="pcb", children=children)

  @property
  def pcb(self) -> Shape:
    return self.get(f"pcb")

  @classmethod
  def load_pcb(
    cls,
    name: str,
    pcb_dir: str | Path,
    pcb_export_dir: str | Path,
    renames: dict[str, str],
    colors: dict[str, ColorLike],
  ) -> "PCBCompound":
    export_dir = Path(pcb_export_dir)
    export_dir.mkdir(parents=True, exist_ok=True)
    pcb_path = Path(pcb_dir) / f"{name}.kicad_pcb"
    step_path = export_dir / f"{name}.step"
    pos_path = export_dir / f"{name}.pos.csv"

    renames['PCB'] = 'pcb'

    if kicad_is_export_needed(pcb_dir, [step_path, pos_path]):
      kicad_export_step(pcb_path, step_path)
      kicad_export_pos(pcb_path, pos_path)

    step = import_step(step_path)
    pcb_compound = cls(name=name, children=step.children)
    pcb_compound.rename_from_pos(pos_path)
    pcb_compound.rename_remove_prefix(f"{name}_")
    pcb_compound.apply_renames(renames)
    pcb_compound.apply_colors(colors)

    return pcb_compound

  def rename_from_pos(self, pos_path: str | Path):
    pos_map: dict[Vector, str] = {}
    with Path(pos_path).open(newline="") as f:
      reader = csv.DictReader(f)
      for row in reader:
        pos_map[Vector(float(row["PosX"]), float(row["PosY"]))] = f"{row['Ref']} {row['Val']}"

    children = [c for c in self.children if not c.label.startswith(f"{self.label}_")]
    while children and pos_map:
      (pos, label) = pos_map.popitem()
      if closest := next((c for c in children if (c.location.position - pos).length < 2), None):
        closest.label = label
        children.remove(closest)
      elif closest := next((c for c in children if (c.location.position - pos).length < 10), None):
        closest.label = label
        children.remove(closest)

  def get(self, name) -> Shape:
    for c in self.children:
      if c.label == name:
        return c
    raise ValueError(f"Top level node with label '{name}' not found")

  def find(self, prefix, copy=True) -> Shape:
    for c in LevelOrderIter(self):
      if c.label.startswith(prefix):
        if copy:
          c = fast_copy(c)
          c.locate(c.global_location)
        return c
    raise ValueError(f"Node with label starting with '{prefix}' not found")

  def apply_renames(self, renames: dict[str, str]):
    for rename_from, rename_to in renames.items():
      if node := self.find(rename_from, copy=False):
        node.label = rename_to

  def rename_remove_prefix(self, prefix: str):
    for c in LevelOrderIter(self):
      if c.label.startswith(prefix):
        c.label = c.label[len(prefix) :]

  def apply_colors(self, colors: dict[str, ColorLike]):
    for prefix, color in colors.items():
      if node := self.find(prefix, copy=False):
        node.color = Color(color)


def load_pcb(
  name: str,
  pcb_path: str | Path,
  pcb_export_dir: str | Path,
  renames: dict[str, str] = {},
  colors: dict[str, ColorLike] = {},
) -> PCBCompound:
  return PCBCompound.load_pcb(name, pcb_path, pcb_export_dir, renames, colors)

def load_pcb_doc(
  name: str,
  pcb_path: str | Path,
  pcb_export_dir: str | Path,
) -> TDocStd_Document:
  export_dir = Path(pcb_export_dir)
  export_dir.mkdir(parents=True, exist_ok=True)
  pcb_path = Path(pcb_path) / f"{name}.kicad_pcb"
  step_path = export_dir / f"{name}.step"

  if kicad_is_export_needed(pcb_path.parent, [step_path]):
    kicad_export_step(pcb_path, step_path)

  doc = import_step_doc(step_path)

  return doc
