import copy
from pathlib import Path

from build123d import Location
from build123d.topology.shape_core import Shape, downcast


def fast_copy(shape: Shape) -> Shape:
  cls = shape.__class__
  result = cls.__new__(cls)
  for key, value in shape.__dict__.items():
    if key == "_wrapped":
      result._wrapped = downcast(shape.wrapped.Located(shape.wrapped.Location()))
    elif key == "wrapped":
      result.wrapped = downcast(shape.wrapped.Located(shape.wrapped.Location()))
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


def find_last_file(directory: str | Path, prefix: str, extension: str) -> tuple[Path, int] | None:
  directory = Path(directory)
  files = list(directory.glob(f"{prefix}*.{extension}"))
  if not files:
    return None
  files = [(f, int(f.stem.split(".v")[-1]) if f and ".v" in f.stem else 0) for f in files]
  files.sort(key=lambda x: x[1], reverse=True)
  return files[0]
