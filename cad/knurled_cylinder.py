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


class KnurledCylinder(BasePartObject):
  _applies_to = [BuildPart._tag]

  def __init__(
    self,
    radius: float,
    height: float,
    knurls: int,
    knurl_depth: float,
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

    self.pitch = self.cylinder_height * 2
    self.step_angle = 360 / self.knurls
    self.inset = math.sqrt(knurl_depth) / 2
    self.single_knurl_height = self.cylinder_height / self.knurls
    self.circumference = 2 * math.pi * self.cylinder_radius
    self.step = 360 * (math.sqrt(knurl_depth) / self.circumference)
    self.hstep = self.cylinder_height * math.sqrt(knurl_depth) / (math.pi * self.cylinder_radius)

    self.base_cylinder = Cylinder(
      self.cylinder_radius,
      self.cylinder_height,
      align=(Align.CENTER, Align.CENTER, Align.MIN),
      mode=Mode.PRIVATE,
    ).rotate(Axis.Z, 180)
    self.base_cylinder_face = self.base_cylinder.faces().filter_by(GeomType.CYLINDER).first

    solid = self.create_solid()

    super().__init__(part=solid, rotation=rotation, align=tuplify(align, 3), mode=mode)

  def create_solid(self) -> Solid:
    helix_edges_cw = self.get_helix_edges()
    helix_edges_ccw = self.get_helix_edges(True)

    helix_inner_faces_cw = self.get_helix_inner_faces(helix_edges_cw)
    helix_inner_faces_ccw = self.get_helix_inner_faces(helix_edges_ccw)

    cylinder_outer_face = self.get_cylinder_outer_face(helix_edges_cw, helix_edges_ccw)
    cylinder_outer_face_bottom = self.get_cylinder_outer_face_bottom(
      helix_edges_cw, helix_edges_ccw
    )
    cylinder_outer_face_top = cylinder_outer_face_bottom.mirror(Plane.XY).translate(
      (0, 0, self.cylinder_height)
    )
    cylinder_outer_face_top.wrapped.Reverse()

    cylinder_bottom_edges = [
      helix_inner_faces_cw.edges().sort_by(Axis.Z).first,
      cylinder_outer_face_bottom.edges().sort_by(Axis.Z).first,
      helix_inner_faces_ccw.edges().sort_by(Axis.Z).first.rotate(Axis.Z, self.step_angle),
    ]

    all_helix_inner_faces = []
    all_cylinder_bottom_edges = []
    all_cylinder_outer_faces = []

    for i in range(self.knurls):
      for j in range(self.knurls):
        for faces in (helix_inner_faces_cw, helix_inner_faces_ccw):
          all_helix_inner_faces.extend(
            z_rot_offset(
              f,
              self.step_angle * j + (self.step_angle / 2 if i % 2 else 0),
              i * self.single_knurl_height,
            )
            for f in faces
          )
        if i == 0:
          for e in cylinder_bottom_edges:
            all_cylinder_bottom_edges.append(e.rotate(Axis.Z, self.step_angle * j))
          all_cylinder_outer_faces.append(
            cylinder_outer_face_bottom.rotate(Axis.Z, self.step_angle * j)
          )
          all_cylinder_outer_faces.append(
            cylinder_outer_face_top.rotate(Axis.Z, self.step_angle * j)
          )
        else:
          all_cylinder_outer_faces.append(
            z_rot_offset(
              cylinder_outer_face,
              self.step_angle * j + (self.step_angle / 2 if not i % 2 else 0),
              (i - 1) * self.single_knurl_height,
            )
          )

    cylinder_bottom_face = Face.make_surface(Wire(all_cylinder_bottom_edges))
    cylinder_top_face = cylinder_bottom_face.mirror(Plane.XY).translate(
      (0, 0, self.cylinder_height)
    )
    cylinder_top_face.wrapped.Reverse()

    all_faces = [
      *all_helix_inner_faces,
      *all_cylinder_outer_faces,
      cylinder_bottom_face,
      cylinder_top_face,
    ]

    return Solid(Shell(all_faces))

  def get_cylinder_face_piece(self, wire: TopoDS_Wire):
    splitter = BRepAlgoAPI_Splitter()
    splitter.SetArguments(list_of_shape([self.base_cylinder_face]))
    splitter.SetTools(list_of_shape([wire]))
    splitter.Build()

    pieces = [Face(s) for s in get_top_level_topods_shapes(downcast(splitter.Shape()))]
    return min(pieces, key=lambda f: f.area)

  def get_helix_edges(self, lefthand=False):
    outer_height = self.single_knurl_height - self.hstep
    inner_radius = self.cylinder_radius - self.inset
    inner = Edge.make_helix(self.pitch, self.single_knurl_height, inner_radius, lefthand=lefthand)
    left = Edge.make_helix(self.pitch, outer_height, self.cylinder_radius, lefthand=lefthand)
    if lefthand:
      left = left.rotate(Axis.Z, -self.step)
    else:
      left = left.translate((0, 0, self.hstep))
    right = Edge.make_helix(self.pitch, outer_height, self.cylinder_radius, lefthand=lefthand)
    if lefthand:
      right = right.translate((0, 0, self.hstep))
    else:
      right = right.rotate(Axis.Z, self.step)
    return inner, left, right

  def get_helix_inner_faces(self, edges):
    inner, left, right = edges
    return ShapeList(
      [
        Face.make_surface_from_curves(inner, left),
        Face.make_surface_from_curves(right, inner),
      ]
    )

  def get_cylinder_face_piece(self, wire: TopoDS_Wire):
    splitter = BRepAlgoAPI_Splitter()
    splitter.SetArguments(list_of_shape([self.base_cylinder_face]))
    splitter.SetTools(list_of_shape([wire]))
    splitter.Build()

    pieces = [Face(s) for s in get_top_level_topods_shapes(downcast(splitter.Shape()))]
    return min(pieces, key=lambda f: f.area)

  def get_cylinder_outer_face(self, edges_cw, edges_ccw):
    _, left_cw, right_cw = edges_cw
    _, left_ccw, right_ccw = edges_ccw
    edges = [
      left_cw,
      right_ccw,
      z_rot_offset(left_ccw, self.step_angle / 2, self.single_knurl_height),
      z_rot_offset(right_cw, -self.step_angle / 2, self.single_knurl_height),
    ]
    return self.get_cylinder_face_piece(Wire._make_wire(edges))

  def get_cylinder_outer_face_bottom(self, edges_cw, edges_ccw):
    _, _, right_cw = edges_cw
    _, left_ccw, _ = edges_ccw
    edges = [
      right_cw,
      left_ccw.rotate(Axis.Z, self.step_angle),
    ]
    edges.append(
      RadiusArc(
        edges[1].start_point(),
        edges[0].start_point(),
        self.cylinder_radius,
        mode=Mode.PRIVATE,
      )
    )
    return self.get_cylinder_face_piece(Wire._make_wire(edges))
