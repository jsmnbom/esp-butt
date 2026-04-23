import math
from os import PathLike, fsdecode

from OCP.BRepMesh import BRepMesh_IncrementalMesh
from OCP.gp import gp_Ax1, gp_Dir, gp_Pnt, gp_Trsf
from OCP.Message import Message_ProgressRange
from OCP.RWGltf import RWGltf_CafWriter
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.TCollection import TCollection_AsciiString, TCollection_ExtendedString
from OCP.TColStd import TColStd_IndexedDataMapOfStringString
from OCP.TDF import TDF_LabelSequence
from OCP.TDocStd import TDocStd_Document
from OCP.TopLoc import TopLoc_Location
from OCP.XCAFApp import XCAFApp_Application
from OCP.XCAFDoc import XCAFDoc_DocumentTool, XCAFDoc_Location, XCAFDoc_ShapeTool


def import_step_doc(file_path: PathLike | str | bytes) -> TDocStd_Document:
  app = XCAFApp_Application.GetApplication_s()
  doc = TDocStd_Document(TCollection_ExtendedString("BinXCAF"))
  app.NewDocument(TCollection_ExtendedString("BinXCAF"), doc)

  reader = STEPCAFControl_Reader()
  reader.SetColorMode(True)
  reader.SetNameMode(True)
  reader.SetLayerMode(True)
  status = reader.ReadFile(fsdecode(file_path))
  if status != 1:  # IFSelect_RetDone
    raise ValueError(f"Error reading STEP file: status={status}")
  reader.Transfer(doc)

  return doc


def export_gltf_doc(
  doc: TDocStd_Document,
  file_path: PathLike | str | bytes,
  linear_deflection: float = 0.01,
  angular_deflection: float = 0.25,
):
  shape_tool = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main())
  free_labels = TDF_LabelSequence()
  shape_tool.GetFreeShapes(free_labels)

  # Tessellate all free shapes in the document
  for i in range(1, free_labels.Length() + 1):
    shape = XCAFDoc_ShapeTool.GetShape_s(free_labels.Value(i))
    if not shape.IsNull():
      BRepMesh_IncrementalMesh(shape, linear_deflection, True, angular_deflection, True).Perform()

  # Map right-handed Z-up (STEP/CAD) → right-handed Y-up (glTF): -90° around X
  # RWMesh_CoordinateSystemConverter is unregistered in this OCP binding, so we
  # apply the rotation directly as an XCAFDoc_Location on each free shape label.
  trsf = gp_Trsf()
  trsf.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)), -math.pi / 2)
  for i in range(1, free_labels.Length() + 1):
    XCAFDoc_Location.Set_s(free_labels.Value(i), TopLoc_Location(trsf))

  writer = RWGltf_CafWriter(TCollection_AsciiString(fsdecode(file_path)), True)
  writer.SetMergeFaces(True)
  file_info = TColStd_IndexedDataMapOfStringString()
  ok = writer.Perform(doc, file_info, Message_ProgressRange())

  if not ok:
    raise RuntimeError("Failed to export glTF")
