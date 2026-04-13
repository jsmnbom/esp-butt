import csv
from datetime import datetime
from io import BytesIO
from pathlib import Path
import copy
import subprocess
from anytree import LevelOrderIter
from build123d import Color, Compound, Compound, Face, Location, Vector, export_step, import_step
from build123d.topology.shape_core import Shape, downcast
from build123d.geometry import ColorLike

__all__ = ["PCBCompound", "load_pcb", "fast_copy", "copy_located", "export", "import_step_colored"]

KICAD_CLI = "kicad-cli"


def kicad_run_pcb_export(pcb_path: str | Path, export_format: str, output: str | Path, *args):
  p = subprocess.run(
    [KICAD_CLI, "pcb", "export", export_format, "--output", str(output), *args, str(pcb_path)],
    check=True,
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
  def __init__(self, label: str = "", children: tuple[Shape, ...] = ()):
    super().__init__(label=label, children=children)

  @property
  def pcb(self) -> Shape:
    return self.get(f"{self.label}_PCB")

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

    if kicad_is_export_needed(pcb_dir, [step_path, pos_path]):
      kicad_export_step(pcb_path, step_path)
      kicad_export_pos(pcb_path, pos_path)

    step = import_step(step_path)
    pcb_compound = cls(label=name, children=step.children)
    pcb_compound.rename_from_pos(pos_path)
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


def fast_copy(shape: Shape) -> Shape:
  cls = shape.__class__
  result = cls.__new__(cls)
  for key, value in shape.__dict__.items():
    if key == "_wrapped":
      result._wrapped = downcast(shape.wrapped.Located(shape.wrapped.Location()))
    elif key in ("_reset_tok", "_python_frame"):
      pass
    else:
      try:
        setattr(result, key, value if key.startswith("_NodeMixin__") else copy.copy(value))
      except Exception as e:
        print(f"Error copying attribute '{key}': {e}")
  return result


def copy_located(shape: Shape, location=Location(), fast=True) -> Shape:
  shape = fast_copy(shape) if fast else copy.copy(shape)
  shape.location = location
  return shape


def find_last_file(directory: str | Path, prefix: str, extension: str) -> Path | None:
  directory = Path(directory)
  files = list(directory.glob(f"{prefix}*.{extension}"))
  if not files:
    return None
  files.sort(key=lambda f: f.stat().st_mtime, reverse=True)
  return files[0]


def export(shape: Shape):
  if not shape.label:
    raise ValueError("Shape must have a label to be exported.")
  export_dir = Path("./export")
  export_dir.mkdir(parents=True, exist_ok=True)

  step_data = BytesIO()
  # override timestamp so compare works
  timestamp = datetime.fromtimestamp(0)

  # Find last .step file for this shape
  # Find last .* file for this shape
  # .* is for .3mf and such for also versioning print settings
  last_step = find_last_file(export_dir, shape.label, "step")
  last_any = find_last_file(export_dir, shape.label, "*")

  # export only if last step is missing or different from current shape
  if last_step:
    last_step_data = last_any.read_bytes()
    export_step(shape, step_data, timestamp=timestamp)
    if step_data.getvalue() == last_step_data:
      print(f"No changes detected for '{shape.label}', skipping export.")
      return

  # If we get here, we need to export a new file
  # Next version will be highest existing version + 1, or 1 if no existing files

  last_step_version = (
    int(last_step.stem.split(".v")[-1]) if last_step and ".v" in last_step.stem else 0
  )
  last_any_version = int(last_any.stem.split(".v")[-1]) if last_any and ".v" in last_any.stem else 0
  version = max(last_step_version, last_any_version) + 1

  # make sure we have data
  if step_data.getvalue() == b"":
    export_step(shape, step_data, timestamp=timestamp)

  new_file = export_dir / f"{shape.label}.v{version}.step"
  new_file.write_bytes(step_data.getvalue())
  print(f"Exported '{shape.label}' to {new_file}")
