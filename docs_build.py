#!/usr/bin/env -S uv run
"""
Build documentation assets: 3D models (CAD), PCB glTF, BOM, and PCB SVGs.

Usage:
    ./docs_build.py              # run all steps
    ./docs_build.py cad svg      # run specific steps (in order)

Steps:
  cad   Export 3D models (.step / .glb) from CAD notebooks
  pcb   Export PCB as glTF (pcb.glb) for the 3D viewer
  bom   Export bill of materials (bom.csv) from the schematic
  svg   Export schematic and PCB SVGs from KiCad via kicad-cli / Inkscape
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent
DOCS_PUBLIC = REPO_ROOT / "docs" / "public"
MODELS_DIR = DOCS_PUBLIC / "models"
SVG_DIR = REPO_ROOT / "docs" / "svg"
CAD_DIR = REPO_ROOT / "cad"
PCB_DIR = REPO_ROOT / "pcb"

PCB_NAME = "esp-butt"
DARK_THEME = "witchhazel"

PCB_FRONT_LAYERS = "F.Cu,F.Mask,F.Silkscreen,Edge.Cuts"
PCB_BACK_LAYERS = "B.Cu,B.Mask,B.Silkscreen,Edge.Cuts"

ALL_STEPS = ["cad", "pcb", "bom", "svg"]

KICAD_CLI = os.getenv("KICAD_CLI", "kicad-cli")
INKSCAPE = os.getenv("INKSCAPE", "inkscape")


def pre_cad_utils_import():
  # Must be set before importing cad_utils so show/viewer calls become no-ops.
  os.environ["BUILDING_DOCS"] = "1"
  # Prevent the individual notebooks from writing versioned .step exports.
  os.environ["NO_CAD_EXPORT"] = "1"

  cad_str = str(CAD_DIR)
  if cad_str not in sys.path:
    sys.path.insert(0, cad_str)


def inkscape_crop_svg(file: Path) -> None:
  """Crop an SVG in-place to its drawing bounds using Inkscape."""
  fd, tmp_path = tempfile.mkstemp(suffix=".svg")
  os.close(fd)
  try:
    with open(file, "rb") as src, open(tmp_path, "wb") as dst:
      subprocess.run(
        [
          INKSCAPE,
          "--pipe",
          "--export-type=svg",
          "--export-area-drawing",
          "--export-plain-svg",
        ],
        stdin=src,
        stdout=dst,
        stderr=subprocess.DEVNULL,
        check=True,
      )
    shutil.move(tmp_path, file)
  except Exception:
    Path(tmp_path).unlink(missing_ok=True)
    raise


def _kicad_env() -> dict[str, str]:
  config_home = os.environ.get("KICAD_CONFIG_HOME", str(PCB_DIR / "kicad-config"))
  return {**os.environ, "KICAD_CONFIG_HOME": config_home}


def _kicad_export(
  path: str | Path,
  export_format: str,
  output: str | Path,
  *args,
  export_type="pcb",
  theme: str | None = None,
) -> None:
  args = [*args, "--theme", theme] if theme else args
  p = subprocess.run(
    [KICAD_CLI, export_type, "export", export_format, "--output", str(output), *args, str(path)],
    capture_output=True,
    env=_kicad_env(),
  )
  if p.returncode != 0:
    print(p.stdout.decode())
    raise RuntimeError(f"Failed to export PCB: {p.stderr.decode()}")


def step_cad() -> None:
  """Export 3D models (.step / .glb) from the CAD notebooks."""
  pre_cad_utils_import()

  # Notebooks use relative paths (e.g. "../pcb/…"), so cwd must be cad/.
  orig_cwd = Path.cwd()
  os.chdir(CAD_DIR)
  try:
    import import_ipynb  # noqa: F401 — registers the .ipynb importer

    from case import case, case_top, case_bottom  # type: ignore[import]
    from encoder_knob import encoder_knob  # type: ignore[import]
    from slider_knob import slider_knob  # type: ignore[import]
    from power_switch_cap import power_switch_cap  # type: ignore[import]
  finally:
    os.chdir(orig_cwd)

  from build123d import Unit, export_gltf, export_step

  MODELS_DIR.mkdir(parents=True, exist_ok=True)

  models = [case, case_top, case_bottom, encoder_knob, slider_knob, power_switch_cap]
  for model in models:
    print(f"  {model.label}...")
    export_step(model, MODELS_DIR / f"{model.label}.step")
    export_gltf(model, str(MODELS_DIR / f"{model.label}.glb"), Unit.MM, True, 0.01, 0.25)


def step_pcb() -> None:
  """Export the PCB as a glTF model (pcb.glb) for the 3D viewer."""
  pre_cad_utils_import()
  from cad_utils import export_gltf_doc, load_pcb_doc

  MODELS_DIR.mkdir(parents=True, exist_ok=True)

  print(f"  {PCB_NAME}.glb...")
  pcb_doc = load_pcb_doc(PCB_NAME, PCB_DIR / PCB_NAME, PCB_DIR / "export")
  export_gltf_doc(pcb_doc, MODELS_DIR / "pcb.glb")


def step_bom() -> None:
  """Export the bill of materials (bom.csv) from the schematic."""
  DOCS_PUBLIC.mkdir(parents=True, exist_ok=True)

  print("  bom.csv...")
  _kicad_export(
    PCB_DIR / PCB_NAME / f"{PCB_NAME}.kicad_sch",
    "bom",
    DOCS_PUBLIC / "bom.csv",
    "--fields",
    "Reference,${QUANTITY},Value,Value_ALT,Source_EU,Source_US",
    "--labels",
    "Reference,Quantity,Value,Value_ALT,Source_EU,Source_US",
    "--group-by",
    "Value,Footprint",
    "--field-delimiter",
    ";",
    export_type="sch",
  )


# ---------------------------------------------------------------------------
# Step: svg
# ---------------------------------------------------------------------------


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


def step_svg() -> None:
  """Export schematic and PCB SVGs via kicad-cli, then crop with Inkscape."""
  sch = PCB_DIR / PCB_NAME / f"{PCB_NAME}.kicad_sch"
  pcb = PCB_DIR / PCB_NAME / f"{PCB_NAME}.kicad_pcb"

  # Schematic — kicad-cli writes <name>.svg into the output dir; rename to light/dark.
  def export_sch(out: Path, theme: str | None) -> None:
    _kicad_export(
      sch,
      "svg",
      out.parent,
      "--no-background-color",
      "--exclude-drawing-sheet",
      export_type="sch",
      theme=theme,
    )
    (out.parent / f"{PCB_NAME}.svg").rename(out)

  def export_front(out: Path, theme: str | None) -> None:
    _kicad_export(
      pcb,
      "svg",
      out,
      "--layers",
      PCB_FRONT_LAYERS,
      "--mode-single",
      "--fit-page-to-board",
      "--exclude-drawing-sheet",
      theme=theme,
    )

  def export_back(out: Path, theme: str | None) -> None:
    _kicad_export(
      pcb,
      "svg",
      out,
      "--layers",
      PCB_BACK_LAYERS,
      "--mode-single",
      "--fit-page-to-board",
      "--mirror",
      "--exclude-drawing-sheet",
      theme=theme,
    )

  _export_svg_pair("schematic", SVG_DIR / "schematic", export_sch)
  _export_svg_pair("PCB front", SVG_DIR / "front", export_front)
  _export_svg_pair("PCB back", SVG_DIR / "back", export_back)


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

STEP_FNS = {"cad": step_cad, "pcb": step_pcb, "bom": step_bom, "svg": step_svg}


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

  for step in steps:
    print(f"==> {step}")
    STEP_FNS[step]()
    print()

  print("Done.")


if __name__ == "__main__":
  main()
