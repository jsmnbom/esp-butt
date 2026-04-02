# %%
import math

from build123d import (
  Align,
  Axis,
  BasePartObject,
  BuildPart,
  Cylinder,
  Edge,
  Face,
  GeomType,
  Mode,
  Plane,
  RadiusArc,
  RotationLike,
  ShapeList,
  Shell,
  Solid,
  Wire,
)
from build123d.build_common import validate_inputs
from build123d.topology.shape_core import Shape, downcast, get_top_level_topods_shapes
from build123d.topology.utils import tuplify
from cad_utils import fast_copy
from OCP.BRepAlgoAPI import BRepAlgoAPI_Splitter
from OCP.gp import gp_Trsf, gp_Vec
from OCP.TopLoc import TopLoc_Location
from OCP.TopoDS import TopoDS_Shape, TopoDS_Wire
from OCP.TopTools import TopTools_ListOfShape
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeFace

# %%

DEG2RAD = math.pi / 180


def z_rot_offset(shape, angle: float, offset: float):
  shape = fast_copy(shape)
  transformation = gp_Trsf()
  transformation.SetRotation(Axis.Z.wrapped, angle * DEG2RAD)
  transformation.SetTranslationPart(gp_Vec(0, 0, offset))
  shape.wrapped.Move(TopLoc_Location(transformation))
  return shape


def list_of_shape(shapes: list[Shape | TopoDS_Shape]) -> TopTools_ListOfShape:
  shape_list = TopTools_ListOfShape()
  for s in shapes:
    shape_list.Append(s.wrapped if isinstance(s, Shape) else s)
  return shape_list


def wire_to_face(wire: Wire) -> Face:
  builder = BRepBuilderAPI_MakeFace(wire.wrapped if isinstance(wire, Wire) else wire)
  if not builder.IsDone():
    raise ValueError("Failed to create face from wire")
  return Face(downcast(builder.Shape()))


class KnurledCylinder(BasePartObject):
  _applies_to = [BuildPart._tag]

  def __init__(
    self,
    radius: float,
    height: float,
    knurls: int,
    knurl_depth: float,
    flatten_top_knurls: bool = False,
    rotation: RotationLike = (0, 0, 0),
    align: Align | tuple[Align, Align, Align] = (
      Align.CENTER,
      Align.CENTER,
      Align.CENTER,
    ),
    mode: Mode = Mode.ADD,
  ):
    context: BuildPart | None = BuildPart._get_context(self)
    validate_inputs(context, self)

    self.cylinder_radius = radius
    self.cylinder_height = height
    self.knurls = knurls
    self.knurl_depth = knurl_depth
    self.flatten_top_knurls = flatten_top_knurls

    self.pitch = self.cylinder_height * 2
    self.step_angle = 360 / self.knurls
    self.inset = math.sqrt(knurl_depth) / 2
    self.single_knurl_height = self.cylinder_height / self.knurls
    self.circumference = 2 * math.pi * self.cylinder_radius
    self.step = 360 * (math.sqrt(knurl_depth) / self.circumference)
    self.hstep = self.cylinder_height * math.sqrt(knurl_depth) / (math.pi * self.cylinder_radius)

    self.step_angles = [self.step_angle * j for j in range(self.knurls)]

    self.helix_edges_cw = self.get_helix_edges()
    self.helix_edges_ccw = self.get_helix_edges(True)

    self.inner_faces: ShapeList[Face] = ShapeList()
    self.outer_faces: ShapeList[Face] = ShapeList()
    self.bottom_face: Face | None = None
    self.top_face: Face | None = None

    self.base_outer_cylinder = Cylinder(
      self.cylinder_radius,
      self.cylinder_height,
      align=(Align.CENTER, Align.CENTER, Align.MIN),
      mode=Mode.PRIVATE,
    ).rotate(Axis.Z, 180)
    self.base_outer_cylinder_face = self.base_outer_cylinder.faces().filter_by(GeomType.CYLINDER).first

    self.base_inner_cylinder = Cylinder(
      self.cylinder_radius - self.inset,
      self.cylinder_height,
      align=(Align.CENTER, Align.CENTER, Align.MIN),
      mode=Mode.PRIVATE,
    ).rotate(Axis.Z, 180)
    self.base_inner_cylinder_face = self.base_inner_cylinder.faces().filter_by(GeomType.CYLINDER).first

    solid = self.create_solid()

    super().__init__(part=solid, rotation=rotation, align=tuplify(align, 3), mode=mode)

  def build_inner_faces(self):
    helix_inner_faces_cw = self.get_inner_faces(self.helix_edges_cw)
    helix_inner_faces_ccw = self.get_inner_faces(self.helix_edges_ccw)

    # Build helix inner faces for all knurl positions
    for i in range(self.knurls):
      z_offset = i * self.single_knurl_height
      angle_offset = self.step_angle / 2 if i % 2 else 0
      for base_angle in self.step_angles:
        angle = base_angle + angle_offset
        if not (self.flatten_top_knurls and i == self.knurls - 1):
          self.inner_faces.append(z_rot_offset(helix_inner_faces_cw[0], angle, z_offset))
        self.inner_faces.append(z_rot_offset(helix_inner_faces_cw[1], angle, z_offset))
        self.inner_faces.append(z_rot_offset(helix_inner_faces_ccw[0], angle, z_offset))
        if not (self.flatten_top_knurls and i == self.knurls - 1):
          self.inner_faces.append(z_rot_offset(helix_inner_faces_ccw[1], angle, z_offset))

        # for faces in (helix_inner_faces_cw, helix_inner_faces_ccw):
        #   self.inner_faces.extend(z_rot_offset(f, angle, z_offset) for f in faces)

  def build_outer_faces(self):
    cylinder_outer_face = self.get_outer_face()

    # Build cylinder outer faces for remaining rows
    for i in range(1, self.knurls):
      z_offset = (i - 1) * self.single_knurl_height
      angle_offset = self.step_angle / 2 if not i % 2 else 0
      for base_angle in self.step_angles:
        self.outer_faces.append(
          z_rot_offset(cylinder_outer_face, base_angle + angle_offset, z_offset)
        )
  
  def build_bottom_top_faces(self):
    cylinder_outer_face_bottom = self.get_outer_face(bottom=True)
    cylinder_outer_face_top = self.get_outer_face(top=True)

    cylinder_bottom_edge = cylinder_outer_face_bottom.edges().sort_by(Axis.Z).first
    bottom_edges = self.inner_faces.edges().group_by(Axis.Z)[0]

    cylinder_top_edge = cylinder_outer_face_top.edges().sort_by(Axis.Z).last
    top_edges = [] if self.flatten_top_knurls else self.inner_faces.edges().group_by(Axis.Z)[-1]

    # Build cylinder bottom edges and outer faces (only needed once at i=0)
    for angle in self.step_angles:
      bottom_edges.append(z_rot_offset(cylinder_bottom_edge, angle, 0))
      top_edges.append(z_rot_offset(cylinder_top_edge, angle, 0))
      self.outer_faces.append(z_rot_offset(cylinder_outer_face_bottom, angle, 0))
      self.outer_faces.append(z_rot_offset(cylinder_outer_face_top, angle, 0))

    self.bottom_face = wire_to_face(Wire(bottom_edges))
    self.top_face = wire_to_face(Wire(top_edges))
    self.top_face.wrapped.Reverse()

  def create_solid(self) -> Solid:
    self.build_inner_faces()
    self.build_bottom_top_faces()
    self.build_outer_faces()

    all_faces = [
      *self.inner_faces,
      *self.outer_faces,
      self.bottom_face,
      self.top_face,
    ]
    return Solid(Shell(all_faces))

  def get_helix_edges(self, lefthand=False):
    outer_height = self.single_knurl_height - self.hstep
    inner_radius = self.cylinder_radius - self.inset
    inner = Edge.make_helix(self.pitch, self.single_knurl_height, inner_radius, lefthand=lefthand)
    left = Edge.make_helix(self.pitch, outer_height, self.cylinder_radius, lefthand=lefthand)
    left = z_rot_offset(left, -self.step if lefthand else 0, 0 if lefthand else self.hstep)
    right = Edge.make_helix(self.pitch, outer_height, self.cylinder_radius, lefthand=lefthand)
    right = z_rot_offset(right, 0 if lefthand else self.step, self.hstep if lefthand else 0)
    return inner.edge(), left.edge(), right.edge()

  def get_inner_faces(self, edges):
    inner, left, right = edges
    return ShapeList(
      [
        Face.make_surface_from_curves(inner, left),
        Face.make_surface_from_curves(right, inner),
      ]
    )

  def get_cylinder_face_piece(self, wire: TopoDS_Wire, inner=False):
    splitter = BRepAlgoAPI_Splitter()
    splitter.SetArguments(list_of_shape([self.base_inner_cylinder_face if inner else self.base_outer_cylinder_face]))
    splitter.SetTools(list_of_shape([wire]))
    splitter.Build()

    pieces = [Face(s) for s in get_top_level_topods_shapes(downcast(splitter.Shape()))]
    return min(pieces, key=lambda f: f.area)

  def get_outer_face(self, *, bottom=False, top=False):
    inner_cw, left_cw, right_cw = self.helix_edges_cw
    inner_ccw, left_ccw, right_ccw = self.helix_edges_ccw
    if bottom or (top and not self.flatten_top_knurls):
      edges = [
        right_cw,
        z_rot_offset(left_ccw, self.step_angle, 0),
      ]
      edges.append(
        RadiusArc(
          edges[1].start_point(),
          edges[0].start_point(),
          self.cylinder_radius,
          mode=Mode.PRIVATE,
        )
      )
      face = self.get_cylinder_face_piece(Wire._make_wire(edges))
    elif self.flatten_top_knurls and top:
      edges = [
        inner_cw,
        z_rot_offset(inner_ccw, self.step_angle, 0),
      ]
      edges.append(
        RadiusArc(
          edges[1].start_point(),
          edges[0].start_point(),
          self.cylinder_radius,
          mode=Mode.PRIVATE,
        )
      )
      face = self.get_cylinder_face_piece(Wire._make_wire(edges), inner=True)
    else:
      edges = [
        left_cw,
        right_ccw,
        z_rot_offset(left_ccw, self.step_angle / 2, self.single_knurl_height),
        z_rot_offset(right_cw, -self.step_angle / 2, self.single_knurl_height),
      ]
      face = self.get_cylinder_face_piece(Wire._make_wire(edges))

    if top:
      face.wrapped.Reverse()
      face = face.mirror(Plane.XY)
      face = z_rot_offset(face, 0, self.cylinder_height)
    return face

if __name__ == "__main__":
  KnurledCylinder(
    radius=10,
    height=16,
    knurls=16,
    knurl_depth=2,
    align=(Align.CENTER, Align.CENTER, Align.MIN),
  )
