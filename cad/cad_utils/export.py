from .utils import find_last_file


from build123d import export_step
from build123d.topology.shape_core import Shape


import os
from datetime import datetime
from io import BytesIO
from pathlib import Path


def export(shape: Shape):
  if not shape.label:
    raise ValueError("Shape must have a label to be exported.")

  if os.getenv("NO_CAD_EXPORT", "0") == "1":
    print(f"NO_CAD_EXPORT is set, skipping export of '{shape.label}'")
    return

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
    last_step_data = last_any[0].read_bytes() if last_any else b""
    export_step(shape, step_data, timestamp=timestamp)
    if step_data.getvalue() == last_step_data:
      print(f"No changes detected for '{shape.label}', skipping export.")
      return

  # If we get here, we need to export a new file
  # Next version will be highest existing version + 1, or 1 if no existing files
  last_step_version = last_step[1] if last_step else 0
  last_any_version = last_any[1] if last_any else 0
  version = max(last_step_version, last_any_version) + 1

  # make sure we have data
  if step_data.getvalue() == b"":
    export_step(shape, step_data, timestamp=timestamp)

  new_file = export_dir / f"{shape.label}.v{version}.step"
  new_file.write_bytes(step_data.getvalue())
  print(f"Exported '{shape.label}' to {new_file}")
