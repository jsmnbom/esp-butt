#!/usr/bin/env -S uv run
"""
Build documentation assets: 3D models (CAD), PCB glTF, BOM, and PCB SVGs.

Usage:
    ./assets.py              # run all steps
    ./assets.py cad svg      # run specific steps (in order)

Steps:
  cad   Export 3D models (.step / .glb) from CAD notebooks
  pcb   Export PCB as glTF (pcb.glb) for the 3D viewer
  bom   Export bill of materials (bom.csv) from the schematic
  svg   Export schematic and PCB SVGs from KiCad via kicad-cli / Inkscape
"""

import argparse
import os
import subprocess
from pathlib import Path
import sys

REPO_ROOT = Path(__file__).resolve().parent.parent
DOCS_DIR = REPO_ROOT / "docs"
DOCS_PUBLIC = DOCS_DIR / "public"
MODELS_DIR = DOCS_DIR / "models"
SVG_DIR = DOCS_DIR / "svg"
CAD_DIR = REPO_ROOT / "cad"
PCB_DIR = REPO_ROOT / "pcb"
PCB_EXPORT_DIR = PCB_DIR / "export"
RECORDING_DIR = DOCS_DIR / "recording"

PCB_NAME = "esp-butt"
DARK_THEME = "witchhazel"

PCB_FRONT_LAYERS = "F.Cu,F.Mask,F.Silkscreen,Edge.Cuts"
PCB_BACK_LAYERS = "B.Cu,B.Mask,B.Silkscreen,Edge.Cuts"

SCH_FILE = PCB_DIR / PCB_NAME / f"{PCB_NAME}.kicad_sch"
PCB_FILE = PCB_DIR / PCB_NAME / f"{PCB_NAME}.kicad_pcb"
BOM_FILE = DOCS_DIR / "bom.csv"

KICAD_CLI = os.getenv("KICAD_CLI", "kicad-cli")
INKSCAPE = os.getenv("INKSCAPE", "inkscape")

os.environ["BUILDING_DOCS"] = "1"
os.environ["NO_CAD_EXPORT"] = "1"
os.environ["KICAD_CONFIG_HOME"] = os.environ.get("KICAD_CONFIG_HOME", str(PCB_DIR / "kicad-config"))

sys.path.insert(0, str(REPO_ROOT))
sys.path.insert(0, str(CAD_DIR))


def inkscape_crop_svg(file: Path) -> None:
  """Crop an SVG in-place to its drawing bounds using Inkscape."""
  subprocess.run(
    [
      INKSCAPE,
      "--export-type=svg",
      "--export-area-drawing",
      "--export-plain-svg",
      "--export-overwrite",
      f"--export-filename={str(file)}",
      str(file),
    ],
    stderr=subprocess.DEVNULL,
    check=True,
  )


def kicad_export(
  path: str | Path,
  export_format: str,
  output: str | Path,
  *args,
  export_type="pcb",
  theme: str | None = None,
) -> None:
  args = [*args, "--theme", theme] if theme else args
  subprocess.run(
    [KICAD_CLI, export_type, "export", export_format, "--output", str(output), *args, str(path)],
    check=True,
  )


def step_cad() -> None:
  """Export 3D models (.step / .glb) from the CAD notebooks."""
  import import_ipynb  # type: ignore

  from cad.case import PCB, case, case_top, case_bottom  # type: ignore[import]
  from cad.encoder_knob import encoder_knob  # type: ignore[import]
  from cad.slider_knob import slider_knob, slider_knob_body, slider_knob_insert  # type: ignore[import]
  from cad.power_switch_cap import power_switch_cap  # type: ignore[import]
  from cad.assembly import create_assembly  # type: ignore[import]

  from build123d import export_step, Unit
  from build123d.exporters3d import _create_xde
  from cad.cad_utils import export_gltf_doc

  MODELS_DIR.mkdir(parents=True, exist_ok=True)

  assembly = create_assembly(PCB, slider_knob, encoder_knob, power_switch_cap, case)

  models = [case, case_top, case_bottom, encoder_knob, slider_knob, slider_knob_body, slider_knob_insert, power_switch_cap, assembly]
  for model in models:
    print(f"  {model.label}...")
    export_step(model, MODELS_DIR / f"{model.label}.step")
    doc = _create_xde(model, Unit.MM, auto_naming=True)
    export_gltf_doc(doc, MODELS_DIR / f"{model.label}.glb", scale=100)


def step_pcb() -> None:
  """Export the PCB as a glTF model (pcb.glb) for the 3D viewer."""
  from cad.cad_utils import export_gltf_doc, load_pcb_doc

  pcb_doc = load_pcb_doc(PCB_NAME, PCB_DIR / PCB_NAME, PCB_EXPORT_DIR)
  export_gltf_doc(pcb_doc, MODELS_DIR / "pcb.glb", scale=100)


def step_bom() -> None:
  """Export the bill of materials (bom.csv) from the schematic."""
  kicad_export(
    SCH_FILE,
    "bom",
    BOM_FILE,
    "--fields",
    "Reference,${QUANTITY},Value,Value_ALT,Description,Source_EU,Source_US",
    "--labels",
    "Reference,Quantity,Value,Value_ALT,Description,Source_EU,Source_US",
    "--group-by",
    "Value,Footprint",
    "--field-delimiter",
    ";",
    export_type="sch",
  )


def _export_svg_pair(
  label: str,
  output_dir: Path,
  export_fn,  # callable(output, theme)
) -> None:
  output_dir.mkdir(parents=True, exist_ok=True)
  for theme_name, theme_arg in [("light", None), ("dark", DARK_THEME)]:
    print(f"  {label} ({theme_name})...")
    out = output_dir / f"{theme_name}.svg"
    export_fn(out, theme_arg)
    inkscape_crop_svg(out)


def _export_svg_sch_fn(out: Path, theme: str | None) -> None:
  kicad_export(
    SCH_FILE,
    "svg",
    out.parent,
    "--no-background-color",
    "--exclude-drawing-sheet",
    export_type="sch",
    theme=theme,
  )
  # sch export svg has no --mode-single
  (out.parent / f"{PCB_NAME}.svg").rename(out)


def _export_svg_pcb(layers: str):
  def export_fn(out: Path, theme: str | None) -> None:
    kicad_export(
      PCB_FILE,
      "svg",
      out,
      "--layers",
      layers,
      "--mode-single",
      "--fit-page-to-board",
      "--exclude-drawing-sheet",
      theme=theme,
    )

  return export_fn


def step_svg() -> None:
  """Export schematic and PCB SVGs via kicad-cli, then crop with Inkscape."""
  _export_svg_pair("schematic", SVG_DIR / "schematic", _export_svg_sch_fn)
  _export_svg_pair("PCB front", SVG_DIR / "front", _export_svg_pcb(PCB_FRONT_LAYERS))
  _export_svg_pair("PCB back", SVG_DIR / "back", _export_svg_pcb(PCB_BACK_LAYERS))


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

STEP_FNS = {
  "cad": step_cad,
  "pcb": step_pcb,
  "bom": step_bom,
  "svg": step_svg,
}

ALL_STEPS = list(STEP_FNS.keys())


def main() -> None:
  parser = argparse.ArgumentParser(
    description=__doc__,
    formatter_class=argparse.RawDescriptionHelpFormatter,
  )
  parser.add_argument(
    "steps",
    nargs="*",
    choices=ALL_STEPS,
    metavar="step",
    help=f"Steps to run (default: all). Choices: {', '.join(ALL_STEPS)}",
  )
  args = parser.parse_args()
  steps = args.steps or ALL_STEPS

  MODELS_DIR.mkdir(parents=True, exist_ok=True)

  for step in steps:
    print(f"==> {step}")
    STEP_FNS[step]()
    print()

  print("Done.")


if __name__ == "__main__":
  main()
