"""Small, deterministic fixtures for exercising Manim's public constructors."""

from __future__ import annotations

import inspect
from pathlib import Path
from typing import Any

import numpy as np
import svgelements as se
from manim import (
    ArcBetweenPoints,
    Circle,
    Dot,
    DOWN,
    LEFT,
    Line,
    MovingCamera,
    ORIGIN,
    RIGHT,
    Square,
    VMobject,
    UP,
)
from manim.mobject.types.point_cloud_mobject import Point as PMPoint
from manim.mobject.opengl.opengl_point_cloud_mobject import OpenGLPMPoint
from manim.mobject.opengl.opengl_surface import OpenGLSurface
from manim.mobject.opengl.opengl_geometry import OpenGLCircle


ROOT = Path(__file__).resolve().parents[2]
REFERENCE_IMAGES = (
    ROOT
    / "benchmarks"
    / "corpus"
    / "reference"
    / "images"
    / "manim_reference_scenes"
)
LIGHT_IMAGE = REFERENCE_IMAGES / "PrimitiveGallery_ManimCE_v0.20.1.png"
DARK_IMAGE = REFERENCE_IMAGES / "TextAndMath_ManimCE_v0.20.1.png"
SVG_FIXTURE = ROOT / "benchmarks" / "corpus" / "fixture.svg"


class UpstreamConstructorFailure(RuntimeError):
    """The pinned Manim release itself cannot construct this public class."""


def _lines() -> tuple[Line, Line]:
    return Line(LEFT, RIGHT), Line(DOWN, UP)


def _opengl_curve() -> OpenGLCircle:
    return OpenGLCircle(radius=0.8)


def _special_fixture(candidate: type[Any], name: str) -> Any | None:
    line1, line2 = _lines()
    points_2d = [LEFT, UP, RIGHT, DOWN]

    if name == "ArrowTip":
        raise UpstreamConstructorFailure("ArrowTip is an intentionally unimplemented base.")
    if name in {"OpenGLElbow", "OpenGLRoundedRectangle"}:
        raise UpstreamConstructorFailure(
            f"Manim 0.20.1 {name} fails under its supported NumPy runtime."
        )
    if name == "Code":
        return candidate(code_string="x = 2 + 2", language="python")
    if name == "ArcPolygon":
        return candidate(*points_2d)
    if name == "ArcPolygonFromArcs":
        return candidate(
            ArcBetweenPoints(LEFT, UP),
            ArcBetweenPoints(UP, RIGHT),
            ArcBetweenPoints(RIGHT, LEFT),
        )
    if name == "BulletedList":
        return candidate("first", "second")
    if name == "ConvexHull":
        return candidate(*points_2d)
    if name == "ConvexHull3D":
        return candidate(
            [-1, -1, -1],
            [1, -1, -1],
            [0, 1, -1],
            [0, 0, 1],
        )
    if name in {"Intersection", "Union"}:
        return candidate(Circle(radius=0.8).shift(LEFT * 0.3), Circle(radius=0.8).shift(RIGHT * 0.3))
    if name == "LabeledArrow":
        return candidate("v", start=LEFT, end=RIGHT)
    if name in {"OpenGLPolygon", "Polygon"}:
        return candidate(*points_2d)
    if name == "SVGMobject":
        return candidate(SVG_FIXTURE)
    if name == "ImageMobject":
        return candidate(LIGHT_IMAGE)
    if name == "ImageMobjectFromCamera":
        return candidate(MovingCamera())
    if name == "OpenGLImageMobject":
        return candidate(LIGHT_IMAGE, width=2.0)
    if name == "OpenGLTexturedSurface":
        surface = OpenGLSurface(
            lambda u, v: [u, v, 0.15 * np.sin(2 * u) * np.cos(2 * v)],
            u_range=(-1, 1),
            v_range=(-0.7, 0.7),
            resolution=(6, 5),
        )
        textured = candidate(surface, LIGHT_IMAGE, dark_image_file=DARK_IMAGE)
        textured.resolution = surface.resolution
        textured.compute_triangle_indices()
        textured.texture_paths = {
            "LightTexture": str(LIGHT_IMAGE),
            "DarkTexture": str(DARK_IMAGE),
        }
        return textured
    if name == "OpenGLSurfaceMesh":
        return candidate(
            OpenGLSurface(
                lambda u, v: [u, v, 0.12 * np.sin(2 * u)],
                u_range=(-1, 1),
                v_range=(-0.7, 0.7),
                resolution=(6, 5),
            ),
            resolution=(6, 5),
        )
    if name == "Polyhedron":
        return candidate(
            [[-1, -1, 0], [1, -1, 0], [0, 1, 0], [0, 0, 1.4]],
            [[0, 1, 2], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        )
    if name == "LabeledPolygram":
        return candidate(points_2d, label="P")
    if name == "VMobjectFromSVGPath":
        return candidate(se.Path("M -1 0 C -0.5 1 0.5 -1 1 0"))
    if name == "MobjectMatrix":
        return candidate([[Circle(radius=0.2), Square(side_length=0.4)]])
    if name == "MobjectTable":
        return candidate([[Dot(), Square(side_length=0.4)]])
    if name == "DecimalTable":
        return candidate([[1.25, 2.5], [3.75, 4.0]])
    if name == "IntegerTable":
        return candidate([[1, 2], [3, 4]])
    if name in {"Difference", "Exclusion"}:
        return candidate(Circle(), Square(side_length=1.2))
    if name == "Cutout":
        return candidate(Square(side_length=2), Circle(radius=0.5))
    if name == "MathTex":
        return candidate(r"x^2 + y^2")
    if name == "Tex":
        return candidate(r"$x^2 + y^2$")
    if name == "Paragraph":
        return candidate("first line", "second line")
    if name == "Polygram":
        return candidate(points_2d)
    if name in {"Group", "Mobject"}:
        return candidate().add(Circle())
    if name in {"Mobject1D", "Mobject2D", "PMobject"}:
        return candidate().add_points(
            [[-0.5, 0, 0], [0, 0.5, 0], [0.5, 0, 0]],
            color="#58c4dd",
        )
    if name == "PGroup":
        return candidate(PMPoint(LEFT), PMPoint(RIGHT))
    if name in {"ThreeDVMobject", "TipableVMobject", "VMobject"}:
        return candidate().set_points_as_corners([LEFT, UP, RIGHT, DOWN, LEFT])
    if name == "VectorField":
        return candidate(
            lambda point: np.array([-point[1], point[0], 0.0])
        ).set_points_as_corners([LEFT, UP, RIGHT, DOWN, LEFT])
    if name == "TracedPath":
        return candidate(lambda: ORIGIN).set_points_as_corners([LEFT, UP, RIGHT])
    if name == "VDict":
        return candidate({"circle": Circle()})
    if name == "VGroup":
        return candidate(Circle(), Square(side_length=0.8))
    if name in {"OpenGLGroup", "OpenGLMobject"}:
        return candidate().add(_opengl_curve())
    if name in {"OpenGLPMobject"}:
        return candidate().set_points(
            np.asarray([[-0.5, 0, 0], [0, 0.5, 0], [0.5, 0, 0]])
        )
    if name == "OpenGLPGroup":
        return candidate(OpenGLPMPoint(LEFT), OpenGLPMPoint(RIGHT))
    if name == "OpenGLSurfaceGroup":
        return candidate(
            OpenGLSurface(
                lambda u, v: [u, v, 0.1 * np.sin(u)],
                resolution=(5, 5),
            )
        )
    if name in {"OpenGLTipableVMobject", "OpenGLVMobject"}:
        return candidate().set_points_as_corners([LEFT, UP, RIGHT, DOWN, LEFT])
    if name == "OpenGLVGroup":
        return candidate(_opengl_curve())
    if name == "TangentialArc":
        return candidate(line1, line2, 0.35)
    return None


def construct_fixture(candidate: type[Any]) -> Any:
    """Instantiate a public Manim mobject with a minimal meaningful input."""

    name = candidate.__name__
    special = _special_fixture(candidate, name)
    if special is not None:
        return special

    signature = inspect.signature(candidate)
    required = [
        parameter
        for parameter in signature.parameters.values()
        if parameter.default is inspect.Parameter.empty
        and parameter.kind
        not in (inspect.Parameter.VAR_POSITIONAL, inspect.Parameter.VAR_KEYWORD)
    ]
    if not required:
        return candidate()

    line1, line2 = _lines()
    values: dict[str, Any] = {
        "line1": line1,
        "line2": line2,
        "vmobject": Circle(),
        "vmob": Circle(),
        "mobject": Circle(),
        "obj": Circle(),
        "start": LEFT,
        "end": RIGHT,
        "start_point": LEFT,
        "end_point": RIGHT,
        "point_1": LEFT,
        "point_2": RIGHT,
        "start_anchor": LEFT,
        "start_handle": LEFT + UP,
        "end_handle": RIGHT + DOWN,
        "end_anchor": RIGHT,
        "a0": LEFT,
        "h0": LEFT + UP,
        "h1": RIGHT + DOWN,
        "a1": RIGHT,
        "values": [1, 2, 3],
        "vertices": [0, 1, 2],
        "edges": [(0, 1), (1, 2)],
        "matrix": [[1, 2], [3, 4]],
        "table": [["1", "2"], ["3", "4"]],
        "label": "x",
        "text": "x",
        "tex_string": r"x^2",
        "num_vertices": 5,
        "var": 1.5,
        "alpha": 0.4,
        "traced_point_func": lambda: ORIGIN,
        "path_obj": se.Path("M -1 0 L 1 0"),
        "filename_or_array": LIGHT_IMAGE,
        "image_file": LIGHT_IMAGE,
        "camera": MovingCamera(),
        "uv_surface": OpenGLSurface(
            lambda u, v: [u, v, 0],
            u_range=(-1, 1),
            v_range=(-1, 1),
            resolution=(5, 5),
        ),
        "vertex_coords": [[-1, -1, 0], [1, -1, 0], [0, 1, 0], [0, 0, 1]],
        "faces_list": [[0, 1, 2], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
        "radius": 0.35,
    }
    if candidate.__module__.startswith("manim.mobject.opengl"):
        values["vmobject"] = _opengl_curve()
        values["vmob"] = _opengl_curve()

    if name in {"ArrowVectorField", "VectorField", "StreamLines"}:
        values["func"] = lambda point: np.array([-point[1], point[0], 0.0])
    elif name == "ImplicitFunction":
        values["func"] = lambda x, y: x * x + y * y - 1
    elif name in {"Surface", "OpenGLSurface"}:
        values["func"] = lambda u, v: np.array([u, v, 0.2 * np.sin(u)])
    elif name in {"FunctionGraph"}:
        values["function"] = lambda x: np.sin(x)
    elif name in {"ParametricFunction"}:
        values["function"] = lambda t: np.array([np.cos(t), np.sin(t), 0.0])

    arguments = []
    keywords: dict[str, Any] = {}
    for parameter in required:
        if parameter.name not in values:
            raise KeyError(f"No constructor fixture for {name}.{parameter.name}.")
        if parameter.kind is inspect.Parameter.KEYWORD_ONLY:
            keywords[parameter.name] = values[parameter.name]
        else:
            arguments.append(values[parameter.name])
    return candidate(*arguments, **keywords)
