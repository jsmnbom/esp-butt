import os

from .doc import export_gltf_doc, import_step_doc
from .export import export
from .kicad import PCBCompound, load_pcb, load_pcb_doc
from .knurled_cylinder import KnurledCylinder
from .utils import copy_located, fast_copy

__all__ = [
  "KnurledCylinder",
  "PCBCompound",
  "load_pcb",
  "fast_copy",
  "copy_located",
  "export",
  "show",
  "show_object",
  "reset_show",
  "set_port",
  "set_viewer_config",
  "import_step_doc",
  "export_gltf_doc",
  "load_pcb_doc",
]

BUILDING_DOCS = bool(os.getenv("BUILDING_DOCS"))
if BUILDING_DOCS:

  def show(*args, **kwargs):
    pass

  def show_object(*args, **kwargs):
    pass

  def reset_show(*args, **kwargs):
    pass

  def set_port(*args, **kwargs):
    pass

  def set_viewer_config(*args, **kwargs):
    pass
else:
  from ocp_vscode import reset_show, set_port, set_viewer_config, show, show_object  # noqa: F401
