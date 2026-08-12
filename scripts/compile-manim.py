#!/usr/bin/env python3
"""Compile a regular ManimCE Scene into the Rust retained scene IR.

This compatibility compiler deliberately runs Manim's own Python object and
animation semantics. It samples the resulting vector state at explicit frame
times, projects it through Manim's active camera, and emits cubic path tracks
that the Rust runtime can seek and play without Python.
"""

from __future__ import annotations

import argparse
import base64
import importlib.util
import json
import math
import mimetypes
import os
import random
import re
import sys
from dataclasses import dataclass, field, replace
from pathlib import Path
from typing import Any

import numpy as np
import moderngl
from PIL import Image
from manim import Scene, config
from manim.animation.changing import TracedPath
from manim.constants import RendererType
from manim.mobject.mobject import Mobject
from manim.mobject.value_tracker import ComplexValueTracker, ValueTracker
from manim.mobject.types.image_mobject import AbstractImageMobject
from manim.mobject.types.point_cloud_mobject import PMobject
from manim.mobject.types.vectorized_mobject import VMobject
from manim.mobject.three_d.three_dimensions import Surface
from manim.mobject.opengl.opengl_point_cloud_mobject import OpenGLPMobject
from manim.mobject.opengl.opengl_image_mobject import OpenGLImageMobject
from manim.mobject.opengl.opengl_surface import OpenGLSurface, OpenGLTexturedSurface
from manim.mobject.opengl.opengl_vectorized_mobject import OpenGLVMobject
from manim.mobject.opengl.opengl_mobject import OpenGLMobject
from manim.renderer.opengl_renderer import OpenGLCamera
from manim.renderer.shader_wrapper import get_shader_code_from_file
from manim.camera.three_d_camera import ThreeDCamera
from manim.camera.moving_camera import MovingCamera as ManimMovingCamera


OUTPUT_WIDTH = 16.0
OUTPUT_HEIGHT = 9.0
SHADER_TRANSLATOR: Any | None = None
REGISTERED_VALUE_TRACKERS: list[ValueTracker] = []
ORIGINAL_VALUE_TRACKER_INIT = ValueTracker.__init__


def registered_value_tracker_init(
    self: ValueTracker, value: float = 0, **kwargs: Any
) -> None:
    ORIGINAL_VALUE_TRACKER_INIT(self, value, **kwargs)
    caller_module = str(sys._getframe(1).f_globals.get("__name__", ""))
    if not caller_module.startswith("manim."):
        REGISTERED_VALUE_TRACKERS.append(self)


ValueTracker.__init__ = registered_value_tracker_init


def translate_shader_program(
    vertex_source: str,
    fragment_source: str,
    attributes: list[str],
) -> dict[str, Any]:
    global SHADER_TRANSLATOR
    if SHADER_TRANSLATOR is None:
        translator_path = Path(__file__).with_name("translate-manim-shader.py")
        spec = importlib.util.spec_from_file_location(
            "_realtime_manim_shader_translator", translator_path
        )
        if spec is None or spec.loader is None:
            raise RuntimeError(f"Could not load {translator_path}.")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        SHADER_TRANSLATOR = module
    return SHADER_TRANSLATOR.translate_program(
        vertex_source, fragment_source, attributes
    )


def finite(value: float) -> float:
    value = float(value)
    return round(value, 6) if math.isfinite(value) else 0.0


def enum_member_name(value: Any) -> str:
    """Return a stable enum member name without importing optional backends."""

    return str(getattr(value, "name", value)).rsplit(".", 1)[-1].upper()


def retained_stroke_cap(mobject: Any) -> str | None:
    """Map ManimCE cap styles to the retained scene vocabulary.

    Cairo's untouched line-cap default is BUTT. OpenGLVMobject does not expose
    cap_style in ManimCE 0.20, so the same retained default applies there.
    """

    return {
        "AUTO": "butt",
        "BUTT": "butt",
        "SQUARE": "square",
        "ROUND": "round",
    }.get(enum_member_name(getattr(mobject, "cap_style", "AUTO")))


def retained_stroke_join(mobject: Any) -> str | None:
    """Map ManimCE joint styles to the retained scene vocabulary.

    Manim's Cairo renderer leaves AUTO at Cairo's MITER default. The retained
    renderer has no backend-specific AUTO mode, so MITER is also the stable
    cross-backend fallback for OpenGLVMobject.
    """

    return {
        "AUTO": "miter",
        "MITER": "miter",
        "ROUND": "round",
        "BEVEL": "bevel",
    }.get(enum_member_name(getattr(mobject, "joint_type", "AUTO")))


def retained_dash_style(mobject: Any) -> tuple[list[float], float]:
    """Read an explicit dash style attached to a VMobject.

    ManimCE's DashedVMobject and DashedLine are already expanded into visible
    child subpaths; those children intentionally return the empty native dash
    style so they are not dashed a second time. The attribute aliases below
    preserve dash metadata attached by SVG/plugin/custom VMobject adapters.
    Values use Manim scene units and are projected alongside the path below.
    """

    raw_array: Any = None
    for attribute in (
        "dash_array",
        "stroke_dash_array",
        "stroke_dasharray",
    ):
        if hasattr(mobject, attribute):
            raw_array = getattr(mobject, attribute)
            break
    if raw_array is None:
        return [], 0.0
    if isinstance(raw_array, str):
        raw_values: Any = [
            part for part in re.split(r"[\s,]+", raw_array.strip()) if part
        ]
    elif np.isscalar(raw_array):
        raw_values = [raw_array]
    else:
        raw_values = list(raw_array)
    if len(raw_values) > 64:
        raise ValueError("dash array has more than 64 entries")
    values = [float(value) for value in raw_values]
    if any(
        not math.isfinite(value) or value < 0.00001 or value > 100_000
        for value in values
    ):
        raise ValueError("dash array entries must be finite positive lengths")
    if not values:
        return [], 0.0
    raw_offset: Any = 0.0
    for attribute in (
        "dash_offset",
        "stroke_dash_offset",
        "stroke_dashoffset",
    ):
        if hasattr(mobject, attribute):
            raw_offset = getattr(mobject, attribute)
            break
    offset = float(raw_offset)
    if not math.isfinite(offset):
        raise ValueError("dash offset must be finite")
    return [finite(value) for value in values], finite(offset)


def projected_dash_scale(camera: Any, mobject: Any) -> float:
    """Return the scene-unit to retained-output-unit scale for dash lengths."""

    if isinstance(camera, OpenGLCamera):
        frame_width, _frame_height = camera.get_shape()
        uniforms = getattr(mobject, "uniforms", {})
        if bool(uniforms.get("is_fixed_in_frame", 0.0)) and not bool(
            uniforms.get("is_fixed_orientation", 0.0)
        ):
            frame_width = 8 * 16 / 9
    else:
        frame_width = float(camera.frame_width)
    return OUTPUT_WIDTH / max(float(frame_width), 1e-9)


def shader_field_layout(field_dtype: np.dtype[Any]) -> tuple[str, int] | None:
    base_dtype, shape = (
        field_dtype.subdtype
        if field_dtype.subdtype is not None
        else (field_dtype, ())
    )
    count = int(np.prod(shape)) if shape else 1
    if count not in (1, 2, 3, 4):
        return None
    prefix = {
        np.dtype(np.float32): "float32",
        np.dtype(np.int32): "sint32",
        np.dtype(np.uint32): "uint32",
    }.get(base_dtype)
    return None if prefix is None else (prefix, count)


def shader_field_storage_layout(
    field_dtype: np.dtype[Any],
) -> tuple[str, int] | None:
    base_dtype, shape = (
        field_dtype.subdtype
        if field_dtype.subdtype is not None
        else (field_dtype, ())
    )
    count = int(np.prod(shape)) if shape else 1
    prefix = {
        np.dtype(np.float32): "float32",
        np.dtype(np.int32): "sint32",
        np.dtype(np.uint32): "uint32",
    }.get(base_dtype)
    if prefix is None or count < 1 or count > 16:
        return None
    return prefix, count


def structured_shader_vertex_values(
    vertex_data: np.ndarray[Any, Any],
    attributes: list[str],
) -> list[float] | None:
    if vertex_data.dtype.itemsize % 4:
        return None
    stride = vertex_data.dtype.itemsize // 4
    values = np.zeros((len(vertex_data), stride), dtype=np.float64)
    for name in attributes:
        field = vertex_data.dtype.fields.get(name)
        if field is None:
            return None
        layout = shader_field_storage_layout(field[0])
        if layout is None:
            return None
        _prefix, count = layout
        offset = int(field[1])
        if offset % 4 or offset // 4 + count > stride:
            return None
        values[:, offset // 4 : offset // 4 + count] = np.asarray(
            vertex_data[name]
        ).reshape((len(vertex_data), count))
    return [finite(value) for value in values.reshape(-1)]


def shader_indices_and_primitive(
    wrapper: Any, vertex_count: int
) -> tuple[list[int], str] | None:
    indices_value = wrapper.vert_indices
    indices = (
        []
        if indices_value is None
        else [
            int(index)
            for index in np.asarray(
                indices_value, dtype=np.uint32
            ).reshape(-1)
        ]
    )
    primitive_value = int(wrapper.render_primitive)
    if primitive_value == moderngl.LINE_LOOP:
        source_indices = indices or list(range(vertex_count))
        return source_indices + source_indices[:1], "line-strip"
    if primitive_value == moderngl.TRIANGLE_FAN:
        source_indices = indices or list(range(vertex_count))
        return (
            [
                value
                for corner in range(1, len(source_indices) - 1)
                for value in (
                    source_indices[0],
                    source_indices[corner],
                    source_indices[corner + 1],
                )
            ],
            "triangle-list",
        )
    primitive = {
        moderngl.POINTS: "point-list",
        moderngl.LINES: "line-list",
        moderngl.LINE_STRIP: "line-strip",
        moderngl.TRIANGLES: "triangle-list",
        moderngl.TRIANGLE_STRIP: "triangle-strip",
    }.get(primitive_value)
    return None if primitive is None else (indices, primitive)


def uses_stock_manim_shader(
    wrapper: Any, stock_folders: set[str]
) -> bool:
    folder = Path(str(getattr(wrapper, "shader_folder", ""))).name
    if folder not in stock_folders:
        return False
    return uses_canonical_manim_shader(wrapper)


def uses_canonical_manim_shader(wrapper: Any) -> bool:
    folder = Path(str(getattr(wrapper, "shader_folder", ""))).name
    if not folder:
        return False
    stage_files = {
        "vertex": "vert",
        "geometry": "geom",
        "fragment": "frag",
    }
    for stage, filename in stage_files.items():
        current = wrapper.program_code.get(f"{stage}_shader")
        canonical = get_shader_code_from_file(
            Path(folder) / f"{filename}.glsl"
        )
        if current != canonical:
            return False
    return bool(wrapper.program_code.get("vertex_shader")) and bool(
        wrapper.program_code.get("fragment_shader")
    )


def mobject_uses_stock_shader(
    mobject: OpenGLMobject, stock_folders: set[str]
) -> bool:
    try:
        wrappers = list(mobject.get_shader_wrapper_list())
    except Exception:
        return False
    return bool(wrappers) and all(
        uses_stock_manim_shader(wrapper, stock_folders)
        for wrapper in wrappers
    )


def mobject_uses_canonical_manim_shader(
    mobject: OpenGLMobject,
) -> bool:
    try:
        wrappers = list(mobject.get_shader_wrapper_list())
    except Exception:
        return False
    return bool(wrappers) and all(
        uses_canonical_manim_shader(wrapper) for wrapper in wrappers
    )


POINT_SHADER_VARYING = re.compile(
    r"^\s*((?:(?:flat|smooth|noperspective|centroid|sample|"
    r"invariant)\s+)*)out\s+"
    r"(float|vec2|vec3|vec4|int|ivec2|ivec3|ivec4|"
    r"uint|uvec2|uvec3|uvec4)\s+([A-Za-z_]\w*)\s*;",
    flags=re.MULTILINE,
)


def point_size_geometry_wrapper(wrapper: Any) -> Any | None:
    vertex_source = wrapper.program_code.get("vertex_shader")
    fragment_source = wrapper.program_code.get("fragment_shader")
    if (
        not vertex_source
        or not fragment_source
        or wrapper.program_code.get("geometry_shader")
        or int(wrapper.render_primitive) != moderngl.POINTS
        or (
            "gl_PointSize" not in vertex_source
            and "gl_PointCoord" not in fragment_source
        )
    ):
        return wrapper
    varyings = POINT_SHADER_VARYING.findall(vertex_source)
    renamed_vertex = vertex_source
    forwarded: list[tuple[str, str, str, str]] = []
    for modifiers, field_type, name in varyings:
        renamed = f"{name}_realtime_manim_point_input"
        renamed_vertex = re.sub(
            rf"\b{re.escape(name)}\b", renamed, renamed_vertex
        )
        forwarded.append(
            (modifiers.strip(), field_type, name, renamed)
        )
    point_coord_name = "realtime_manim_point_coord"
    rewritten_fragment = fragment_source
    if "gl_PointCoord" in fragment_source:
        rewritten_fragment = re.sub(
            r"\bgl_PointCoord\b",
            point_coord_name,
            rewritten_fragment,
        )
        rewritten_fragment = re.sub(
            r"(#version\s+\d+\s*)",
            rf"\1\nin vec2 {point_coord_name};\n",
            rewritten_fragment,
            count=1,
        )
    declarations = [
        "#version 330",
        "layout(points) in;",
        "layout(triangle_strip, max_vertices = 4) out;",
        "uniform vec2 realtime_manim_viewport_pixels;",
    ]
    assignments: list[str] = []
    for modifiers, field_type, name, renamed in forwarded:
        prefix = f"{modifiers} " if modifiers else ""
        declarations.append(
            f"{prefix}in {field_type} {renamed}[];"
        )
        declarations.append(f"{prefix}out {field_type} {name};")
        assignments.append(f"        {name} = {renamed}[0];")
    declarations.append(f"out vec2 {point_coord_name};")
    geometry_source = "\n".join(
        [
            *declarations,
            "",
            "void main() {",
            "    vec4 center = gl_in[0].gl_Position;",
            "    float point_size = max(gl_in[0].gl_PointSize, 1.0);",
            "    vec2 clip_radius = point_size * center.w",
            "        / realtime_manim_viewport_pixels;",
            "    vec2 corners[4] = vec2[4](",
            "        vec2(-1.0, -1.0),",
            "        vec2( 1.0, -1.0),",
            "        vec2(-1.0,  1.0),",
            "        vec2( 1.0,  1.0)",
            "    );",
            "    vec2 coords[4] = vec2[4](",
            "        vec2(0.0, 0.0),",
            "        vec2(1.0, 0.0),",
            "        vec2(0.0, 1.0),",
            "        vec2(1.0, 1.0)",
            "    );",
            "    for (int corner = 0; corner < 4; corner++) {",
            *assignments,
            f"        {point_coord_name} = coords[corner];",
            "        gl_Position = center + vec4(",
            "            corners[corner] * clip_radius, 0.0, 0.0",
            "        );",
            "        EmitVertex();",
            "    }",
            "    EndPrimitive();",
            "}",
            "",
        ]
    )
    expanded = wrapper.copy()
    expanded.program_code = dict(wrapper.program_code)
    expanded.program_code["vertex_shader"] = renamed_vertex
    expanded.program_code["geometry_shader"] = geometry_source
    expanded.program_code["fragment_shader"] = rewritten_fragment
    return expanded


def rgba_hex(rgba: Any) -> str:
    channels = np.clip(np.asarray(rgba, dtype=float), 0.0, 1.0)
    if channels.shape[0] == 3:
        channels = np.append(channels, 1.0)
    values = np.rint(channels[:4] * 255).astype(np.uint8)
    return "#" + "".join(f"{int(value):02x}" for value in values)


def projected_commands(camera: Any, vmobject: VMobject) -> list[dict[str, Any]]:
    pixels = camera.points_to_subpixel_coords(vmobject, vmobject.points)
    if len(pixels) == 0:
        return []
    projected = projected_pixels(camera, pixels)
    commands: list[dict[str, Any]] = []
    for subpath in vmobject.gen_subpaths_from_points_2d(projected):
        if len(subpath) < 4:
            continue
        commands.append(
            {"op": "moveTo", "x": finite(subpath[0][0]), "y": finite(subpath[0][1])}
        )
        for _start, control_1, control_2, end in vmobject.gen_cubic_bezier_tuples_from_points(
            subpath
        ):
            commands.append(
                {
                    "op": "cubicTo",
                    "c1x": finite(control_1[0]),
                    "c1y": finite(control_1[1]),
                    "c2x": finite(control_2[0]),
                    "c2y": finite(control_2[1]),
                    "x": finite(end[0]),
                    "y": finite(end[1]),
                }
            )
        if vmobject.consider_points_equals_2d(subpath[0], subpath[-1]):
            commands.append({"op": "close"})
    return commands


def world_commands_2d(vmobject: VMobject) -> list[dict[str, Any]]:
    commands: list[dict[str, Any]] = []
    points = np.asarray(vmobject.points, dtype=float)
    for subpath in vmobject.gen_subpaths_from_points_2d(points):
        if len(subpath) < 4:
            continue
        commands.append(
            {
                "op": "moveTo",
                "x": finite(subpath[0][0]),
                "y": finite(subpath[0][1]),
            }
        )
        for _start, control_1, control_2, end in vmobject.gen_cubic_bezier_tuples_from_points(
            subpath
        ):
            commands.append(
                {
                    "op": "cubicTo",
                    "c1x": finite(control_1[0]),
                    "c1y": finite(control_1[1]),
                    "c2x": finite(control_2[0]),
                    "c2y": finite(control_2[1]),
                    "x": finite(end[0]),
                    "y": finite(end[1]),
                }
            )
        if vmobject.consider_points_equals_2d(subpath[0], subpath[-1]):
            commands.append({"op": "close"})
    return commands


def world_commands_3d(vmobject: VMobject) -> list[dict[str, Any]]:
    commands: list[dict[str, Any]] = []
    points = np.asarray(vmobject.points, dtype=float)
    for subpath in vmobject.gen_subpaths_from_points_2d(points):
        if len(subpath) < 4:
            continue
        commands.append(
            {
                "op": "moveTo",
                "x": finite(subpath[0][0]),
                "y": finite(subpath[0][1]),
                "z": finite(subpath[0][2]),
            }
        )
        for _start, control_1, control_2, end in vmobject.gen_cubic_bezier_tuples_from_points(
            subpath
        ):
            commands.append(
                {
                    "op": "cubicTo",
                    "c1x": finite(control_1[0]),
                    "c1y": finite(control_1[1]),
                    "c1z": finite(control_1[2]),
                    "c2x": finite(control_2[0]),
                    "c2y": finite(control_2[1]),
                    "c2z": finite(control_2[2]),
                    "x": finite(end[0]),
                    "y": finite(end[1]),
                    "z": finite(end[2]),
                }
            )
        if vmobject.consider_points_equals(subpath[0], subpath[-1]):
            commands.append({"op": "close"})
    return commands


def projected_opengl_commands(
    camera: Any, vmobject: OpenGLVMobject
) -> list[dict[str, Any]]:
    if len(vmobject.points) == 0:
        return []
    projected = project_mobject_points(camera, vmobject, vmobject.points)
    commands: list[dict[str, Any]] = []
    for subpath in vmobject.get_subpaths_from_points(projected):
        if len(subpath) < 4:
            continue
        commands.append(
            {
                "op": "moveTo",
                "x": finite(subpath[0][0]),
                "y": finite(subpath[0][1]),
            }
        )
        for curve in vmobject.get_bezier_tuples_from_points(subpath):
            if len(curve) == 3:
                _start, control, end = curve
                commands.append(
                    {
                        "op": "quadTo",
                        "cx": finite(control[0]),
                        "cy": finite(control[1]),
                        "x": finite(end[0]),
                        "y": finite(end[1]),
                    }
                )
            elif len(curve) == 4:
                _start, control_1, control_2, end = curve
                commands.append(
                    {
                        "op": "cubicTo",
                        "c1x": finite(control_1[0]),
                        "c1y": finite(control_1[1]),
                        "c2x": finite(control_2[0]),
                        "c2y": finite(control_2[1]),
                        "x": finite(end[0]),
                        "y": finite(end[1]),
                    }
                )
        if vmobject.consider_points_equals(subpath[0], subpath[-1]):
            commands.append({"op": "close"})
    return commands


def projected_pixels(camera: Any, pixels: np.ndarray) -> np.ndarray:
    projected = np.zeros((len(pixels), 3), dtype=float)
    projected[:, 0] = (pixels[:, 0] / camera.pixel_width - 0.5) * OUTPUT_WIDTH
    projected[:, 1] = (0.5 - pixels[:, 1] / camera.pixel_height) * OUTPUT_HEIGHT
    return projected


def project_mobject_points(
    camera: Any, mobject: Any, points: np.ndarray
) -> np.ndarray:
    if isinstance(camera, OpenGLCamera):
        values = np.asarray(points, dtype=float)
        homogeneous = np.concatenate(
            [values[:, :3], np.ones((len(values), 1), dtype=float)], axis=1
        )
        model = np.asarray(
            getattr(mobject, "model_matrix", np.eye(4)), dtype=float
        )
        positioned = (model @ homogeneous.T).T[:, :3]
        uniforms = getattr(mobject, "uniforms", {})
        fixed_in_frame = bool(uniforms.get("is_fixed_in_frame", 0.0))
        fixed_orientation = bool(
            uniforms.get("is_fixed_orientation", 0.0)
        )
        if not fixed_in_frame:
            if fixed_orientation:
                fixed_center = np.asarray(
                    uniforms.get("fixed_orientation_center", (0, 0, 0)),
                    dtype=float,
                )
                new_center = camera.inverse_rotation_matrix @ fixed_center
                positioned = positioned + (new_center - fixed_center)
            else:
                positioned = (
                    camera.inverse_rotation_matrix
                    @ (positioned - camera.get_center()).T
                ).T
        frame_width, frame_height = camera.get_shape()
        if fixed_in_frame and not fixed_orientation:
            frame_width, frame_height = (8 * 16 / 9, 8)
        scale = np.ones(len(positioned), dtype=float)
        if not fixed_in_frame:
            focal_distance = float(camera.get_focal_distance())
            denominator = focal_distance - positioned[:, 2]
            valid = np.abs(denominator) > 1e-9
            scale[valid] = np.maximum(
                0.0, focal_distance / denominator[valid]
            )
            scale[~valid] = 0.0
        projected = np.zeros((len(values), 3), dtype=float)
        projected[:, 0] = (
            positioned[:, 0] * scale * OUTPUT_WIDTH / frame_width
        )
        projected[:, 1] = (
            positioned[:, 1] * scale * OUTPUT_HEIGHT / frame_height
        )
        projected[:, 2] = positioned[:, 2]
        return projected
    return projected_pixels(
        camera, camera.points_to_subpixel_coords(mobject, points)
    )


def opengl_camera_space_points(
    camera: OpenGLCamera, mobject: Any, points: np.ndarray
) -> np.ndarray:
    values = np.asarray(points, dtype=float)
    homogeneous = np.concatenate(
        [values[:, :3], np.ones((len(values), 1), dtype=float)], axis=1
    )
    model = np.asarray(
        getattr(mobject, "model_matrix", np.eye(4)), dtype=float
    )
    positioned = (model @ homogeneous.T).T[:, :3]
    return (
        camera.inverse_rotation_matrix
        @ (positioned - camera.get_center()).T
    ).T


def paint(
    camera: Any, vmobject: VMobject, kind: str
) -> tuple[str, dict[str, Any] | None]:
    values = (
        camera.get_fill_rgbas(vmobject)
        if kind == "fill"
        else camera.get_stroke_rgbas(vmobject)
    )
    if len(values) == 0:
        return "#00000000", None
    solid = rgba_hex(values[0])
    if len(values) == 1 or np.allclose(values, values[0]):
        return solid, None
    endpoints = vmobject.get_gradient_start_and_end_points()
    pixels = camera.points_to_subpixel_coords(vmobject, endpoints)
    projected = projected_pixels(camera, pixels)
    return solid, {
        "from": [finite(projected[0][0]), finite(projected[0][1])],
        "to": [finite(projected[1][0]), finite(projected[1][1])],
        "stops": [
            {
                "offset": finite(index / (len(values) - 1)),
                "color": rgba_hex(rgba),
            }
            for index, rgba in enumerate(values)
        ],
    }


def world_gradient(
    camera: Any, vmobject: VMobject, kind: str
) -> dict[str, Any] | None:
    values = (
        camera.get_fill_rgbas(vmobject)
        if kind == "fill"
        else camera.get_stroke_rgbas(vmobject)
    )
    if len(values) <= 1 or np.allclose(values, values[0]):
        return None
    endpoints = np.asarray(
        vmobject.get_gradient_start_and_end_points(), dtype=float
    )
    if endpoints.shape != (2, 3):
        return None
    return {
        "from": [finite(endpoints[0][0]), finite(endpoints[0][1])],
        "to": [finite(endpoints[1][0]), finite(endpoints[1][1])],
        "stops": [
            {
                "offset": finite(index / (len(values) - 1)),
                "color": rgba_hex(rgba),
            }
            for index, rgba in enumerate(values)
        ],
    }


def opengl_paint(
    camera: Any, vmobject: OpenGLVMobject, kind: str
) -> tuple[str, dict[str, Any] | None]:
    values = np.asarray(vmobject.data[f"{kind}_rgba"], dtype=float)
    if len(values) == 0:
        return "#00000000", None
    solid = rgba_hex(values[0])
    if len(values) == 1 or np.allclose(values, values[0]):
        return solid, None
    points = project_mobject_points(camera, vmobject, vmobject.points)
    minimum = np.min(points[:, :2], axis=0)
    maximum = np.max(points[:, :2], axis=0)
    if np.allclose(minimum, maximum):
        return solid, None
    return solid, {
        "from": [finite(minimum[0]), finite(minimum[1])],
        "to": [finite(maximum[0]), finite(minimum[1])],
        "stops": [
            {
                "offset": finite(index / (len(values) - 1)),
                "color": rgba_hex(rgba),
            }
            for index, rgba in enumerate(values)
        ],
    }


@dataclass
class Snapshot:
    at: float
    commands: list[dict[str, Any]]
    fill: str
    fill_gradient: dict[str, Any] | None
    stroke: str
    stroke_gradient: dict[str, Any] | None
    stroke_width: float
    z_index: int
    stroke_cap: str = "butt"
    stroke_join: str = "miter"
    dash_array: list[float] = field(default_factory=list)
    dash_offset: float = 0.0
    commands_3d: list[dict[str, Any]] = field(default_factory=list)
    semantic_base_commands: list[dict[str, Any]] = field(
        default_factory=list
    )
    draw_start: float = 0.0
    draw_end: float = 1.0
    world_commands_2d: list[dict[str, Any]] = field(default_factory=list)
    world_stroke_width: float = 0.0
    world_dash_array: list[float] = field(default_factory=list)
    world_dash_offset: float = 0.0
    world_fill_gradient: dict[str, Any] | None = None
    world_stroke_gradient: dict[str, Any] | None = None
    fixed_orientation_center: list[float] = field(default_factory=list)
    fixed_orientation_base: list[float] = field(default_factory=list)


@dataclass
class ObjectTrack:
    node_id: str
    first_seen: float
    last_seen: float
    is_traced_path: bool = False
    disappear_at: float | None = None
    snapshots: list[Snapshot] = field(default_factory=list)


@dataclass
class ImageSnapshot:
    at: float
    corners: list[list[float]]
    opacity: float
    z_index: int
    world_corners: list[list[float]] = field(default_factory=list)


@dataclass
class ImageTrack:
    node_id: str
    first_seen: float
    last_seen: float
    pixels: str
    pixel_width: int
    pixel_height: int
    base_pixel_array: np.ndarray
    original_alpha: np.ndarray
    resampling: str
    disappear_at: float | None = None
    snapshots: list[ImageSnapshot] = field(default_factory=list)


@dataclass
class PointCloudSnapshot:
    at: float
    points: list[list[float]]
    colors: list[str]
    rgbas: np.ndarray
    radius: float
    z_index: int
    world_points: list[list[float]] = field(default_factory=list)


@dataclass
class PointCloudTrack:
    node_id: str
    first_seen: float
    last_seen: float
    disappear_at: float | None = None
    snapshots: list[PointCloudSnapshot] = field(default_factory=list)


@dataclass
class MeshSnapshot:
    at: float
    vertices: list[list[float]]
    normals: list[list[float]]
    light_position: list[list[float]]
    uvs: list[list[float]]
    colors: list[str]
    rgbas: np.ndarray
    opacity: float
    z_index: int
    surface_vertices: list[list[float]] = field(default_factory=list)
    surface_patches: list[list[int]] = field(default_factory=list)
    surface_rgbas: np.ndarray | None = None
    surface_stroke_radii: list[float] = field(default_factory=list)


@dataclass
class MeshTrack:
    node_id: str
    first_seen: float
    last_seen: float
    triangles: list[list[int]]
    double_sided: bool
    unlit: bool
    gloss: float
    shadow: float
    uvs: list[list[float]]
    texture_pixels: str
    texture_width: int
    texture_height: int
    dark_texture_pixels: str
    dark_texture_width: int
    dark_texture_height: int
    texture_resampling: str
    disappear_at: float | None = None
    snapshots: list[MeshSnapshot] = field(default_factory=list)


@dataclass
class CustomShaderSnapshot:
    at: float
    vertex_data: list[float]
    uniform_values: list[float]
    z_index: int


@dataclass
class CustomShaderTrack:
    node_id: str
    first_seen: float
    last_seen: float
    vertex_wgsl: str
    fragment_wgsl: str
    attributes: list[dict[str, Any]]
    vertex_stride: int
    indices: list[int]
    primitive: str
    uniforms: list[dict[str, Any]]
    depth_test: bool
    program_signature: tuple[str, ...]
    disappear_at: float | None = None
    snapshots: list[CustomShaderSnapshot] = field(default_factory=list)


@dataclass
class Camera3dSnapshot:
    at: float
    position: list[float]
    target: list[float]
    up: list[float]
    fov_y: float


@dataclass
class Camera2dSnapshot:
    at: float
    x: float
    y: float
    zoom: float
    aspect_matches: bool


def project_camera_3d_point(
    snapshot: Camera3dSnapshot,
    point: np.ndarray,
) -> list[float] | None:
    position = np.asarray(snapshot.position, dtype=float)
    target = np.asarray(snapshot.target, dtype=float)
    up = np.asarray(snapshot.up, dtype=float)
    forward = target - position
    forward_length = float(np.linalg.norm(forward))
    if forward_length <= 0.000001:
        return None
    forward /= forward_length
    right = np.cross(forward, up)
    right_length = float(np.linalg.norm(right))
    if right_length <= 0.000001:
        return None
    right /= right_length
    camera_up = np.cross(right, forward)
    camera_up /= max(float(np.linalg.norm(camera_up)), 0.000001)
    relative = np.asarray(point, dtype=float) - position
    view_z = float(np.dot(relative, forward))
    if view_z <= 0.001 or view_z > 1_000:
        return None
    tan_half_fov = max(math.tan(snapshot.fov_y * 0.5), 0.0001)
    aspect = OUTPUT_WIDTH / OUTPUT_HEIGHT
    return [
        finite(float(np.dot(relative, right)) / (view_z * tan_half_fov * aspect) * OUTPUT_WIDTH * 0.5),
        finite(float(np.dot(relative, camera_up)) / (view_z * tan_half_fov) * OUTPUT_HEIGHT * 0.5),
    ]


class CompatibilityFileWriter:
    def __init__(self, renderer: "CompatibilityRenderer") -> None:
        self.renderer = renderer
        self.subcaptions: list[Any] = []

    def add_sound(
        self,
        sound_file: str,
        time: float,
        gain: float | None = None,
        **_kwargs: Any,
    ) -> None:
        path = Path(sound_file).expanduser().resolve()
        try:
            payload = path.read_bytes()
        except OSError:
            self.renderer.diagnostics.add("unreadable-audio")
            return
        if not payload or len(payload) > 64 * 1024 * 1024:
            self.renderer.diagnostics.add("invalid-audio-size")
            return
        mime_type = {
            ".wav": "audio/wav",
            ".raw": "audio/wav",
            ".mp3": "audio/mpeg",
            ".m4a": "audio/mp4",
            ".aac": "audio/aac",
            ".ogg": "audio/ogg",
            ".opus": "audio/ogg",
            ".webm": "audio/webm",
            ".flac": "audio/flac",
        }.get(path.suffix.lower(), mimetypes.guess_type(path.name)[0])
        if not mime_type or not mime_type.startswith("audio/"):
            self.renderer.diagnostics.add("unsupported-audio-format")
            return
        self.renderer.audio.append(
            {
                "id": f"manim-audio-{len(self.renderer.audio):03d}",
                "data": base64.b64encode(payload).decode("ascii"),
                "mimeType": mime_type,
                "startTime": finite(time),
                "gainDb": 0.0 if gain is None else finite(gain),
            }
        )


class CompatibilityRenderer:
    """Minimal renderer contract that executes regular Manim animation semantics."""

    def __init__(
        self,
        fps: int,
        renderer_name: str = "cairo",
        *,
        semantic_billboards: bool = True,
        compact_surface_lifetimes: bool = True,
        full_affine_tracks: bool = True,
    ) -> None:
        self.fps = fps
        self.renderer_name = renderer_name
        self.enable_semantic_billboards = semantic_billboards
        self.enable_compact_surface_lifetimes = compact_surface_lifetimes
        self.enable_full_affine_tracks = full_affine_tracks
        self.time = 0.0
        self.num_plays = 0
        self.skip_animations = False
        self.static_image = None
        self.file_writer = CompatibilityFileWriter(self)
        # Retain object references as identity keys. Integer `id()` values can
        # be reused after Manim's always_redraw objects are collected.
        self.object_tracks: dict[tuple[Mobject, int], ObjectTrack] = {}
        self.image_tracks: dict[
            tuple[AbstractImageMobject, int], ImageTrack
        ] = {}
        self.point_tracks: dict[tuple[Any, int], PointCloudTrack] = {}
        self.mesh_tracks: dict[tuple[Any, int], MeshTrack] = {}
        self.object_active: dict[Mobject, tuple[Mobject, int]] = {}
        self.image_active: dict[
            AbstractImageMobject, tuple[AbstractImageMobject, int]
        ] = {}
        self.point_active: dict[Any, tuple[Any, int]] = {}
        self.mesh_active: dict[Any, tuple[Any, int]] = {}
        self.object_generations: dict[Mobject, int] = {}
        self.image_generations: dict[AbstractImageMobject, int] = {}
        self.point_generations: dict[Any, int] = {}
        self.mesh_generations: dict[Any, int] = {}
        self.custom_shader_tracks: dict[
            tuple[OpenGLMobject, int, int], CustomShaderTrack
        ] = {}
        self.custom_shader_active: dict[
            tuple[OpenGLMobject, int], tuple[OpenGLMobject, int, int]
        ] = {}
        self.custom_shader_generations: dict[
            tuple[OpenGLMobject, int], int
        ] = {}
        self._gl_transform_context: Any | None = None
        self.camera_2d_snapshots: list[Camera2dSnapshot] = []
        self.camera_3d_snapshots: list[Camera3dSnapshot] = []
        self.audio: list[dict[str, Any]] = []
        self.diagnostics: set[str] = set()
        self.frames = 0
        self.semantic_translation_tracks = 0
        self.semantic_scale_translation_tracks = 0
        self.semantic_rotation_translation_tracks = 0
        self.semantic_similarity_tracks = 0
        self.semantic_affine_transform_tracks = 0
        self.semantic_timeline_controls = 0
        self.semantic_draw_progress_tracks = 0
        self.semantic_draw_range_tracks = 0
        self.semantic_surface_nodes = 0
        self.semantic_dynamic_surface_nodes = 0
        self.semantic_path_references = 0
        self.semantic_translated_path_references = 0
        self.semantic_affine_groups = 0
        self.semantic_affine_group_members = 0
        self.semantic_billboard_groups = 0
        self.semantic_billboard_members = 0
        self.semantic_track_references = 0
        self.semantic_path_3d_nodes = 0
        self.semantic_trace_path_nodes = 0
        self.semantic_path_data_tracks = 0
        self.semantic_camera_2d_tracks = 0
        self.semantic_camera_3d_tracks = 0
        self.random_seed = 0
        self.active_animations: list[Any] = []
        self.active_local_time = 0.0
        self.scene_name = "Manim scene"
        self.value_tracker_snapshots: dict[
            ValueTracker, list[tuple[float, float]]
        ] = {}

    def init_scene(self, scene: Scene) -> None:
        self.scene_name = scene.__class__.__name__
        self.random_seed = int(scene.random_seed or 0)
        if self.renderer_name == "opengl":
            self.camera = OpenGLCamera()
            self.camera.use_z_index = False
        else:
            self.camera = scene.camera_class()

    def play(self, scene: Scene, *animations: Any, **kwargs: Any) -> None:
        scene.compile_animation_data(*animations, **kwargs)
        scene.begin_animations()
        assert scene.animations is not None
        self.active_animations = list(scene.animations)
        scene.duration = scene.get_run_time(scene.animations)
        scene.time_progression = scene._get_animation_time_progression(
            scene.animations, scene.duration
        )
        start_time = self.time
        for local_time in scene.time_progression:
            self.active_local_time = float(local_time)
            self.time = start_time + float(local_time)
            scene.update_to_time(float(local_time))
            self.capture(scene)
            if scene.stop_condition is not None and scene.stop_condition():
                scene.time_progression.close()
                break
        for animation in scene.animations:
            animation.finish()
            animation.clean_up_from_scene(scene)
        scene.update_mobjects(0)
        self.static_image = None
        self.time = start_time + scene.duration
        self.active_local_time = scene.duration
        self.capture(scene)
        self.active_animations = []
        scene.time_progression.close()
        self.num_plays += 1

    def capture(self, scene: Scene) -> None:
        if hasattr(self.camera, "reset_rotation_matrix"):
            self.camera.reset_rotation_matrix()
        roots = list(scene.mobjects)
        roots.extend(
            mobject for mobject in scene.foreground_mobjects if mobject not in roots
        )
        if self.renderer_name == "opengl":
            displayed = []
            seen_displayed: set[Any] = set()
            for root in roots:
                for mobject in root.get_family():
                    if mobject not in seen_displayed:
                        displayed.append(mobject)
                        seen_displayed.add(mobject)
        else:
            displayed = self.camera.get_mobjects_to_display(
                roots, include_submobjects=True
            )
        if isinstance(self.camera, ManimMovingCamera):
            displayed = [
                mobject
                for mobject in displayed
                if mobject is not self.camera.frame
            ]
        for root in roots:
            self._diagnose_non_vector(root)
        at = finite(self.time)
        self._capture_value_trackers(at)
        self._capture_camera_2d(at)
        self._capture_camera_3d(at)
        visible_vectors: set[tuple[Mobject, int]] = set()
        visible_images: set[tuple[AbstractImageMobject, int]] = set()
        visible_points: set[tuple[Any, int]] = set()
        visible_meshes: set[tuple[Any, int]] = set()
        visible_custom_shaders: set[tuple[OpenGLMobject, int, int]] = set()
        if self.renderer_name == "cairo" and not bool(
            getattr(self.camera, "exponential_projection", False)
        ):
            cairo_surfaces: list[Surface] = []
            seen_surfaces: set[Surface] = set()
            for root in roots:
                for member in root.get_family():
                    if isinstance(member, Surface) and member not in seen_surfaces:
                        cairo_surfaces.append(member)
                        seen_surfaces.add(member)
            consumed_surface_members: set[Any] = set()
            for surface in cairo_surfaces:
                family = set(surface.get_family())
                pieces = [
                    member
                    for member in surface.get_family()
                    if member in family
                    and member is not surface
                    and member in displayed
                    and isinstance(member, VMobject)
                    and len(member.points) > 0
                ]
                if pieces:
                    first_order = min(
                        index
                        for index, member in enumerate(displayed)
                        if member in family
                    )
                    mesh_key = self._capture_cairo_surface(
                        surface, pieces, at, first_order
                    )
                    if mesh_key is not None:
                        visible_meshes.add(mesh_key)
                    consumed_surface_members.update(family)
            if consumed_surface_members:
                displayed = [
                    member
                    for member in displayed
                    if member not in consumed_surface_members
                ]
        for order, mobject in enumerate(displayed):
            if isinstance(mobject, OpenGLMobject) and not bool(
                getattr(mobject, "should_render", True)
            ):
                continue
            if (
                isinstance(mobject, OpenGLImageMobject)
                and not mobject_uses_stock_shader(
                    mobject, {"textured_surface"}
                )
            ):
                custom_shader_keys = self._capture_custom_opengl_shaders(
                    mobject, at, order
                )
                if custom_shader_keys is not None:
                    visible_custom_shaders.update(custom_shader_keys)
                else:
                    self.diagnostics.add(
                        "unsupported-opengl-custom-shader:"
                        f"{type(mobject).__name__}"
                    )
                continue
            if isinstance(mobject, AbstractImageMobject):
                image_key = self._capture_image(mobject, at, order)
                if image_key is not None:
                    visible_images.add(image_key)
                continue
            if isinstance(mobject, (PMobject, OpenGLPMobject)):
                if len(getattr(mobject, "points", ())) == 0:
                    # PGroup/OpenGLPGroup are family containers; their visual
                    # children are captured independently.
                    continue
                if (
                    isinstance(mobject, OpenGLPMobject)
                    and not mobject_uses_stock_shader(
                        mobject, {"true_dot"}
                    )
                ):
                    custom_shader_keys = (
                        self._capture_custom_opengl_shaders(
                            mobject, at, order
                        )
                    )
                    if custom_shader_keys is not None:
                        visible_custom_shaders.update(
                            custom_shader_keys
                        )
                    else:
                        self.diagnostics.add(
                            "unsupported-opengl-custom-shader:"
                            f"{type(mobject).__name__}"
                        )
                    continue
                point_key = self._capture_point_cloud(mobject, at, order)
                if point_key is not None:
                    visible_points.add(point_key)
                continue
            if (
                isinstance(mobject, OpenGLSurface)
                and len(getattr(mobject, "points", ())) == 0
                and len(getattr(mobject, "submobjects", ())) > 0
            ):
                # OpenGLSurfaceGroup carries no geometry of its own. Its
                # wrapper can expose a child index buffer without a matching
                # root vertex buffer, so capture only the displayed children.
                continue
            if (
                isinstance(mobject, OpenGLMobject)
                and len(getattr(mobject, "points", ())) == 0
                and (
                    len(getattr(mobject, "submobjects", ())) > 0
                    or type(mobject).__name__ == "OpenGLPoint"
                )
            ):
                # Family containers and OpenGLPoint's invisible positioning
                # helper do not own a visual shader payload.
                continue
            if (
                isinstance(mobject, OpenGLMobject)
                and len(getattr(mobject, "points", ())) == 0
            ):
                try:
                    empty_wrappers = list(
                        mobject.get_shader_wrapper_list()
                    )
                except Exception:
                    empty_wrappers = []
                if not empty_wrappers:
                    # Empty cleanup sentinels and base-class placeholders are
                    # non-visual. A plugin with wrapper-owned geometry still
                    # proceeds through custom-shader capture.
                    continue
            if (
                isinstance(mobject, OpenGLSurface)
                and mobject_uses_stock_shader(
                    mobject, {"surface", "textured_surface"}
                )
            ):
                mesh_key = self._capture_opengl_surface(mobject, at, order)
                if mesh_key is not None:
                    visible_meshes.add(mesh_key)
                continue
            if (
                isinstance(mobject, OpenGLVMobject)
                and not mobject_uses_canonical_manim_shader(mobject)
            ):
                custom_shader_keys = self._capture_custom_opengl_shaders(
                    mobject, at, order
                )
                if custom_shader_keys is not None:
                    visible_custom_shaders.update(custom_shader_keys)
                else:
                    self.diagnostics.add(
                        "unsupported-opengl-custom-shader:"
                        f"{type(mobject).__name__}"
                    )
                continue
            if (
                isinstance(mobject, OpenGLMobject)
                and not mobject_uses_canonical_manim_shader(mobject)
            ):
                custom_shader_keys = self._capture_custom_opengl_shaders(
                    mobject, at, order
                )
                if custom_shader_keys is not None:
                    visible_custom_shaders.update(custom_shader_keys)
                else:
                    self.diagnostics.add(
                        "unsupported-opengl-custom-shader:"
                        f"{type(mobject).__name__}"
                    )
                continue
            if isinstance(mobject, OpenGLMobject):
                custom_shader_keys = self._capture_custom_opengl_shaders(
                    mobject, at, order
                )
                if custom_shader_keys is not None:
                    visible_custom_shaders.update(custom_shader_keys)
                    continue
            if isinstance(mobject, OpenGLSurface):
                mesh_key = self._capture_opengl_surface(mobject, at, order)
                if mesh_key is not None:
                    visible_meshes.add(mesh_key)
                continue
            if isinstance(mobject, OpenGLMobject) and self._capture_generic_opengl_mesh(
                mobject, at, order
            ):
                mesh_key = self.mesh_active.get(mobject)
                if mesh_key is not None:
                    visible_meshes.add(mesh_key)
                continue
            if (
                isinstance(mobject, OpenGLMobject)
                and type(mobject).__name__ == "OpenGLPoint"
                and not getattr(mobject, "shader_folder", "")
            ):
                # OpenGLPoint is Manim's intentionally invisible one-point
                # positioning helper, not a rendered point-cloud primitive.
                continue
            if not isinstance(mobject, (VMobject, OpenGLVMobject)) or len(mobject.points) == 0:
                if (
                    not isinstance(mobject, (VMobject, OpenGLVMobject))
                    and not isinstance(mobject, AbstractImageMobject)
                    and not isinstance(mobject, (PMobject, OpenGLPMobject))
                    and not (
                        isinstance(mobject, OpenGLMobject)
                        and len(getattr(mobject, "points", ())) == 0
                    )
                    and not type(mobject).__module__.startswith(
                        "manim.mobject.value_tracker"
                    )
                ):
                    self.diagnostics.add(
                        f"unsupported-mobject:{type(mobject).__name__}"
                    )
                continue
            is_opengl = isinstance(mobject, OpenGLVMobject)
            commands = (
                projected_opengl_commands(self.camera, mobject)
                if is_opengl
                else projected_commands(self.camera, mobject)
            )
            if not commands:
                continue
            commands_3d: list[dict[str, Any]] = []
            if (
                isinstance(self.camera, ThreeDCamera)
                and not is_opengl
                and mobject not in self.camera.fixed_in_frame_mobjects
                and mobject not in self.camera.fixed_orientation_mobjects
            ):
                commands_3d = world_commands_3d(mobject)
            (
                semantic_base_commands,
                draw_start,
                draw_end,
            ) = self._semantic_draw_range(mobject, commands)
            key = self.object_active.get(mobject)
            track = None if key is None else self.object_tracks.get(key)
            if track is None or track.disappear_at is not None:
                if track is not None and track.disappear_at is None:
                    track.disappear_at = at
                key = self._new_lifetime_key(
                    mobject,
                    self.object_active,
                    self.object_generations,
                )
                track = ObjectTrack(
                    node_id=f"manim-{len(self.object_tracks):05d}",
                    first_seen=at,
                    last_seen=at,
                    is_traced_path=isinstance(mobject, TracedPath),
                )
                self.object_tracks[key] = track
            fill, fill_gradient = (
                opengl_paint(self.camera, mobject, "fill")
                if is_opengl
                else paint(self.camera, mobject, "fill")
            )
            stroke, stroke_gradient = (
                opengl_paint(self.camera, mobject, "stroke")
                if is_opengl
                else paint(self.camera, mobject, "stroke")
            )
            stroke_width = float(np.max(mobject.get_stroke_width()))
            stroke_cap = retained_stroke_cap(mobject)
            if stroke_cap is None:
                self.diagnostics.add("unsupported-stroke-cap")
                stroke_cap = "butt"
            stroke_join = retained_stroke_join(mobject)
            if stroke_join is None:
                self.diagnostics.add("unsupported-stroke-join")
                stroke_join = "miter"
            try:
                world_dash_array, world_dash_offset = retained_dash_style(
                    mobject
                )
                dash_scale = projected_dash_scale(self.camera, mobject)
                dash_array = [
                    finite(value * dash_scale)
                    for value in world_dash_array
                ]
                dash_offset = finite(world_dash_offset * dash_scale)
                if any(
                    value < 0.00001 or value > 100_000
                    for value in dash_array
                ):
                    raise ValueError("projected dash length is out of range")
            except (TypeError, ValueError, OverflowError):
                self.diagnostics.add("unsupported-stroke-dash-style")
                dash_array = []
                dash_offset = 0.0
                world_dash_array = []
                world_dash_offset = 0.0
            fixed_orientation_center: list[float] = []
            fixed_orientation_base: list[float] = []
            if (
                isinstance(self.camera, ThreeDCamera)
                and not is_opengl
                and mobject in self.camera.fixed_orientation_mobjects
                and self.camera_3d_snapshots
            ):
                center = np.asarray(
                    self.camera.fixed_orientation_mobjects[mobject](),
                    dtype=float,
                )
                base = project_camera_3d_point(
                    self.camera_3d_snapshots[-1], center
                )
                if base is not None:
                    fixed_orientation_center = [
                        finite(channel) for channel in center
                    ]
                    fixed_orientation_base = base
            snapshot = Snapshot(
                at=at,
                commands=commands,
                fill=fill,
                fill_gradient=fill_gradient,
                stroke=stroke,
                stroke_gradient=stroke_gradient,
                stroke_width=finite(
                    stroke_width
                    * (
                        OUTPUT_WIDTH / float(config.pixel_width)
                        if is_opengl
                        else float(self.camera.cairo_line_width_multiple)
                        * OUTPUT_WIDTH
                        / float(self.camera.frame_width)
                    )
                ),
                z_index=int(getattr(mobject, "z_index", 0)) * 10_000 + order,
                stroke_cap=stroke_cap,
                stroke_join=stroke_join,
                dash_array=dash_array,
                dash_offset=dash_offset,
                commands_3d=commands_3d,
                semantic_base_commands=semantic_base_commands,
                draw_start=draw_start,
                draw_end=draw_end,
                world_commands_2d=(
                    world_commands_2d(mobject)
                    if isinstance(self.camera, ManimMovingCamera)
                    and not is_opengl
                    else []
                ),
                world_stroke_width=finite(
                    stroke_width
                    * float(self.camera.cairo_line_width_multiple)
                    if isinstance(self.camera, ManimMovingCamera)
                    and not is_opengl
                    else 0.0
                ),
                world_dash_array=world_dash_array,
                world_dash_offset=world_dash_offset,
                world_fill_gradient=(
                    world_gradient(self.camera, mobject, "fill")
                    if isinstance(self.camera, ManimMovingCamera)
                    and not is_opengl
                    else None
                ),
                world_stroke_gradient=(
                    world_gradient(self.camera, mobject, "stroke")
                    if isinstance(self.camera, ManimMovingCamera)
                    and not is_opengl
                    else None
                ),
                fixed_orientation_center=fixed_orientation_center,
                fixed_orientation_base=fixed_orientation_base,
            )
            previous_snapshot = (
                track.snapshots[-1] if track.snapshots else None
            )
            native_dash_style_changed = previous_snapshot is not None and (
                previous_snapshot.world_dash_array
                != snapshot.world_dash_array
                or previous_snapshot.world_dash_offset
                != snapshot.world_dash_offset
            )
            projected_dash_style_changed = previous_snapshot is not None and (
                previous_snapshot.dash_array != snapshot.dash_array
                or previous_snapshot.dash_offset != snapshot.dash_offset
            )
            if previous_snapshot is not None and (
                previous_snapshot.stroke_cap != snapshot.stroke_cap
                or previous_snapshot.stroke_join != snapshot.stroke_join
                or (
                    native_dash_style_changed
                    if isinstance(self.camera, ManimMovingCamera)
                    and not is_opengl
                    else projected_dash_style_changed
                )
            ):
                # These are static retained-style fields rather than track
                # properties. Preserve updater-driven changes by starting a
                # new lifetime at the exact sampled transition instead of
                # silently freezing the first value.
                track.disappear_at = at
                key = self._new_lifetime_key(
                    mobject,
                    self.object_active,
                    self.object_generations,
                )
                track = ObjectTrack(
                    node_id=f"manim-{len(self.object_tracks):05d}",
                    first_seen=at,
                    last_seen=at,
                    is_traced_path=isinstance(mobject, TracedPath),
                )
                self.object_tracks[key] = track
            semantic_base = next(
                (
                    prior.semantic_base_commands
                    for prior in track.snapshots
                    if prior.semantic_base_commands
                ),
                [],
            )
            if (
                semantic_base
                and not snapshot.semantic_base_commands
                and snapshot.commands != semantic_base
                and track.snapshots
                and track.snapshots[-1].commands == semantic_base
            ):
                previous = replace(
                    track.snapshots[-1],
                    semantic_base_commands=[],
                    draw_start=0.0,
                    draw_end=1.0,
                )
                track.last_seen = previous.at
                track.disappear_at = previous.at
                key = self._new_lifetime_key(
                    mobject,
                    self.object_active,
                    self.object_generations,
                )
                track = ObjectTrack(
                    node_id=f"manim-{len(self.object_tracks):05d}",
                    first_seen=previous.at,
                    last_seen=at,
                    is_traced_path=track.is_traced_path,
                    snapshots=[previous],
                )
                self.object_tracks[key] = track
            visible_vectors.add(key)
            track.last_seen = at
            if not track.snapshots or track.snapshots[-1] != snapshot:
                track.snapshots.append(snapshot)
        for key, track in self.object_tracks.items():
            if key not in visible_vectors and track.disappear_at is None:
                track.disappear_at = at
        for key, track in self.image_tracks.items():
            if key not in visible_images and track.disappear_at is None:
                track.disappear_at = at
        for key, track in self.point_tracks.items():
            if key not in visible_points and track.disappear_at is None:
                track.disappear_at = at
        for key, track in self.mesh_tracks.items():
            if key not in visible_meshes and track.disappear_at is None:
                track.disappear_at = at
        for key, track in self.custom_shader_tracks.items():
            if key not in visible_custom_shaders and track.disappear_at is None:
                track.disappear_at = at
        self.frames += 1

    def _semantic_draw_range(
        self,
        mobject: VMobject | OpenGLVMobject,
        commands: list[dict[str, Any]],
    ) -> tuple[list[dict[str, Any]], float, float]:
        animation = getattr(mobject, "anim", None)
        if (
            animation is not None
            and type(animation).__name__ == "ShowPassingFlash"
            and hasattr(mobject, "time")
            and hasattr(animation, "starting_mobject")
            and hasattr(animation, "_get_bounds")
        ):
            try:
                alpha = float(mobject.time) / max(
                    float(animation.run_time), 1e-9
                )
                sub_alpha = animation.get_sub_alpha(
                    float(np.clip(alpha, 0.0, 1.0)), 0, 1
                )
                lower, upper = animation._get_bounds(sub_alpha)
                verified = self._verified_partial_commands(
                    animation.starting_mobject,
                    lower,
                    upper,
                    commands,
                )
                if verified is not None:
                    return verified, finite(lower), finite(upper)
            except Exception:
                pass

        for active in self.active_animations:
            animation_name = type(active).__name__
            if animation_name not in {
                "Create",
                "Uncreate",
                "Write",
                "DrawBorderThenFill",
            }:
                continue
            try:
                families = list(active.get_all_families_zipped())
                for index, family in enumerate(families):
                    if not family or family[0] is not mobject:
                        continue
                    alpha = float(np.clip(
                        self.active_local_time
                        / max(float(active.run_time), 1e-9),
                        0.0,
                        1.0,
                    ))
                    sub_alpha = active.get_sub_alpha(
                        alpha, index, len(families)
                    )
                    if animation_name in {"Create", "Uncreate"}:
                        lower, upper = active._get_bounds(sub_alpha)
                        starting = family[1]
                    else:
                        lower = 0.0
                        upper = min(1.0, max(0.0, sub_alpha * 2.0))
                        starting = family[2]
                    verified = self._verified_partial_commands(
                        starting, lower, upper, commands
                    )
                    if verified is not None:
                        return verified, finite(lower), finite(upper)
            except Exception:
                continue
        return [], 0.0, 1.0

    def _verified_partial_commands(
        self,
        starting: Any,
        lower: float,
        upper: float,
        commands: list[dict[str, Any]],
    ) -> list[dict[str, Any]] | None:
        if not isinstance(starting, (VMobject, OpenGLVMobject)):
            return None
        expected = starting.copy()
        expected.pointwise_become_partial(starting, lower, upper)
        expected_commands = (
            projected_opengl_commands(self.camera, expected)
            if isinstance(expected, OpenGLVMobject)
            else projected_commands(self.camera, expected)
        )
        if expected_commands != commands:
            return None
        base_commands = (
            projected_opengl_commands(self.camera, starting)
            if isinstance(starting, OpenGLVMobject)
            else projected_commands(self.camera, starting)
        )
        return base_commands or None

    def _capture_value_trackers(self, at: float) -> None:
        for tracker in REGISTERED_VALUE_TRACKERS:
            if isinstance(tracker, ComplexValueTracker):
                continue
            try:
                value = float(tracker.get_value())
            except (TypeError, ValueError):
                continue
            if not math.isfinite(value):
                continue
            snapshots = self.value_tracker_snapshots.setdefault(tracker, [])
            sample = (at, finite(value))
            if snapshots and snapshots[-1][0] == at:
                snapshots[-1] = sample
            else:
                snapshots.append(sample)

    def _capture_camera_2d(self, at: float) -> None:
        if not isinstance(self.camera, ManimMovingCamera):
            return
        center = np.asarray(self.camera.frame_center, dtype=float)
        frame_width = float(self.camera.frame_width)
        frame_height = float(self.camera.frame_height)
        snapshot = Camera2dSnapshot(
            at=at,
            x=finite(center[0]),
            y=finite(center[1]),
            zoom=finite(OUTPUT_WIDTH / max(frame_width, 1e-9)),
            aspect_matches=bool(
                abs(frame_width / max(frame_height, 1e-9) - OUTPUT_WIDTH / OUTPUT_HEIGHT)
                <= 0.000001
            ),
        )
        if (
            not self.camera_2d_snapshots
            or self.camera_2d_snapshots[-1] != snapshot
        ):
            self.camera_2d_snapshots.append(snapshot)

    def _capture_camera_3d(self, at: float) -> None:
        if not isinstance(self.camera, ThreeDCamera):
            return
        if bool(getattr(self.camera, "exponential_projection", False)):
            return
        rotation = np.asarray(
            self.camera.get_rotation_matrix(), dtype=float
        )
        center = np.asarray(self.camera.frame_center, dtype=float)
        focal = float(self.camera.get_focal_distance())
        zoom = float(self.camera.get_zoom())
        position = center + rotation.T @ np.asarray([0.0, 0.0, focal])
        up = rotation.T @ np.asarray([0.0, 1.0, 0.0])
        frame_height = float(self.camera.frame_height)
        snapshot = Camera3dSnapshot(
            at=at,
            position=[finite(channel) for channel in position],
            target=[finite(channel) for channel in center],
            up=[finite(channel) for channel in up],
            fov_y=finite(
                2 * math.atan(frame_height / (2 * focal * max(zoom, 1e-9)))
            ),
        )
        if (
            not self.camera_3d_snapshots
            or self.camera_3d_snapshots[-1] != snapshot
        ):
            self.camera_3d_snapshots.append(snapshot)

    def _capture_cairo_surface(
        self,
        surface: Surface,
        pieces: list[VMobject],
        at: float,
        order: int,
    ) -> tuple[Any, int] | None:
        vertices: list[list[float]] = []
        normals: list[list[float]] = []
        colors: list[str] = []
        rgbas: list[np.ndarray] = []
        triangles: list[list[int]] = []
        surface_vertices: list[list[float]] = []
        surface_vertex_indices: dict[tuple[float, float, float], int] = {}
        surface_patches: list[list[int]] = []
        surface_fill_rgbas: list[np.ndarray] = []
        surface_stroke_rgbas: list[np.ndarray] = []
        surface_stroke_radii: list[float] = []
        for piece in pieces:
            anchors: list[np.ndarray] = []
            for point in np.asarray(piece.get_anchors(), dtype=float):
                if not anchors or not np.allclose(point, anchors[-1], atol=1e-9):
                    anchors.append(point)
            if len(anchors) > 1 and np.allclose(
                anchors[0], anchors[-1], atol=1e-9
            ):
                anchors.pop()
            if len(anchors) < 3:
                # Parametric poles and collapsed cells have zero visible area.
                continue
            patch: list[int] = []
            for point in anchors:
                compact_point = tuple(finite(channel) for channel in point)
                compact_index = surface_vertex_indices.get(compact_point)
                if compact_index is None:
                    compact_index = len(surface_vertices)
                    surface_vertex_indices[compact_point] = compact_index
                    surface_vertices.append(list(compact_point))
                patch.append(compact_index)
            surface_patches.append(patch)
            base = len(vertices)
            vertices.extend(
                [
                    [finite(channel) for channel in point]
                    for point in anchors
                ]
            )
            normal = np.zeros(3, dtype=float)
            for first, second in zip(
                anchors, anchors[1:] + anchors[:1]
            ):
                normal += np.cross(first, second)
            length = float(np.linalg.norm(normal))
            if length > 1e-12:
                normal /= length
            else:
                normal = np.asarray([0.0, 0.0, 1.0])
            normals.extend(
                [[finite(channel) for channel in normal] for _ in anchors]
            )
            triangles.extend(
                [
                    [base, base + corner, base + corner + 1]
                    for corner in range(1, len(anchors) - 1)
                ]
            )
            fill_rgbas = np.asarray(
                self.camera.get_fill_rgbas(piece), dtype=float
            )
            if fill_rgbas.ndim != 2 or fill_rgbas.shape[1] != 4:
                self.diagnostics.add("unsupported-cairo-surface-color")
                return None
            start_color = np.clip(fill_rgbas[0], 0.0, 1.0)
            end_color = np.clip(fill_rgbas[-1], 0.0, 1.0)
            if len(anchors) == 4:
                piece_colors = [
                    start_color,
                    start_color,
                    end_color,
                    end_color,
                ]
            else:
                piece_colors = [
                    (1.0 - alpha) * start_color + alpha * end_color
                    for alpha in np.linspace(0.0, 1.0, len(anchors))
                ]
            rgbas.extend(piece_colors)
            colors.extend(rgba_hex(color) for color in piece_colors)
            surface_fill_rgbas.extend(piece_colors)
            stroke_width = float(np.max(piece.get_stroke_width()))
            stroke_rgbas = np.asarray(
                self.camera.get_stroke_rgbas(piece), dtype=float
            )
            half_width = (
                stroke_width
                * float(self.camera.cairo_line_width_multiple)
                * OUTPUT_WIDTH
                / float(self.camera.frame_width)
                * 0.5
            )
            stroke_color = (
                np.clip(stroke_rgbas[0], 0.0, 1.0)
                if stroke_rgbas.ndim == 2
                and stroke_rgbas.shape[0] > 0
                and stroke_rgbas.shape[1] == 4
                else np.asarray([0.0, 0.0, 0.0, 0.0])
            )
            surface_stroke_rgbas.append(stroke_color)
            surface_stroke_radii.append(finite(half_width))
            for edge_start, edge_end in zip(
                anchors, anchors[1:] + anchors[:1]
            ):
                direction = np.asarray(edge_end) - np.asarray(edge_start)
                perpendicular = np.cross(normal, direction)
                perpendicular_length = float(np.linalg.norm(perpendicular))
                if perpendicular_length <= 1e-12:
                    perpendicular = np.asarray([1.0, 0.0, 0.0])
                    perpendicular_length = 1.0
                offset = (
                    perpendicular / perpendicular_length * half_width
                )
                wire_base = len(vertices)
                wire_vertices = [
                    np.asarray(edge_start) - offset,
                    np.asarray(edge_start) + offset,
                    np.asarray(edge_end) - offset,
                    np.asarray(edge_end) + offset,
                ]
                vertices.extend(
                    [
                        [finite(channel) for channel in point]
                        for point in wire_vertices
                    ]
                )
                normals.extend(
                    [
                        [finite(channel) for channel in normal]
                        for _ in wire_vertices
                    ]
                )
                triangles.extend(
                    [
                        [wire_base, wire_base + 1, wire_base + 2],
                        [wire_base + 2, wire_base + 1, wire_base + 3],
                    ]
                )
                rgbas.extend([stroke_color] * 4)
                colors.extend([rgba_hex(stroke_color)] * 4)
        key = self.mesh_active.get(surface)
        track = None if key is None else self.mesh_tracks.get(key)
        if (
            track is None
            or track.disappear_at is not None
            or track.triangles != triangles
            or (
                track.snapshots
                and len(track.snapshots[0].vertices) != len(vertices)
            )
            or (
                self.enable_compact_surface_lifetimes
                and track.snapshots
                and (
                    len(track.snapshots[0].surface_vertices)
                    != len(surface_vertices)
                    or track.snapshots[0].surface_patches
                    != surface_patches
                    or len(track.snapshots[0].surface_stroke_radii)
                    != len(surface_stroke_radii)
                )
            )
        ):
            if track is not None and track.disappear_at is None:
                track.disappear_at = at
            key = self._new_lifetime_key(
                surface, self.mesh_active, self.mesh_generations
            )
            track = MeshTrack(
                node_id=f"manim-mesh-{len(self.mesh_tracks):05d}",
                first_seen=at,
                last_seen=at,
                triangles=triangles,
                double_sided=True,
                unlit=True,
                gloss=0.0,
                shadow=0.0,
                uvs=[],
                texture_pixels="",
                texture_width=0,
                texture_height=0,
                dark_texture_pixels="",
                dark_texture_width=0,
                dark_texture_height=0,
                texture_resampling="bicubic",
            )
            self.mesh_tracks[key] = track
        track.last_seen = at
        rgba_array = np.asarray(rgbas, dtype=float)
        track.snapshots.append(
            MeshSnapshot(
                at=at,
                vertices=vertices,
                normals=normals,
                light_position=[[0.0, 0.0, 8.0]],
                uvs=[],
                colors=colors,
                rgbas=rgba_array,
                opacity=finite(float(np.max(rgba_array[:, 3]))),
                z_index=int(getattr(surface, "z_index", 0)) * 10_000
                + order,
                surface_vertices=surface_vertices,
                surface_patches=surface_patches,
                surface_rgbas=np.asarray(
                    surface_fill_rgbas + surface_stroke_rgbas,
                    dtype=float,
                ),
                surface_stroke_radii=surface_stroke_radii,
            )
        )
        return key

    def _new_custom_shader_key(
        self, mobject: OpenGLMobject, wrapper_index: int
    ) -> tuple[OpenGLMobject, int, int]:
        slot = (mobject, wrapper_index)
        generation = self.custom_shader_generations.get(slot, -1) + 1
        self.custom_shader_generations[slot] = generation
        key = (mobject, wrapper_index, generation)
        self.custom_shader_active[slot] = key
        return key

    @staticmethod
    def _new_lifetime_key(
        owner: Any,
        active: dict[Any, Any],
        generations: dict[Any, int],
    ) -> tuple[Any, int]:
        generation = generations.get(owner, -1) + 1
        generations[owner] = generation
        key = (owner, generation)
        active[owner] = key
        return key

    def _capture_custom_opengl_shaders(
        self, mobject: OpenGLMobject, at: float, order: int
    ) -> list[tuple[OpenGLMobject, int, int]] | None:
        try:
            wrappers = list(mobject.get_shader_wrapper_list())
        except Exception:
            return None
        wrappers = [
            point_size_geometry_wrapper(wrapper) for wrapper in wrappers
        ]
        if any(wrapper is None for wrapper in wrappers):
            return None
        if not wrappers or any(
            wrapper.program_code.get("geometry_shader") for wrapper in wrappers
        ):
            stock_vmobject_folders = {
                "quadratic_bezier_fill",
                "quadratic_bezier_stroke",
                "vectorized_mobject_fill",
                "vectorized_mobject_stroke",
            }
            custom_vmobject_program = isinstance(
                mobject, OpenGLVMobject
            ) and any(
                not uses_stock_manim_shader(
                    wrapper, stock_vmobject_folders
                )
                for wrapper in wrappers
            )
            if (
                wrappers
                and (
                    not isinstance(mobject, OpenGLVMobject)
                    or custom_vmobject_program
                )
                and all(
                    wrapper.program_code.get("geometry_shader")
                    for wrapper in wrappers
                )
            ):
                return self._capture_expanded_geometry_shaders(
                    mobject, wrappers, at, order
                )
            return None
        captured: list[tuple[OpenGLMobject, int, int]] = []
        for wrapper_index, wrapper in enumerate(wrappers):
            vertex_source = wrapper.program_code.get("vertex_shader")
            fragment_source = wrapper.program_code.get("fragment_shader")
            if not vertex_source or not fragment_source:
                return None
            vert_data = np.asarray(wrapper.vert_data)
            dtype = vert_data.dtype
            if (
                len(vert_data) == 0
                or not dtype.names
                or dtype.itemsize <= 0
                or dtype.itemsize % 4
            ):
                return None
            topology = shader_indices_and_primitive(wrapper, len(vert_data))
            if topology is None:
                return None
            current_indices, current_primitive = topology
            slot = (mobject, wrapper_index)
            key = self.custom_shader_active.get(slot)
            track = (
                None
                if key is None
                else self.custom_shader_tracks.get(key)
            )
            signature = (vertex_source, fragment_source)
            if track is not None and (
                track.disappear_at is not None
                or track.program_signature != signature
            ):
                track.disappear_at = at
                track = None
            if track is None:
                key = self._new_custom_shader_key(
                    mobject, wrapper_index
                )
                try:
                    translated = translate_shader_program(
                        vertex_source,
                        fragment_source,
                        list(wrapper.vert_attributes or dtype.names),
                    )
                except Exception:
                    return None
                shader_attributes: list[dict[str, Any]] = []
                for attribute in translated["attributes"]:
                    name = attribute["name"]
                    source_name = attribute.get("sourceName", name)
                    field = dtype.fields.get(source_name)
                    if field is None:
                        return None
                    field_dtype, offset = field[:2]
                    source_column = attribute.get("sourceColumn")
                    if source_column is None:
                        layout = shader_field_layout(field_dtype)
                        if layout is None:
                            return None
                        format_prefix, count = layout
                    else:
                        base_dtype, shape = (
                            field_dtype.subdtype
                            if field_dtype.subdtype is not None
                            else (field_dtype, ())
                        )
                        dimension = int(
                            attribute["type"].removeprefix("vec")
                        )
                        if (
                            base_dtype != np.dtype(np.float32)
                            or int(np.prod(shape)) != dimension * dimension
                            or source_column < 0
                            or source_column >= dimension
                        ):
                            return None
                        format_prefix = "float32"
                        count = dimension
                        offset = int(offset) + source_column * dimension * 4
                    shader_attributes.append(
                        {
                            "name": name,
                            "_sourceName": source_name,
                            **(
                                {"_sourceColumn": int(source_column)}
                                if source_column is not None
                                else {}
                            ),
                            "location": int(attribute["location"]),
                            "offset": int(offset),
                            "format": format_prefix
                            + (f"x{count}" if count > 1 else ""),
                        }
                    )
                uniform_descriptors = self._custom_shader_uniforms(
                    mobject, wrapper, translated["uniformBindings"]
                )
                if uniform_descriptors is None:
                    return None
                track = CustomShaderTrack(
                    node_id=(
                        f"manim-shader-{len(self.custom_shader_tracks):05d}"
                    ),
                    first_seen=at,
                    last_seen=at,
                    vertex_wgsl=translated["vertexWgsl"],
                    fragment_wgsl=translated["fragmentWgsl"],
                    attributes=shader_attributes,
                    vertex_stride=int(dtype.itemsize),
                    indices=current_indices,
                    primitive=current_primitive,
                    uniforms=uniform_descriptors,
                    depth_test=bool(wrapper.depth_test),
                    program_signature=signature,
                )
                self.custom_shader_tracks[key] = track
            current_uniforms = self._custom_shader_uniforms(
                mobject,
                wrapper,
                [
                    {
                        "name": uniform["name"],
                        "type": uniform["type"],
                        "binding": uniform["binding"],
                        "arrayLength": uniform.get("arrayLength", 1),
                        **(
                            {"samplerBinding": uniform["samplerBinding"]}
                            if "samplerBinding" in uniform
                            else {}
                        ),
                    }
                    for uniform in track.uniforms
                ],
            )
            if current_uniforms is None:
                return None
            stable_uniforms = [
                {
                    key: value
                    for key, value in uniform.items()
                    if key != "values"
                }
                for uniform in track.uniforms
            ]
            current_stable = [
                {
                    key: value
                    for key, value in uniform.items()
                    if key != "values"
                }
                for uniform in current_uniforms
            ]
            current_attributes = []
            for attribute in track.attributes:
                source_name = attribute.get(
                    "_sourceName", attribute["name"]
                )
                field = dtype.fields.get(source_name)
                if field is None:
                    return None
                field_dtype, offset = field[:2]
                source_column = attribute.get("_sourceColumn")
                if source_column is None:
                    layout = shader_field_layout(field_dtype)
                    if layout is None:
                        return None
                    format_prefix, count = layout
                else:
                    base_dtype, shape = (
                        field_dtype.subdtype
                        if field_dtype.subdtype is not None
                        else (field_dtype, ())
                    )
                    count = int(round(math.sqrt(int(np.prod(shape)))))
                    if (
                        base_dtype != np.dtype(np.float32)
                        or count not in (2, 3, 4)
                        or int(np.prod(shape)) != count * count
                        or source_column < 0
                        or source_column >= count
                    ):
                        return None
                    format_prefix = "float32"
                    offset = int(offset) + source_column * count * 4
                current_attributes.append(
                    {
                        "name": attribute["name"],
                        "_sourceName": source_name,
                        **(
                            {"_sourceColumn": int(source_column)}
                            if source_column is not None
                            else {}
                        ),
                        "location": attribute["location"],
                        "offset": int(offset),
                        "format": format_prefix
                        + (f"x{count}" if count > 1 else ""),
                    }
                )
            vertex_data = structured_shader_vertex_values(
                vert_data,
                list(
                    dict.fromkeys(
                        attribute.get("_sourceName", attribute["name"])
                        for attribute in current_attributes
                    )
                ),
            )
            if vertex_data is None:
                return None
            if (
                stable_uniforms != current_stable
                or track.attributes != current_attributes
                or track.vertex_stride != int(dtype.itemsize)
                or track.indices != current_indices
                or track.primitive != current_primitive
                or track.depth_test != bool(wrapper.depth_test)
                or (
                    track.snapshots
                    and len(track.snapshots[0].vertex_data)
                    != len(vertex_data)
                )
            ):
                track.disappear_at = at
                key = self._new_custom_shader_key(
                    mobject, wrapper_index
                )
                track = replace(
                    track,
                    node_id=(
                        f"manim-shader-{len(self.custom_shader_tracks):05d}"
                    ),
                    first_seen=at,
                    last_seen=at,
                    attributes=current_attributes,
                    vertex_stride=int(dtype.itemsize),
                    indices=current_indices,
                    primitive=current_primitive,
                    uniforms=current_uniforms,
                    depth_test=bool(wrapper.depth_test),
                    snapshots=[],
                    disappear_at=None,
                )
                self.custom_shader_tracks[key] = track
            uniform_values = [
                finite(value)
                for uniform in current_uniforms
                for value in uniform["values"]
            ]
            snapshot = CustomShaderSnapshot(
                at=at,
                vertex_data=vertex_data,
                uniform_values=uniform_values,
                z_index=int(getattr(mobject, "z_index", 0)) * 10_000
                + order
                + wrapper_index,
            )
            track.last_seen = at
            if not track.snapshots or track.snapshots[-1] != snapshot:
                track.snapshots.append(snapshot)
            captured.append(key)
        return captured

    def _capture_expanded_geometry_shaders(
        self,
        mobject: OpenGLMobject,
        wrappers: list[Any],
        at: float,
        order: int,
    ) -> list[tuple[OpenGLMobject, int, int]] | None:
        captured: list[tuple[OpenGLMobject, int, int]] = []
        for wrapper_index, wrapper in enumerate(wrappers):
            try:
                expanded = self._expand_geometry_shader(wrapper)
            except Exception:
                return None
            if expanded is None:
                return None
            slot = (mobject, wrapper_index)
            key = self.custom_shader_active.get(slot)
            signature = (
                wrapper.program_code["vertex_shader"],
                wrapper.program_code["geometry_shader"],
                wrapper.program_code["fragment_shader"],
            )
            track = (
                None
                if key is None
                else self.custom_shader_tracks.get(key)
            )
            if track is not None and (
                track.disappear_at is not None
                or
                track.program_signature != signature
                or track.indices != expanded["indices"]
                or track.vertex_stride != expanded["vertexStride"]
                or track.primitive != expanded["primitive"]
                or (
                    track.snapshots
                    and len(track.snapshots[0].vertex_data)
                    != len(expanded["vertexData"])
                )
            ):
                track.disappear_at = at
                track = None
            if track is None:
                key = self._new_custom_shader_key(
                    mobject, wrapper_index
                )
                uniform_descriptors = self._custom_shader_uniforms(
                    mobject,
                    wrapper,
                    expanded["uniformBindings"],
                )
                if uniform_descriptors is None:
                    return None
                track = CustomShaderTrack(
                    node_id=(
                        f"manim-shader-{len(self.custom_shader_tracks):05d}"
                    ),
                    first_seen=at,
                    last_seen=at,
                    vertex_wgsl=expanded["vertexWgsl"],
                    fragment_wgsl=expanded["fragmentWgsl"],
                    attributes=expanded["attributes"],
                    vertex_stride=expanded["vertexStride"],
                    indices=expanded["indices"],
                    primitive=expanded["primitive"],
                    uniforms=uniform_descriptors,
                    depth_test=bool(wrapper.depth_test),
                    program_signature=signature,
                )
                self.custom_shader_tracks[key] = track
            current_uniforms = self._custom_shader_uniforms(
                mobject,
                wrapper,
                [
                    {
                        "name": uniform["name"],
                        "type": uniform["type"],
                        "binding": uniform["binding"],
                        "arrayLength": uniform.get("arrayLength", 1),
                        **(
                            {"samplerBinding": uniform["samplerBinding"]}
                            if "samplerBinding" in uniform
                            else {}
                        ),
                    }
                    for uniform in track.uniforms
                ],
            )
            if current_uniforms is None:
                return None
            stable_uniforms = [
                {
                    field: value
                    for field, value in uniform.items()
                    if field != "values"
                }
                for uniform in track.uniforms
            ]
            current_stable = [
                {
                    field: value
                    for field, value in uniform.items()
                    if field != "values"
                }
                for uniform in current_uniforms
            ]
            if stable_uniforms != current_stable:
                track.disappear_at = at
                key = self._new_custom_shader_key(
                    mobject, wrapper_index
                )
                track = replace(
                    track,
                    node_id=(
                        f"manim-shader-{len(self.custom_shader_tracks):05d}"
                    ),
                    first_seen=at,
                    last_seen=at,
                    uniforms=current_uniforms,
                    snapshots=[],
                    disappear_at=None,
                )
                self.custom_shader_tracks[key] = track
            snapshot = CustomShaderSnapshot(
                at=at,
                vertex_data=[
                    finite(value) for value in expanded["vertexData"]
                ],
                uniform_values=[
                    finite(value)
                    for uniform in current_uniforms
                    for value in uniform["values"]
                ],
                z_index=int(getattr(mobject, "z_index", 0)) * 10_000
                + order
                + wrapper_index,
            )
            track.last_seen = at
            if not track.snapshots or track.snapshots[-1] != snapshot:
                track.snapshots.append(snapshot)
            captured.append(key)
        return captured

    def _expand_geometry_shader(
        self, wrapper: Any
    ) -> dict[str, Any] | None:
        vertex_source = wrapper.program_code.get("vertex_shader")
        geometry_source = wrapper.program_code.get("geometry_shader")
        fragment_source = wrapper.program_code.get("fragment_shader")
        if not vertex_source or not geometry_source or not fragment_source:
            return None
        output_layout = re.search(
            r"layout\s*\(\s*(points|line_strip|triangle_strip)"
            r"\s*,\s*max_vertices\s*=\s*(\d+)\s*\)\s*out\s*;",
            geometry_source,
        )
        input_layout = re.search(
            r"layout\s*\(\s*"
            r"(points|lines|lines_adjacency|triangles|triangles_adjacency)"
            r"\s*\)\s*in\s*;",
            geometry_source,
        )
        if output_layout is None or input_layout is None:
            return None
        output_primitive = output_layout.group(1)
        max_vertices = int(output_layout.group(2))
        if max_vertices <= 0 or max_vertices > 256:
            return None
        fragment_uses_point_coord = "gl_PointCoord" in fragment_source
        geometry_sets_point_size = "gl_PointSize" in geometry_source
        emulate_point_output = output_primitive == "points" and (
            geometry_sets_point_size or fragment_uses_point_coord
        )
        output_field_matches = re.findall(
            r"^\s*((?:(?:flat|smooth|noperspective|centroid|sample|"
            r"invariant)\s+)*)out\s+"
            r"(float|vec2|vec3|vec4|int|ivec2|ivec3|ivec4|"
            r"uint|uvec2|uvec3|uvec4)\s+([A-Za-z_]\w*)\s*;",
            geometry_source,
            flags=re.MULTILINE,
        )
        output_fields = [
            (field_type, name, modifiers.strip())
            for modifiers, field_type, name in output_field_matches
        ]
        if not output_fields:
            return None
        point_size_output = (
            "out float realtime_manim_geometry_point_size;\n"
            if emulate_point_output
            else ""
        )
        marker_declarations = f"""
{point_size_output}
out float realtime_manim_marker;
out float realtime_manim_input_primitive;
out float realtime_manim_strip;
float realtime_manim_strip_value = 0.0;
"""
        geometry_source = re.sub(
            r"(#version\s+\d+\s*)",
            r"\1\n" + marker_declarations,
            geometry_source,
            count=1,
        )
        point_size_assignment = ""
        if emulate_point_output:
            point_size_assignment = (
                "realtime_manim_geometry_point_size = "
                + (
                    "max(gl_PointSize, 1.0);"
                    if geometry_sets_point_size
                    else "1.0;"
                )
            )
        geometry_source = re.sub(
            r"\bEmitVertex\s*\(\s*\)\s*;",
            f"""
            {point_size_assignment}
            realtime_manim_marker = 1.0;
            realtime_manim_input_primitive = float(gl_PrimitiveIDIn);
            realtime_manim_strip = realtime_manim_strip_value;
            EmitVertex();
            """,
            geometry_source,
        )
        geometry_source = re.sub(
            r"\bEndPrimitive\s*\(\s*\)\s*;",
            """
            EndPrimitive();
            realtime_manim_strip_value += 1.0;
            """,
            geometry_source,
        )
        varying_names = [
            "gl_Position",
            *(name for _field_type, name, _modifiers in output_fields),
            *(
                ["realtime_manim_geometry_point_size"]
                if emulate_point_output
                else []
            ),
            "realtime_manim_marker",
            "realtime_manim_input_primitive",
            "realtime_manim_strip",
        ]
        if self._gl_transform_context is None:
            self._gl_transform_context = moderngl.create_standalone_context(
                require=330
            )
        context = self._gl_transform_context
        program = context.program(
            vertex_shader=vertex_source,
            geometry_shader=geometry_source,
            varyings=varying_names,
        )
        values = self._opengl_uniform_values(wrapper)
        for name, value in values.items():
            try:
                program[name].value = value
            except (KeyError, TypeError, ValueError):
                pass
        gl_textures = []
        for texture_unit, (name, texture_value) in enumerate(
            wrapper.texture_paths.items()
        ):
            try:
                image = (
                    texture_value
                    if hasattr(texture_value, "convert")
                    else Image.open(texture_value)
                )
                pixels = np.asarray(
                    image.convert("RGBA"), dtype=np.uint8
                )
                texture = context.texture(
                    (int(pixels.shape[1]), int(pixels.shape[0])),
                    4,
                    pixels.tobytes(),
                )
                texture.use(location=texture_unit)
                program[name].value = texture_unit
                gl_textures.append(texture)
            except (KeyError, OSError, ValueError):
                for texture in gl_textures:
                    texture.release()
                return None
        vert_data = np.asarray(wrapper.vert_data)
        declared_attributes = list(wrapper.vert_attributes or ())
        if not declared_attributes or not vert_data.dtype.names:
            return None
        # ModernGL's transform-feedback varying table can shadow an input
        # attribute with the same identifier (Manim's true_dot program uses
        # `point` and `color` at both ends of the pipeline). The dedicated
        # attribute-location map remains unambiguous.
        active_attribute_names = set(program._attribute_locations)
        attributes = [
            name
            for name in declared_attributes
            if name in active_attribute_names
        ]
        if not attributes:
            return None
        filtered_dtype = []
        for name in attributes:
            field = vert_data.dtype.fields.get(name)
            if field is None:
                return None
            filtered_dtype.append((name, field[0]))
        filtered_vert_data = np.zeros(len(vert_data), dtype=filtered_dtype)
        for name in attributes:
            filtered_vert_data[name] = vert_data[name]
        expected_offset = 0
        for name in attributes:
            field = filtered_vert_data.dtype.fields.get(name)
            if field is None:
                return None
            field_dtype, offset = field[:2]
            layout = shader_field_layout(field_dtype)
            if layout is None or int(offset) != expected_offset:
                return None
            _format_prefix, count = layout
            expected_offset += count * 4
        if expected_offset != filtered_vert_data.dtype.itemsize:
            return None
        vertex_buffer = context.buffer(filtered_vert_data.tobytes())
        indices = wrapper.vert_indices
        index_buffer = (
            None
            if indices is None
            else context.buffer(
                np.asarray(indices, dtype=np.int32).reshape(-1).tobytes()
            )
        )
        vao = context.vertex_array(
            program,
            [
                (
                    vertex_buffer,
                    moderngl.detect_format(program, attributes),
                    *attributes,
                )
            ],
            index_buffer,
        )
        draw_count = len(filtered_vert_data) if indices is None else len(indices)
        input_arity = {
            "points": 1,
            "lines": 2,
            "lines_adjacency": 4,
            "triangles": 3,
            "triangles_adjacency": 6,
        }[input_layout.group(1)]
        input_primitives = max(1, draw_count // input_arity)
        component_counts = {
            "float": 1,
            "vec2": 2,
            "vec3": 3,
            "vec4": 4,
            "int": 1,
            "ivec2": 2,
            "ivec3": 3,
            "ivec4": 4,
            "uint": 1,
            "uvec2": 2,
            "uvec3": 3,
            "uvec4": 4,
        }
        record_components = (
            4
            + sum(
                component_counts[field_type]
                for field_type, _name, _modifiers in output_fields
            )
            + (1 if emulate_point_output else 0)
            + 3
        )
        # Transform feedback returns geometry-shader strips as their assembled
        # primitive stream (line pairs or triangle triples), rather than the
        # original EmitVertex strip. Reserve for that expanded stream.
        records_per_input = {
            "points": max_vertices,
            "line_strip": max(0, max_vertices - 1) * 2,
            "triangle_strip": max(0, max_vertices - 2) * 3,
        }[output_primitive]
        output_buffer = context.buffer(
            reserve=max(
                4,
                input_primitives
                * records_per_input
                * record_components
                * 4,
            )
        )
        output_buffer.clear()
        transform_mode = {
            "points": moderngl.POINTS,
            "lines": moderngl.LINES,
            "lines_adjacency": moderngl.LINES_ADJACENCY,
            "triangles": moderngl.TRIANGLES,
            "triangles_adjacency": moderngl.TRIANGLES_ADJACENCY,
        }[input_layout.group(1)]
        vao.transform(
            output_buffer,
            mode=transform_mode,
            vertices=-1,
        )
        raw = np.frombuffer(output_buffer.read(), dtype=np.uint8).reshape(
            (-1, record_components * 4)
        )
        marker_values = (
            raw[:, -12:]
            .copy()
            .view(np.float32)
            .reshape((-1, 3))
        )
        emitted_mask = np.abs(marker_values[:, 0] - 1.0) < 0.01
        emitted_raw = raw[emitted_mask]
        for resource in [
            vao,
            vertex_buffer,
            index_buffer,
            output_buffer,
            program,
            *gl_textures,
        ]:
            if resource is not None:
                resource.release()
        if len(emitted_raw) == 0:
            return None
        emitted_parts = [
            emitted_raw[:, :16].copy().view(np.float32).reshape((-1, 4))
        ]
        source_offset = 16
        for field_type, _name, _modifiers in output_fields:
            count = component_counts[field_type]
            field_bytes = emitted_raw[
                :, source_offset : source_offset + count * 4
            ].copy()
            if field_type.startswith("i") or field_type == "int":
                field_values = field_bytes.view(np.int32)
            elif field_type.startswith("u") or field_type == "uint":
                field_values = field_bytes.view(np.uint32)
            else:
                field_values = field_bytes.view(np.float32)
            emitted_parts.append(
                field_values.reshape((-1, count)).astype(np.float64)
            )
            source_offset += count * 4
        emitted_values = np.concatenate(emitted_parts, axis=1)
        if emulate_point_output:
            point_sizes = (
                emitted_raw[:, source_offset : source_offset + 4]
                .copy()
                .view(np.float32)
                .reshape((-1, 1))
            )
            corners = np.asarray(
                [
                    [-1.0, -1.0],
                    [1.0, -1.0],
                    [-1.0, 1.0],
                    [-1.0, 1.0],
                    [1.0, -1.0],
                    [1.0, 1.0],
                ],
                dtype=np.float64,
            )
            point_coords = np.asarray(
                [
                    [0.0, 0.0],
                    [1.0, 0.0],
                    [0.0, 1.0],
                    [0.0, 1.0],
                    [1.0, 0.0],
                    [1.0, 1.0],
                ],
                dtype=np.float64,
            )
            expanded_points: list[np.ndarray] = []
            for emitted, point_size in zip(
                emitted_values, point_sizes[:, 0], strict=True
            ):
                position = emitted[:4]
                radius = max(float(point_size), 1.0) * position[3] / np.asarray(
                    [float(config.pixel_width), float(config.pixel_height)],
                    dtype=np.float64,
                )
                for corner, point_coord in zip(
                    corners, point_coords, strict=True
                ):
                    expanded_position = position.copy()
                    expanded_position[:2] += corner * radius
                    parts = [expanded_position, emitted[4:]]
                    if fragment_uses_point_coord:
                        parts.append(point_coord)
                    expanded_points.append(np.concatenate(parts))
            emitted_values = np.asarray(expanded_points, dtype=np.float64)
            if fragment_uses_point_coord:
                point_coord_name = "realtime_manim_geometry_point_coord"
                fragment_source = re.sub(
                    r"\bgl_PointCoord\b",
                    point_coord_name,
                    fragment_source,
                )
                fragment_source = re.sub(
                    r"(#version\s+\d+\s*)",
                    rf"\1\nin vec2 {point_coord_name};\n",
                    fragment_source,
                    count=1,
                )
                output_fields.append(("vec2", point_coord_name, ""))
            primitive = "triangle-list"
        elif output_primitive == "points":
            primitive = "point-list"
        elif output_primitive == "line_strip":
            primitive = "line-list"
        else:
            primitive = "triangle-list"
        expanded_indices = list(range(len(emitted_values)))
        if not expanded_indices:
            return None
        pass_inputs = ["rm_position"]
        vertex_lines = [
            "#version 330",
            "in vec4 rm_position;",
            "out vec4 realtime_manim_unused_position;",
        ]
        main_lines = [
            "void main() {",
            "gl_Position = rm_position;",
            "realtime_manim_unused_position = rm_position;",
        ]
        offset = 16
        shader_attributes = [
            {
                "name": "rm_position",
                "location": 0,
                "offset": 0,
                "format": "float32x4",
            }
        ]
        for field_index, (field_type, name, modifiers) in enumerate(
            output_fields, start=1
        ):
            input_name = f"rm_attribute_{field_index}"
            pass_inputs.append(input_name)
            vertex_lines.append(f"in {field_type} {input_name};")
            modifier_prefix = f"{modifiers} " if modifiers else ""
            vertex_lines.append(
                f"{modifier_prefix}out {field_type} {name};"
            )
            main_lines.append(f"{name} = {input_name};")
            count = component_counts[field_type]
            format_prefix = (
                "sint32"
                if field_type.startswith("i") or field_type == "int"
                else (
                    "uint32"
                    if field_type.startswith("u") or field_type == "uint"
                    else "float32"
                )
            )
            shader_attributes.append(
                {
                    "name": input_name,
                    "location": field_index,
                    "offset": offset,
                    "format": format_prefix
                    + (f"x{count}" if count > 1 else ""),
                }
            )
            offset += count * 4
        main_lines.append("}")
        pass_vertex_source = "\n".join(
            [*vertex_lines, "", *main_lines, ""]
        )
        translated = translate_shader_program(
            pass_vertex_source,
            fragment_source,
            pass_inputs,
        )
        translated.update(
            {
                "attributes": shader_attributes,
                "vertexStride": offset,
                "vertexData": emitted_values.reshape(-1).tolist(),
                "indices": expanded_indices,
                "primitive": primitive,
            }
        )
        return translated

    def _opengl_uniform_values(self, wrapper: Any) -> dict[str, Any]:
        camera = self.camera
        frame_width, frame_height = camera.get_shape()
        rotation = np.asarray(camera.inverse_rotation_matrix, dtype=float)
        light_position = rotation @ np.asarray(
            camera.light_source.get_location(), dtype=float
        )
        return {
            **wrapper.uniforms,
            "frame_shape": (frame_width, frame_height),
            "anti_alias_width": 1.5
            / (float(config.pixel_height) / float(frame_height)),
            "camera_center": tuple(camera.get_center()),
            "camera_rotation": tuple(rotation.T.flatten()),
            "light_source_position": tuple(light_position),
            "focal_distance": float(camera.get_focal_distance()),
            "realtime_manim_viewport_pixels": (
                float(config.pixel_width),
                float(config.pixel_height),
            ),
            "u_model_matrix": tuple(np.eye(4).T.flatten()),
            "u_view_matrix": tuple(camera.formatted_view_matrix),
            "u_projection_matrix": tuple(camera.projection_matrix),
        }

    def _custom_shader_uniforms(
        self,
        mobject: OpenGLMobject,
        wrapper: Any,
        bindings: list[dict[str, Any]],
    ) -> list[dict[str, Any]] | None:
        values = self._opengl_uniform_values(wrapper)
        descriptors: list[dict[str, Any]] = []
        texture_paths = dict(wrapper.texture_paths)
        value_counts = {
            "float": 1,
            "vec2": 2,
            "vec3": 3,
            "vec4": 4,
            "mat3": 9,
            "mat4": 16,
            "int": 1,
            "ivec2": 2,
            "ivec3": 3,
            "ivec4": 4,
            "uint": 1,
            "uvec2": 2,
            "uvec3": 3,
            "uvec4": 4,
            "bool": 1,
            "bvec2": 2,
            "bvec3": 3,
            "bvec4": 4,
        }
        for binding in bindings:
            name = binding["name"]
            uniform_type = binding["type"]
            descriptor: dict[str, Any] = {
                "name": name,
                "binding": int(binding["binding"]),
                "type": uniform_type,
                "arrayLength": int(binding.get("arrayLength", 1)),
            }
            if uniform_type == "sampler2D":
                texture_value = texture_paths.get(name)
                if texture_value is None:
                    return None
                try:
                    image = (
                        texture_value
                        if hasattr(texture_value, "convert")
                        else Image.open(texture_value)
                    )
                    pixels = np.asarray(
                        image.convert("RGBA"), dtype=np.uint8
                    )
                except Exception:
                    return None
                descriptor.update(
                    {
                        "samplerBinding": int(binding["samplerBinding"]),
                        "values": [],
                        "texturePixels": base64.b64encode(
                            pixels.tobytes()
                        ).decode("ascii"),
                        "textureWidth": int(pixels.shape[1]),
                        "textureHeight": int(pixels.shape[0]),
                    }
                )
            else:
                base_count = value_counts.get(uniform_type)
                array_length = descriptor["arrayLength"]
                if (
                    base_count is None
                    or array_length < 1
                    or array_length > 256
                ):
                    return None
                count = base_count * array_length
                raw_value = values.get(name, 0.0)
                array = np.asarray(raw_value, dtype=float).reshape(-1)
                if len(array) == 1 and count > 1:
                    array = np.repeat(array, count)
                if len(array) != count:
                    return None
                descriptor.update(
                    {
                        "values": [finite(value) for value in array],
                        "texturePixels": "",
                        "textureWidth": 0,
                        "textureHeight": 0,
                    }
                )
            descriptors.append(descriptor)
        return descriptors

    def _capture_generic_opengl_mesh(
        self, mobject: OpenGLMobject, at: float, order: int
    ) -> bool:
        if len(getattr(mobject, "points", ())) < 3:
            return False
        shader_folder = Path(str(getattr(mobject, "shader_folder", ""))).name
        if shader_folder not in {"default", "vertex_colors", "surface", "manim_coords"}:
            return False
        if shader_folder == "surface" and (
            abs(float(getattr(mobject, "gloss", 0.0))) > 1e-6
            or abs(float(getattr(mobject, "shadow", 0.0))) > 1e-6
        ):
            return False
        try:
            shader_data = np.asarray(mobject.get_shader_data())
        except Exception:
            return False
        field_names = set(shader_data.dtype.names or ())
        position_field = next(
            (
                field
                for field in ("point", "in_vert", "position")
                if field in field_names
            ),
            None,
        )
        if position_field is None:
            return False
        source_points = np.asarray(shader_data[position_field], dtype=float)
        if source_points.ndim != 2 or source_points.shape[1] not in (3, 4):
            return False
        source_points = source_points[:, :3]
        color_field = next(
            (
                field
                for field in ("color", "in_color")
                if field in field_names
            ),
            None,
        )
        if color_field is None:
            uniform_color = getattr(mobject, "uniforms", {}).get(
                "u_color", (1, 1, 1, 1)
            )
            rgbas = np.repeat(
                [np.asarray(uniform_color, dtype=float)], len(source_points), axis=0
            )
        else:
            rgbas = np.asarray(shader_data[color_field], dtype=float)
            if rgbas.shape == (1, 4):
                rgbas = np.repeat(rgbas, len(source_points), axis=0)
        if rgbas.shape != (len(source_points), 4):
            return False
        indices_value = mobject.get_shader_vert_indices()
        indices = (
            np.arange(len(source_points), dtype=np.int64)
            if indices_value is None
            else np.asarray(indices_value, dtype=np.int64).reshape(-1)
        )
        primitive = int(getattr(mobject, "render_primitive", moderngl.TRIANGLES))
        if primitive == moderngl.TRIANGLES:
            if len(indices) < 3 or len(indices) % 3:
                return False
            triangles = indices.reshape((-1, 3))
        elif primitive == moderngl.TRIANGLE_STRIP:
            if len(indices) < 3:
                return False
            triangles = np.asarray(
                [
                    [indices[index], indices[index + 1], indices[index + 2]]
                    if index % 2 == 0
                    else [indices[index + 1], indices[index], indices[index + 2]]
                    for index in range(len(indices) - 2)
                ],
                dtype=np.int64,
            )
        elif primitive == moderngl.TRIANGLE_FAN:
            if len(indices) < 3:
                return False
            triangles = np.asarray(
                [
                    [indices[0], indices[index], indices[index + 1]]
                    for index in range(1, len(indices) - 1)
                ],
                dtype=np.int64,
            )
        else:
            return False
        if np.any(triangles < 0) or np.any(triangles >= len(source_points)):
            return False
        vertices = opengl_camera_space_points(
            self.camera, mobject, source_points
        )
        key = self.mesh_active.get(mobject)
        track = None if key is None else self.mesh_tracks.get(key)
        triangle_list = triangles.tolist()
        if (
            track is None
            or track.disappear_at is not None
            or track.triangles != triangle_list
        ):
            if track is not None and track.disappear_at is None:
                track.disappear_at = at
            key = self._new_lifetime_key(
                mobject, self.mesh_active, self.mesh_generations
            )
            track = MeshTrack(
                node_id=f"manim-mesh-{len(self.mesh_tracks):05d}",
                first_seen=at,
                last_seen=at,
                triangles=triangle_list,
                double_sided=True,
                unlit=True,
                gloss=0.0,
                shadow=0.0,
                uvs=[],
                texture_pixels="",
                texture_width=0,
                texture_height=0,
                dark_texture_pixels="",
                dark_texture_width=0,
                dark_texture_height=0,
                texture_resampling="bicubic",
            )
            self.mesh_tracks[key] = track
        track.last_seen = at
        clipped_rgbas = np.clip(rgbas, 0.0, 1.0)
        track.snapshots.append(
            MeshSnapshot(
                at=at,
                vertices=[
                    [finite(channel) for channel in point] for point in vertices
                ],
                normals=[],
                light_position=[[-10.0, 10.0, 10.0]],
                uvs=[],
                colors=[rgba_hex(rgba) for rgba in clipped_rgbas],
                rgbas=clipped_rgbas.copy(),
                opacity=1.0,
                z_index=order,
            )
        )
        return True

    def _capture_opengl_surface(
        self, mobject: OpenGLSurface, at: float, order: int
    ) -> tuple[Any, int] | None:
        indices = np.asarray(mobject.triangle_indices, dtype=np.int64)
        if indices.ndim != 1 or len(indices) < 3 or len(indices) % 3:
            self.diagnostics.add("unsupported-opengl-surface-indices")
            return None
        triangles_array = indices.reshape((-1, 3))
        vertex_count = int(np.max(triangles_array)) + 1
        source_points = np.asarray(mobject.points[:vertex_count], dtype=float)
        if len(source_points) != vertex_count or source_points.shape[1] != 3:
            self.diagnostics.add("unsupported-opengl-surface-layout")
            return None
        all_points = np.asarray(mobject.points, dtype=float)
        if len(all_points) < vertex_count * 3:
            self.diagnostics.add("unsupported-opengl-surface-normals")
            return None
        source_du = all_points[vertex_count : vertex_count * 2]
        source_dv = all_points[vertex_count * 2 : vertex_count * 3]
        vertices = opengl_camera_space_points(
            self.camera, mobject, source_points
        )
        du_vertices = opengl_camera_space_points(
            self.camera, mobject, source_du
        )
        dv_vertices = opengl_camera_space_points(
            self.camera, mobject, source_dv
        )
        normals = np.cross(du_vertices - vertices, dv_vertices - vertices)
        normal_lengths = np.linalg.norm(normals, axis=1)
        valid_normals = normal_lengths > 1e-12
        normals[valid_normals] /= normal_lengths[valid_normals, None]
        normals[~valid_normals] = np.asarray([0.0, 0.0, 1.0])
        light_location = np.asarray(
            self.camera.light_source.get_location(), dtype=float
        )
        light_position = self.camera.inverse_rotation_matrix @ light_location
        rgbas = np.asarray(mobject.rgbas, dtype=float)
        if rgbas.shape == (1, 4):
            rgbas = np.repeat(rgbas, vertex_count, axis=0)
        elif len(rgbas) != vertex_count:
            self.diagnostics.add("unsupported-opengl-surface-colors")
            return None
        uvs: list[list[float]] = []
        texture_pixels = ""
        texture_width = 0
        texture_height = 0
        dark_texture_pixels = ""
        dark_texture_width = 0
        dark_texture_height = 0
        texture_resampling = "bicubic"
        if isinstance(mobject, OpenGLTexturedSurface):
            texture_coordinates = np.asarray(mobject.im_coords, dtype=float)
            if texture_coordinates.shape != (vertex_count, 2):
                self.diagnostics.add("unsupported-opengl-texture-coordinates")
                return None
            uvs = [
                [finite(channel) for channel in coordinate]
                for coordinate in texture_coordinates
            ]
            textures = getattr(mobject, "texture_paths", {})
            light_texture = textures.get("LightTexture")
            dark_texture = textures.get("DarkTexture")
            if light_texture is None:
                self.diagnostics.add("missing-opengl-light-texture")
                return None
            light_image = (
                light_texture
                if hasattr(light_texture, "convert")
                else Image.open(light_texture)
            )
            light_array = np.asarray(light_image.convert("RGBA"), dtype=np.uint8)
            if dark_texture is not None:
                dark_image = (
                    dark_texture
                    if hasattr(dark_texture, "convert")
                    else Image.open(dark_texture)
                )
                dark_array = np.asarray(
                    dark_image.convert("RGBA"), dtype=np.uint8
                )
                if (
                    dark_array.shape != light_array.shape
                    or not np.array_equal(dark_array, light_array)
                ):
                    dark_texture_height, dark_texture_width = dark_array.shape[:2]
                    dark_texture_pixels = base64.b64encode(
                        dark_array.tobytes()
                    ).decode("ascii")
            texture_height, texture_width = light_array.shape[:2]
            texture_pixels = base64.b64encode(light_array.tobytes()).decode(
                "ascii"
            )
            resampling_name = str(
                getattr(
                    getattr(mobject, "resampling_algorithm", None),
                    "name",
                    "BICUBIC",
                )
            ).lower()
            texture_resampling = {
                "nearest": "nearest",
                "box": "box",
                "bilinear": "bilinear",
                "hamming": "hamming",
                "bicubic": "bicubic",
                "lanczos": "lanczos",
            }.get(resampling_name, "bicubic")
        triangle_list = triangles_array.tolist()
        gloss = finite(float(getattr(mobject, "gloss", 0.0)))
        shadow = finite(float(getattr(mobject, "shadow", 0.0)))
        key = self.mesh_active.get(mobject)
        track = None if key is None else self.mesh_tracks.get(key)
        static_changed = track is not None and (
            track.triangles != triangle_list
            or (
                track.snapshots
                and len(track.snapshots[0].vertices) != vertex_count
            )
            or abs(track.gloss - gloss) > 1e-6
            or abs(track.shadow - shadow) > 1e-6
            or track.uvs != uvs
            or track.texture_pixels != texture_pixels
            or track.texture_width != texture_width
            or track.texture_height != texture_height
            or track.dark_texture_pixels != dark_texture_pixels
            or track.dark_texture_width != dark_texture_width
            or track.dark_texture_height != dark_texture_height
            or track.texture_resampling != texture_resampling
        )
        if track is None or track.disappear_at is not None or static_changed:
            if track is not None and track.disappear_at is None:
                track.disappear_at = at
            key = self._new_lifetime_key(
                mobject, self.mesh_active, self.mesh_generations
            )
            track = MeshTrack(
                node_id=f"manim-mesh-{len(self.mesh_tracks):05d}",
                first_seen=at,
                last_seen=at,
                triangles=triangle_list,
                double_sided=True,
                unlit=False,
                gloss=gloss,
                shadow=shadow,
                uvs=uvs,
                texture_pixels=texture_pixels,
                texture_width=texture_width,
                texture_height=texture_height,
                dark_texture_pixels=dark_texture_pixels,
                dark_texture_width=dark_texture_width,
                dark_texture_height=dark_texture_height,
                texture_resampling=texture_resampling,
            )
            self.mesh_tracks[key] = track
        track.last_seen = at
        track.snapshots.append(
            MeshSnapshot(
                at=at,
                vertices=[
                    [finite(channel) for channel in point]
                    for point in vertices
                ],
                normals=[
                    [finite(channel) for channel in normal]
                    for normal in normals
                ],
                light_position=[
                    [finite(channel) for channel in light_position]
                ],
                uvs=uvs,
                colors=[rgba_hex(rgba) for rgba in np.clip(rgbas, 0.0, 1.0)],
                rgbas=np.clip(rgbas.copy(), 0.0, 1.0),
                opacity=finite(
                    float(
                        np.max(mobject.opacity)
                        if isinstance(mobject, OpenGLTexturedSurface)
                        else np.max(rgbas[:, 3])
                    )
                ),
                z_index=order,
            )
        )
        return key

    def _capture_point_cloud(
        self, mobject: Any, at: float, order: int
    ) -> tuple[Any, int] | None:
        points_3d = np.asarray(mobject.points, dtype=float)
        rgbas = np.asarray(mobject.rgbas, dtype=float)
        if rgbas.shape == (1, 4) and len(points_3d) > 1:
            rgbas = np.repeat(rgbas, len(points_3d), axis=0)
        if (
            points_3d.ndim != 2
            or points_3d.shape[1] != 3
            or rgbas.ndim != 2
            or rgbas.shape != (len(points_3d), 4)
            or len(points_3d) == 0
        ):
            self.diagnostics.add("unsupported-point-cloud-layout")
            return None
        projected = project_mobject_points(self.camera, mobject, points_3d)
        key = self.point_active.get(mobject)
        track = None if key is None else self.point_tracks.get(key)
        point_count_changed = bool(
            track is not None
            and track.snapshots
            and len(track.snapshots[0].points) != len(projected)
        )
        if (
            track is None
            or track.disappear_at is not None
            or point_count_changed
        ):
            if track is not None and track.disappear_at is None:
                track.disappear_at = at
            key = self._new_lifetime_key(
                mobject, self.point_active, self.point_generations
            )
            track = PointCloudTrack(
                node_id=f"manim-points-{len(self.point_tracks):05d}",
                first_seen=at,
                last_seen=at,
            )
            self.point_tracks[key] = track
        track.last_seen = at
        if isinstance(mobject, OpenGLPMobject):
            camera_points = opengl_camera_space_points(
                self.camera, mobject, points_3d
            )
            mean_z = float(np.mean(camera_points[:, 2]))
            focal = float(self.camera.get_focal_distance())
            frame_width, frame_height = self.camera.get_shape()
            geometry_scale = focal / max(focal - mean_z, 1e-6)
            radius_scale = 1.0 / max(
                1.0 - mean_z / focal / float(frame_height), 1e-6
            )
            radius = max(
                0.001,
                float(mobject.point_radius)
                * radius_scale
                * geometry_scale
                * OUTPUT_WIDTH
                / float(frame_width),
            )
        else:
            thickness = float(self.camera.adjusted_thickness(mobject.stroke_width))
            canonical_pixel_width = float(
                getattr(self.camera, "pixel_width", config.pixel_width)
            )
            radius = max(
                0.001,
                thickness * OUTPUT_WIDTH / canonical_pixel_width,
            )
        track.snapshots.append(
            PointCloudSnapshot(
                at=at,
                points=[
                    [finite(point[0]), finite(point[1])] for point in projected
                ],
                colors=[rgba_hex(rgba) for rgba in np.clip(rgbas, 0.0, 1.0)],
                rgbas=np.clip(rgbas.copy(), 0.0, 1.0),
                radius=finite(radius),
                z_index=int(getattr(mobject, "z_index", 0)) * 10_000 + order,
                world_points=(
                    [
                        [finite(point[0]), finite(point[1])]
                        for point in points_3d
                    ]
                    if isinstance(self.camera, ManimMovingCamera)
                    else []
                ),
            )
        )
        return key

    def _capture_image(
        self, mobject: AbstractImageMobject, at: float, order: int
    ) -> tuple[AbstractImageMobject, int] | None:
        pixels = np.asarray(mobject.get_pixel_array(), dtype=np.uint8)
        if pixels.ndim != 3 or pixels.shape[2] not in (3, 4):
            self.diagnostics.add("unsupported-image-pixel-format")
            return None
        if pixels.shape[2] == 3:
            pixels = np.concatenate(
                [pixels, np.full((*pixels.shape[:2], 1), 255, dtype=np.uint8)],
                axis=2,
            )
        current_resampling = {
            0: "nearest",
            4: "box",
            2: "bilinear",
            5: "hamming",
            3: "bicubic",
            1: "lanczos",
        }.get(int(getattr(mobject, "resampling_algorithm", 3)))
        if current_resampling is None:
            self.diagnostics.add("unsupported-image-resampling")
            return None

        def opacity_against(alpha: np.ndarray) -> tuple[float, bool]:
            if alpha.shape != pixels[:, :, 3].shape:
                return 1.0, False
            mask = alpha > 0
            if not np.any(mask):
                return 1.0, bool(np.all(pixels[:, :, 3] == 0))
            opacity_value = float(
                np.median(
                    pixels[:, :, 3][mask].astype(float)
                    / alpha[mask].astype(float)
                )
            )
            expected_alpha = np.rint(
                alpha.astype(float) * opacity_value
            ).astype(np.uint8)
            return opacity_value, bool(
                np.allclose(pixels[:, :, 3], expected_alpha, atol=1)
            )

        key = self.image_active.get(mobject)
        image_track = None if key is None else self.image_tracks.get(key)
        opacity = 1.0
        compatible = False
        if image_track is not None and image_track.disappear_at is None:
            opacity, alpha_compatible = opacity_against(
                image_track.original_alpha
            )
            visible_rgb = image_track.original_alpha > 0
            rgb_compatible = (
                pixels.shape == image_track.base_pixel_array.shape
                and (
                    not np.any(visible_rgb)
                    or np.allclose(
                        pixels[:, :, :3][visible_rgb],
                        image_track.base_pixel_array[:, :, :3][visible_rgb],
                        atol=1,
                    )
                )
            )
            compatible = bool(
                current_resampling == image_track.resampling
                and rgb_compatible
                and alpha_compatible
            )
        if image_track is None or not compatible:
            if image_track is not None and image_track.disappear_at is None:
                image_track.disappear_at = at
            candidate_alpha = np.asarray(
                getattr(mobject, "orig_alpha_pixel_array", pixels[:, :, 3]),
                dtype=np.uint8,
            )
            opacity, alpha_compatible = opacity_against(candidate_alpha)
            original_alpha = (
                candidate_alpha.copy()
                if alpha_compatible
                else pixels[:, :, 3].copy()
            )
            if not alpha_compatible:
                opacity = 1.0
            base_pixels = pixels.copy()
            base_pixels[:, :, 3] = original_alpha
            key = self._new_lifetime_key(
                mobject, self.image_active, self.image_generations
            )
            image_track = ImageTrack(
                node_id=f"manim-image-{len(self.image_tracks):05d}",
                first_seen=at,
                last_seen=at,
                pixels=base64.b64encode(base_pixels.tobytes()).decode("ascii"),
                pixel_width=int(base_pixels.shape[1]),
                pixel_height=int(base_pixels.shape[0]),
                base_pixel_array=base_pixels,
                original_alpha=original_alpha,
                resampling=current_resampling,
            )
            self.image_tracks[key] = image_track
        image_track.last_seen = at
        projected = projected_pixels(
            self.camera,
            self.camera.points_to_subpixel_coords(mobject, mobject.points),
        )
        image_track.snapshots.append(
            ImageSnapshot(
                at=at,
                corners=[
                    [finite(point[0]), finite(point[1])] for point in projected
                ],
                opacity=finite(max(0.0, min(1.0, opacity))),
                z_index=int(getattr(mobject, "z_index", 0)) * 10_000 + order,
                world_corners=(
                    [
                        [finite(point[0]), finite(point[1])]
                        for point in np.asarray(mobject.points, dtype=float)
                    ]
                    if isinstance(self.camera, ManimMovingCamera)
                    else []
                ),
            )
        )
        return key

    def _diagnose_non_vector(self, root: Mobject) -> None:
        for mobject in root.get_family():
            if isinstance(mobject, AbstractImageMobject):
                continue
            if isinstance(mobject, (PMobject, OpenGLPMobject)):
                continue
            if isinstance(mobject, OpenGLSurface):
                continue
            if isinstance(mobject, OpenGLMobject):
                # OpenGL families are diagnosed after shader-data inspection in
                # capture(), where plugin-defined indexed geometry may be retained.
                continue
            if type(mobject).__module__.startswith("manim.mobject.value_tracker"):
                # ValueTracker stores its scalar in a point but is intentionally
                # non-visual; its effect is already reflected in updated VMobjects.
                continue
            elif len(getattr(mobject, "points", ())) and not isinstance(
                mobject, (VMobject, OpenGLVMobject)
            ):
                self.diagnostics.add(
                    f"unsupported-mobject:{type(mobject).__name__}"
                )

    def scene_finished(self, scene: Scene) -> None:
        self.capture(scene)

    def clear_screen(self) -> None:
        self.object_tracks.clear()
        self.image_tracks.clear()
        self.point_tracks.clear()
        self.mesh_tracks.clear()
        self.object_active.clear()
        self.image_active.clear()
        self.point_active.clear()
        self.mesh_active.clear()
        self.object_generations.clear()
        self.image_generations.clear()
        self.point_generations.clear()
        self.mesh_generations.clear()
        self.custom_shader_tracks.clear()
        self.custom_shader_active.clear()
        self.custom_shader_generations.clear()

    def _value_tracker_controls(
        self, duration: float
    ) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
        for tracker_index, snapshots in enumerate(
            self.value_tracker_snapshots.values()
        ):
            samples = [
                (min(duration, at), value)
                for at, value in snapshots
                if at <= duration + 0.0001
            ]
            if len(samples) < 2:
                continue
            values = [value for _, value in samples]
            minimum = min(values)
            maximum = max(values)
            if maximum - minimum <= 0.000001:
                continue
            nondecreasing = all(
                left <= right + 0.000001
                for left, right in zip(values, values[1:])
            )
            nonincreasing = all(
                left + 0.000001 >= right
                for left, right in zip(values, values[1:])
            )
            if not nondecreasing and not nonincreasing:
                continue
            signal_id = f"manim-value-tracker-{tracker_index:03d}"
            keyframes = CompatibilityRenderer._simplify_keyframes(
                [
                    {
                        "at": at,
                        "value": value,
                        "easing": "linear",
                    }
                    for at, value in samples
                ],
                "trackerValue",
            )
            step = max((maximum - minimum) / 200.0, 0.000001)
            self.semantic_timeline_controls = 1
            return (
                [{"id": signal_id, "keyframes": keyframes}],
                [
                    {
                        "id": f"{signal_id}-control",
                        "label": "ValueTracker",
                        "signal": signal_id,
                        "min": finite(minimum),
                        "max": finite(maximum),
                        "step": finite(step),
                        "default": finite(values[0]),
                        "timeline": True,
                    }
                ],
            )
        return [], []

    def to_scene(self) -> dict[str, Any]:
        duration = max(0.25, finite(self.time))
        nodes: list[dict[str, Any]] = []
        tracks: list[dict[str, Any]] = []
        native_camera_2d = bool(
            self.camera_2d_snapshots
            and all(
                snapshot.aspect_matches
                for snapshot in self.camera_2d_snapshots
            )
            and not self.mesh_tracks
            and not self.custom_shader_tracks
            and all(
                snapshot.world_commands_2d
                and (
                    snapshot.fill_gradient is None
                    or snapshot.world_fill_gradient is not None
                )
                and (
                    snapshot.stroke_gradient is None
                    or snapshot.world_stroke_gradient is not None
                )
                for track in self.object_tracks.values()
                for snapshot in track.snapshots
            )
            and all(
                len(snapshot.world_corners) == 4
                for track in self.image_tracks.values()
                for snapshot in track.snapshots
            )
            and all(
                len(snapshot.world_points) == len(snapshot.points)
                for track in self.point_tracks.values()
                for snapshot in track.snapshots
            )
        )

        def disappear_at(track: Any, first_seen: float) -> float:
            if track.disappear_at is not None:
                recorded = max(first_seen + 0.000001, track.disappear_at)
            else:
                recorded = max(
                    first_seen + 1 / self.fps,
                    track.last_seen + 1 / self.fps,
                )
            return min(
                duration + 0.0001,
                recorded,
            )

        for object_track in self.object_tracks.values():
            snapshots = (
                [
                    replace(
                        snapshot,
                        commands=snapshot.world_commands_2d,
                        stroke_width=snapshot.world_stroke_width,
                        dash_array=snapshot.world_dash_array,
                        dash_offset=snapshot.world_dash_offset,
                        fill_gradient=snapshot.world_fill_gradient,
                        stroke_gradient=snapshot.world_stroke_gradient,
                        semantic_base_commands=[],
                        draw_start=0.0,
                        draw_end=1.0,
                    )
                    for snapshot in object_track.snapshots
                ]
                if native_camera_2d
                else object_track.snapshots
            )
            if not snapshots:
                continue
            affine_scales: list[float] | None = None
            affine_includes_stroke_width = False
            self._normalize_optional_gradient(
                snapshots, "fill_gradient", "fill"
            )
            self._normalize_optional_gradient(
                snapshots, "stroke_gradient", "stroke"
            )
            initial = snapshots[0]
            draw_range_trace = self._draw_range_trace(snapshots)
            draw_progress_trace = (
                None
                if draw_range_trace is not None
                else self._draw_progress_trace(snapshots)
            )
            path_3d_commands = (
                initial.commands_3d
                if initial.commands_3d
                and draw_range_trace is None
                and draw_progress_trace is None
                and all(
                    snapshot.commands_3d == initial.commands_3d
                    and snapshot.fill_gradient is None
                    and snapshot.stroke_gradient is None
                    for snapshot in snapshots
                )
                else []
            )
            trace_path = (
                self._sliding_cubic_trace(snapshots)
                if object_track.is_traced_path
                and not path_3d_commands
                and draw_range_trace is None
                and draw_progress_trace is None
                else None
            )
            node_style = {
                "fill": initial.fill,
                "fillGradient": initial.fill_gradient,
                "stroke": initial.stroke,
                "strokeGradient": initial.stroke_gradient,
                "strokeWidth": initial.stroke_width,
                "strokeCap": initial.stroke_cap,
                "strokeJoin": initial.stroke_join,
                "dashArray": initial.dash_array,
                "dashOffset": initial.dash_offset,
            }
            if draw_range_trace is not None:
                node_style["drawStart"] = draw_range_trace[1][0]
                node_style["drawProgress"] = draw_range_trace[2][0]
            elif draw_progress_trace is not None:
                node_style["drawProgress"] = draw_progress_trace[1][0]
            node = {
                "id": object_track.node_id,
                "zIndex": initial.z_index,
                "appearAt": min(initial.at, duration),
                "disappearAt": disappear_at(object_track, initial.at),
                "style": node_style,
            }
            if path_3d_commands:
                node.update({"type": "path3d", "commands": path_3d_commands})
            elif trace_path is not None:
                node.update({"type": "tracePath", **trace_path})
            else:
                node.update(
                    {
                        "type": "path",
                        "commands": (
                            draw_range_trace[0]
                            if draw_range_trace is not None
                            else draw_progress_trace[0]
                            if draw_progress_trace is not None
                            else initial.commands
                        ),
                    }
                )
            nodes.append(node)
            if path_3d_commands:
                self.semantic_path_3d_nodes += 1
            elif trace_path is not None:
                self.semantic_trace_path_nodes += 1
            elif draw_range_trace is not None:
                self._add_exact_draw_range_track(
                    tracks,
                    object_track.node_id,
                    snapshots,
                    draw_range_trace[1],
                    draw_range_trace[2],
                )
                self.semantic_draw_range_tracks += 1
            elif draw_progress_trace is not None:
                self._add_exact_numeric_track(
                    tracks,
                    object_track.node_id,
                    "drawProgress",
                    snapshots,
                    draw_progress_trace[1],
                )
                self.semantic_draw_progress_tracks += 1
            else:
                affine_result = self._add_affine_tracks(
                    tracks,
                    object_track.node_id,
                    snapshots,
                )
                if affine_result is None:
                    if self._add_path_data_track(
                        tracks, object_track.node_id, snapshots
                    ):
                        self.semantic_path_data_tracks += 1
                    else:
                        self._add_track(
                            tracks,
                            object_track.node_id,
                            "commands",
                            snapshots,
                            "commands",
                        )
                else:
                    (
                        affine_kind,
                        affine_scales,
                        affine_includes_stroke_width,
                    ) = affine_result
                    if affine_kind == "translation":
                        self.semantic_translation_tracks += 1
                    elif affine_kind == "scaleTranslation":
                        self.semantic_scale_translation_tracks += 1
                    elif affine_kind == "rotationTranslation":
                        self.semantic_rotation_translation_tracks += 1
                    elif affine_kind == "affine":
                        self.semantic_affine_transform_tracks += 1
                    else:
                        self.semantic_similarity_tracks += 1
            self._add_track(tracks, object_track.node_id, "fill", snapshots, "fill")
            self._add_optional_track(
                tracks,
                object_track.node_id,
                "fillGradient",
                snapshots,
                "fill_gradient",
            )
            self._add_track(
                tracks, object_track.node_id, "stroke", snapshots, "stroke"
            )
            self._add_optional_track(
                tracks,
                object_track.node_id,
                "strokeGradient",
                snapshots,
                "stroke_gradient",
            )
            if affine_scales is None:
                self._add_track(
                    tracks,
                    object_track.node_id,
                    "strokeWidth",
                    snapshots,
                    "stroke_width",
                )
            elif not affine_includes_stroke_width:
                self._add_exact_numeric_track(
                    tracks,
                    object_track.node_id,
                    "strokeWidth",
                    snapshots,
                    [
                        snapshot.stroke_width / max(scale, 1e-9)
                        for snapshot, scale in zip(
                            snapshots,
                            affine_scales,
                            strict=True,
                        )
                    ],
                )
        for image_track in self.image_tracks.values():
            snapshots = (
                [
                    replace(snapshot, corners=snapshot.world_corners)
                    for snapshot in image_track.snapshots
                ]
                if native_camera_2d
                else image_track.snapshots
            )
            if not snapshots:
                continue
            initial = snapshots[0]
            nodes.append(
                {
                    "id": image_track.node_id,
                    "type": "image",
                    "pixels": image_track.pixels,
                    "pixelWidth": image_track.pixel_width,
                    "pixelHeight": image_track.pixel_height,
                    "corners": initial.corners,
                    "resampling": image_track.resampling,
                    "zIndex": initial.z_index,
                    "appearAt": min(initial.at, duration),
                    "disappearAt": disappear_at(image_track, initial.at),
                    "style": {
                        "fill": None,
                        "stroke": None,
                        "opacity": initial.opacity,
                    },
                }
            )
            self._add_track(
                tracks, image_track.node_id, "points", snapshots, "corners"
            )
            self._add_track(
                tracks, image_track.node_id, "opacity", snapshots, "opacity"
            )
        for point_track in self.point_tracks.values():
            snapshots = (
                [
                    replace(
                        snapshot,
                        points=snapshot.world_points,
                    )
                    for snapshot in point_track.snapshots
                ]
                if native_camera_2d
                else point_track.snapshots
            )
            if not snapshots:
                continue
            point_counts = {len(snapshot.points) for snapshot in snapshots}
            if len(point_counts) != 1:
                self.diagnostics.add("dynamic-point-cloud-count")
                continue
            initial = snapshots[0]
            nodes.append(
                {
                    "id": point_track.node_id,
                    "type": "pointCloud",
                    "points": [
                        {
                            "x": point[0],
                            "y": point[1],
                            "color": initial.colors[index],
                        }
                        for index, point in enumerate(initial.points)
                    ],
                    "radius": initial.radius,
                    **(
                        {"screenSpaceRadius": True}
                        if native_camera_2d
                        else {}
                    ),
                    "zIndex": initial.z_index,
                    "appearAt": min(initial.at, duration),
                    "disappearAt": disappear_at(point_track, initial.at),
                    "style": {
                        "fill": None,
                        "stroke": None,
                        "opacity": 1.0,
                    },
                }
            )
            self._add_track(
                tracks, point_track.node_id, "points", snapshots, "points"
            )
            self._add_track(
                tracks, point_track.node_id, "colors", snapshots, "colors"
            )
            radius_snapshots = [
                Snapshot(
                    at=snapshot.at,
                    commands=[],
                    fill="",
                    fill_gradient=None,
                    stroke="",
                    stroke_gradient=None,
                    stroke_width=snapshot.radius,
                    z_index=snapshot.z_index,
                )
                for snapshot in snapshots
            ]
            self._add_track(
                tracks,
                point_track.node_id,
                "radius",
                radius_snapshots,
                "stroke_width",
            )
        for shader_track in self.custom_shader_tracks.values():
            snapshots = shader_track.snapshots
            if not snapshots:
                continue
            vertex_counts = {len(snapshot.vertex_data) for snapshot in snapshots}
            uniform_counts = {
                len(snapshot.uniform_values) for snapshot in snapshots
            }
            if len(vertex_counts) != 1:
                self.diagnostics.add("dynamic-opengl-shader-vertex-layout")
                continue
            if len(uniform_counts) != 1:
                self.diagnostics.add("dynamic-opengl-shader-uniform-layout")
                continue
            initial = snapshots[0]
            initial_uniforms = []
            value_offset = 0
            for uniform in shader_track.uniforms:
                uniform = dict(uniform)
                count = len(uniform["values"])
                if count:
                    uniform["values"] = initial.uniform_values[
                        value_offset : value_offset + count
                    ]
                    value_offset += count
                initial_uniforms.append(uniform)
            nodes.append(
                {
                    "id": shader_track.node_id,
                    "type": "customShaderMesh",
                    "vertexWgsl": shader_track.vertex_wgsl,
                    "fragmentWgsl": shader_track.fragment_wgsl,
                    "attributes": [
                        {
                            key: value
                            for key, value in attribute.items()
                            if not key.startswith("_")
                        }
                        for attribute in shader_track.attributes
                    ],
                    "vertexStride": shader_track.vertex_stride,
                    "vertexData": initial.vertex_data,
                    "indices": shader_track.indices,
                    "primitive": shader_track.primitive,
                    "uniforms": initial_uniforms,
                    "depthTest": shader_track.depth_test,
                    "zIndex": initial.z_index,
                    "appearAt": min(initial.at, duration),
                    "disappearAt": disappear_at(shader_track, initial.at),
                    "style": {
                        "fill": None,
                        "stroke": None,
                        "opacity": 1.0,
                    },
                }
            )
            self._add_track(
                tracks,
                shader_track.node_id,
                "shaderVertexData",
                snapshots,
                "vertex_data",
            )
            self._add_track(
                tracks,
                shader_track.node_id,
                "shaderUniformValues",
                snapshots,
                "uniform_values",
            )
        for mesh_track in self.mesh_tracks.values():
            snapshots = mesh_track.snapshots
            if not snapshots:
                continue
            vertex_counts = {len(snapshot.vertices) for snapshot in snapshots}
            if len(vertex_counts) != 1:
                self.diagnostics.add("dynamic-opengl-surface-vertex-count")
                continue
            initial = snapshots[0]
            textured = bool(mesh_track.texture_pixels)
            color_snapshots = snapshots
            if textured:
                opacities = [
                    max(0.0, min(1.0, snapshot.opacity))
                    for snapshot in snapshots
                ]
            else:
                opacities = []
                color_snapshots = []
                for snapshot in snapshots:
                    opacity = max(
                        0.0,
                        min(
                            1.0,
                            float(np.max(snapshot.rgbas[:, 3])),
                        ),
                    )
                    opacities.append(opacity)
                    normalized = snapshot.rgbas.copy()
                    if opacity > 1e-9:
                        normalized[:, 3] /= opacity
                    color_snapshots.append(
                        replace(
                            snapshot,
                            colors=[
                                rgba_hex(color)
                                for color in np.clip(normalized, 0.0, 1.0)
                            ],
                            rgbas=normalized,
                        )
                    )
            initial_colors = color_snapshots[0].colors
            optional_surface_colors: list[list[str] | None] = []
            if initial.surface_patches:
                for snapshot, opacity in zip(snapshots, opacities):
                    if snapshot.surface_rgbas is None:
                        optional_surface_colors = []
                        break
                    if opacity <= 1e-9:
                        # A fully transparent surface has no observable
                        # per-patch color. Reuse the first visible material so
                        # FadeIn can remain a scalar opacity track.
                        optional_surface_colors.append(None)
                        continue
                    normalized_surface = snapshot.surface_rgbas.copy()
                    normalized_surface[:, 3] /= opacity
                    optional_surface_colors.append(
                        [
                            rgba_hex(color)
                            for color in np.clip(
                                normalized_surface, 0.0, 1.0
                            )
                        ]
                    )
            visible_surface_colors = next(
                (
                    colors
                    for colors in optional_surface_colors
                    if colors is not None
                ),
                None,
            )
            surface_color_snapshots = (
                [
                    colors or visible_surface_colors
                    for colors in optional_surface_colors
                ]
                if visible_surface_colors is not None
                else []
            )
            is_compact_surface = bool(surface_color_snapshots) and all(
                len(snapshot.surface_vertices)
                == len(initial.surface_vertices)
                and snapshot.surface_patches == initial.surface_patches
                and len(snapshot.surface_stroke_radii)
                == len(initial.surface_stroke_radii)
                and len(surface_color_snapshots[index])
                == len(surface_color_snapshots[0])
                for index, snapshot in enumerate(snapshots)
            )
            if is_compact_surface:
                surface_colors = surface_color_snapshots[0]
                fill_color_count = sum(
                    len(patch) for patch in initial.surface_patches
                )
                nodes.append(
                    {
                        "id": mesh_track.node_id,
                        "type": "surface",
                        "vertices": initial.surface_vertices,
                        "patches": initial.surface_patches,
                        "colors": surface_colors[:fill_color_count],
                        "strokeColors": surface_colors[fill_color_count:],
                        "strokeRadii": initial.surface_stroke_radii,
                        "unlit": mesh_track.unlit,
                        "doubleSided": mesh_track.double_sided,
                        "zIndex": initial.z_index,
                        "appearAt": min(initial.at, duration),
                        "disappearAt": disappear_at(
                            mesh_track, initial.at
                        ),
                        "style": {
                            "fill": initial_colors[0],
                            "stroke": None,
                            "opacity": opacities[0],
                        },
                    }
                )
                self.semantic_surface_nodes += 1
                surface_geometry_is_dynamic = any(
                    snapshot.surface_vertices != initial.surface_vertices
                    for snapshot in snapshots[1:]
                )
                surface_material_is_dynamic = any(
                    surface_color_snapshots[index] != surface_colors
                    for index in range(1, len(surface_color_snapshots))
                )
                surface_stroke_is_dynamic = any(
                    snapshot.surface_stroke_radii
                    != initial.surface_stroke_radii
                    for snapshot in snapshots[1:]
                )
                if (
                    surface_geometry_is_dynamic
                    or surface_material_is_dynamic
                    or surface_stroke_is_dynamic
                ):
                    self.semantic_dynamic_surface_nodes += 1
                self._add_track(
                    tracks,
                    mesh_track.node_id,
                    "vertices",
                    snapshots,
                    "surface_vertices",
                )
                material_snapshots = [
                    replace(snapshot, colors=surface_color_snapshots[index])
                    for index, snapshot in enumerate(snapshots)
                ]
                self._add_track(
                    tracks,
                    mesh_track.node_id,
                    "surfaceColors",
                    material_snapshots,
                    "colors",
                )
                self._add_track(
                    tracks,
                    mesh_track.node_id,
                    "strokeRadii",
                    snapshots,
                    "surface_stroke_radii",
                )
                opacity_snapshots = [
                    ImageSnapshot(
                        at=snapshot.at,
                        corners=[],
                        opacity=opacities[index],
                        z_index=snapshot.z_index,
                    )
                    for index, snapshot in enumerate(snapshots)
                ]
                self._add_track(
                    tracks,
                    mesh_track.node_id,
                    "opacity",
                    opacity_snapshots,
                    "opacity",
                )
                continue
            nodes.append(
                {
                    "id": mesh_track.node_id,
                    "type": "mesh",
                    "vertices": initial.vertices,
                    "triangles": mesh_track.triangles,
                    "colors": initial_colors,
                    "normals": initial.normals,
                    "uvs": mesh_track.uvs,
                    "texturePixels": mesh_track.texture_pixels,
                    "textureWidth": mesh_track.texture_width,
                    "textureHeight": mesh_track.texture_height,
                    "darkTexturePixels": mesh_track.dark_texture_pixels,
                    "darkTextureWidth": mesh_track.dark_texture_width,
                    "darkTextureHeight": mesh_track.dark_texture_height,
                    "textureResampling": mesh_track.texture_resampling,
                    "gloss": mesh_track.gloss,
                    "shadow": mesh_track.shadow,
                    "lightPosition": initial.light_position[0],
                    "unlit": mesh_track.unlit,
                    "doubleSided": mesh_track.double_sided,
                    "zIndex": initial.z_index,
                    "appearAt": min(initial.at, duration),
                    "disappearAt": disappear_at(mesh_track, initial.at),
                    "style": {
                        "fill": initial_colors[0],
                        "stroke": None,
                        "opacity": opacities[0],
                    },
                }
            )
            self._add_track(
                tracks,
                mesh_track.node_id,
                "vertices",
                snapshots,
                "vertices",
            )
            self._add_track(
                tracks,
                mesh_track.node_id,
                "normals",
                snapshots,
                "normals",
            )
            self._add_track(
                tracks,
                mesh_track.node_id,
                "lightPosition",
                snapshots,
                "light_position",
            )
            if not textured:
                self._add_track(
                    tracks,
                    mesh_track.node_id,
                    "colors",
                    color_snapshots,
                    "colors",
                )
            opacity_snapshots = [
                ImageSnapshot(
                    at=snapshot.at,
                    corners=[],
                    opacity=opacities[index],
                    z_index=snapshot.z_index,
                )
                for index, snapshot in enumerate(snapshots)
            ]
            self._add_track(
                tracks,
                mesh_track.node_id,
                "opacity",
                opacity_snapshots,
                "opacity",
            )
        camera_2d = {"x": 0, "y": 0, "zoom": 1, "rotation": 0}
        if native_camera_2d:
            initial_camera = self.camera_2d_snapshots[0]
            camera_2d = {
                "x": initial_camera.x,
                "y": initial_camera.y,
                "zoom": initial_camera.zoom,
                "rotation": 0,
            }
            before = len(tracks)
            for property_name, attribute in (
                ("cameraX", "x"),
                ("cameraY", "y"),
                ("cameraZoom", "zoom"),
            ):
                self._add_track(
                    tracks,
                    "__camera__",
                    property_name,
                    self.camera_2d_snapshots,
                    attribute,
                )
            self.semantic_camera_2d_tracks = len(tracks) - before
        billboard_candidates: dict[str, dict[str, Any]] = {}
        for object_track in (
            self.object_tracks.values()
            if self.enable_semantic_billboards
            else ()
        ):
            snapshots = object_track.snapshots
            if (
                snapshots
                and snapshots[0].fixed_orientation_center
                and snapshots[0].fixed_orientation_base
                and all(
                    snapshot.fixed_orientation_center
                    and snapshot.fixed_orientation_base
                    for snapshot in snapshots
                )
            ):
                billboard_candidates[object_track.node_id] = {
                    "anchor": snapshots[0].fixed_orientation_center,
                    "base": snapshots[0].fixed_orientation_base,
                    "frames": [
                        {
                            "at": snapshot.at,
                            "anchor": snapshot.fixed_orientation_center,
                            "position": snapshot.fixed_orientation_base,
                        }
                        for snapshot in snapshots
                    ],
                }
        (
            self.semantic_affine_groups,
            self.semantic_affine_group_members,
            self.semantic_billboard_groups,
            self.semantic_billboard_members,
        ) = self._factor_shared_affine_tracks(
            nodes,
            tracks,
            billboard_candidates,
        )
        (
            self.semantic_path_references,
            self.semantic_translated_path_references,
        ) = self._deduplicate_static_paths(
            nodes,
            tracks,
        )
        self.semantic_track_references = self._deduplicate_track_keyframes(
            tracks
        )
        background = (
            rgba_hex(self.camera.background_color.to_rgba())
            if hasattr(self.camera, "background_color")
            else rgba_hex(config.background_color.to_rgba())
        )
        camera_3d = {
            "position": [0, 0, 8],
            "target": [0, 0, 0],
            "up": [0, 1, 0],
            "fovY": math.pi / 4,
            "near": 0.1,
            "far": 100,
            "ambient": 0.28,
            "lightDirection": [-0.4, 0.7, 1],
        }
        if isinstance(self.camera, OpenGLCamera):
            focal = float(self.camera.get_focal_distance())
            _frame_width, frame_height = self.camera.get_shape()
            camera_3d = {
                "position": [0, 0, finite(focal)],
                "target": [0, 0, 0],
                "up": [0, 1, 0],
                "fovY": finite(
                    2 * math.atan(float(frame_height) / (2 * focal))
                ),
                "near": 0.001,
                "far": 1_000,
                "ambient": 0.35,
                "lightDirection": [-0.4, 0.7, 1],
            }
        elif self.camera_3d_snapshots:
            initial_camera = self.camera_3d_snapshots[0]
            camera_3d = {
                "position": initial_camera.position,
                "target": initial_camera.target,
                "up": initial_camera.up,
                "fovY": initial_camera.fov_y,
                "near": 0.001,
                "far": 1_000,
                "ambient": 1.0,
                "lightDirection": [-0.4, 0.7, 1],
            }
            self.semantic_camera_3d_tracks = self._add_camera_3d_track(
                tracks,
                self.camera_3d_snapshots,
            )
        signals, controls = self._value_tracker_controls(duration)
        return {
            "version": 2,
            "title": self.scene_name,
            "width": OUTPUT_WIDTH,
            "height": OUTPUT_HEIGHT,
            "pixelWidth": int(config.pixel_width),
            "pixelHeight": int(config.pixel_height),
            "duration": duration,
            "fps": self.fps,
            "background": background,
            "camera": camera_2d,
            "camera3d": camera_3d,
            "nodes": nodes,
            "tracks": tracks,
            "signals": signals,
            "bindings": [],
            "controls": controls,
            "audio": self.audio,
            "captions": [
                {
                    "text": subtitle.content,
                    "start": finite(
                        min(duration, subtitle.start.total_seconds())
                    ),
                    "end": finite(min(duration, subtitle.end.total_seconds())),
                }
                for subtitle in self.file_writer.subcaptions
                if subtitle.end.total_seconds() > 0
                and min(duration, subtitle.end.total_seconds())
                > min(duration, subtitle.start.total_seconds())
            ],
        }

    @staticmethod
    def _factor_shared_affine_tracks(
        nodes: list[dict[str, Any]],
        tracks: list[dict[str, Any]],
        billboard_candidates: dict[str, dict[str, Any]],
    ) -> tuple[int, int, int, int]:
        affine_properties = {
            "x",
            "y",
            "rotation",
            "scaleX",
            "scaleY",
        }
        node_by_id = {node["id"]: node for node in nodes}
        billboard_targets: set[str] = set()
        billboards_by_candidate: dict[str, list[str]] = {}
        projected_targets = {
            track["target"]
            for track in tracks
            if track["property"]
            in {"x", "y", "transform2d", "affine2d", "commands", "pathData"}
        }
        for target, candidate in billboard_candidates.items():
            if (
                target in node_by_id
                and target in projected_targets
                and "parent" not in node_by_id[target]
            ):
                signature = json.dumps(candidate, separators=(",", ":"))
                billboards_by_candidate.setdefault(signature, []).append(
                    target
                )
        billboard_count = 0
        billboard_member_count = 0
        for signature, targets in billboards_by_candidate.items():
            candidate = json.loads(signature)
            group_id = f"semantic-billboard-{billboard_count:05d}"
            nodes.append(
                {
                    "id": group_id,
                    "type": "billboard",
                    "anchor": candidate["anchor"],
                    "base": candidate["base"],
                }
            )
            anchor_keyframes = [
                {
                    "at": frame["at"],
                    "value": frame["anchor"],
                    "easing": "linear",
                }
                for frame in candidate["frames"]
            ]
            if any(
                keyframe["value"] != anchor_keyframes[0]["value"]
                for keyframe in anchor_keyframes[1:]
            ):
                tracks.append(
                    {
                        "target": group_id,
                        "property": "billboardAnchor",
                        "keyframes": CompatibilityRenderer._simplify_keyframes(
                            anchor_keyframes, "billboardAnchor"
                        ),
                    }
                )
            for target in targets:
                node_by_id[target]["parent"] = group_id
            billboard_targets.update(targets)
            billboard_count += 1
            billboard_member_count += len(targets)
        if billboard_targets:
            residual_tracks: list[dict[str, Any]] = []
            for track in tracks:
                if (
                    track["target"] not in billboard_targets
                    or track["property"]
                    not in {"x", "y", "transform2d", "affine2d", "commands", "pathData"}
                ):
                    residual_tracks.append(track)
                    continue
                candidate = billboard_candidates[track["target"]]
                frames = candidate["frames"]
                base = candidate["base"]

                def camera_offset(at: float) -> list[float]:
                    exact = next(
                        (
                            frame["position"]
                            for frame in frames
                            if abs(frame["at"] - at) <= 0.000001
                        ),
                        None,
                    )
                    if exact is not None:
                        return [exact[0] - base[0], exact[1] - base[1]]
                    before = max(
                        (frame for frame in frames if frame["at"] <= at),
                        key=lambda frame: frame["at"],
                        default=frames[0],
                    )
                    after = min(
                        (frame for frame in frames if frame["at"] >= at),
                        key=lambda frame: frame["at"],
                        default=frames[-1],
                    )
                    span = after["at"] - before["at"]
                    amount = (
                        0.0
                        if abs(span) <= 0.000001
                        else (at - before["at"]) / span
                    )
                    position = [
                        before["position"][index]
                        + (
                            after["position"][index]
                            - before["position"][index]
                        )
                        * amount
                        for index in range(2)
                    ]
                    return [position[0] - base[0], position[1] - base[1]]

                keyframes = []
                for keyframe in track["keyframes"]:
                    offset = camera_offset(float(keyframe["at"]))
                    value = keyframe["value"]
                    if track["property"] == "x":
                        residual: Any = finite(float(value) - offset[0])
                    elif track["property"] == "y":
                        residual = finite(float(value) - offset[1])
                    elif track["property"] == "transform2d":
                        residual = list(value)
                        residual[0] = finite(float(residual[0]) - offset[0])
                        residual[1] = finite(float(residual[1]) - offset[1])
                    elif track["property"] == "affine2d":
                        residual = list(value)
                        residual[4] = finite(float(residual[4]) - offset[0])
                        residual[5] = finite(float(residual[5]) - offset[1])
                    elif track["property"] == "pathData":
                        residual = [
                            finite(
                                float(component)
                                - offset[index % 2]
                            )
                            for index, component in enumerate(value)
                        ]
                    else:
                        residual = [
                            {
                                **command,
                                **(
                                    {
                                        key: finite(
                                            float(command[key])
                                            - offset[
                                                0 if key.endswith("x") else 1
                                            ]
                                        )
                                        for key in command
                                        if key
                                        in {
                                            "x",
                                            "y",
                                            "cx",
                                            "cy",
                                            "c1x",
                                            "c1y",
                                            "c2x",
                                            "c2y",
                                        }
                                    }
                                ),
                            }
                            for command in value
                        ]
                    keyframes.append({**keyframe, "value": residual})
                numeric_values = [
                    float(keyframe["value"])
                    for keyframe in keyframes
                    if isinstance(keyframe["value"], (int, float))
                ]
                if numeric_values and max(
                    abs(value) for value in numeric_values
                ) <= 0.0000041:
                    continue
                residual_tracks.append(
                    {
                        **track,
                        "keyframes": CompatibilityRenderer._simplify_keyframes(
                            keyframes, track["property"]
                        ),
                    }
                )
            tracks[:] = residual_tracks

        affine_by_target: dict[str, list[dict[str, Any]]] = {}
        for track in tracks:
            if (
                track["property"] in affine_properties
                and track["target"] in node_by_id
                and "parent" not in node_by_id[track["target"]]
            ):
                affine_by_target.setdefault(track["target"], []).append(track)
        targets_by_signature: dict[str, list[str]] = {}
        for target, affine_tracks in affine_by_target.items():
            signature = json.dumps(
                sorted(
                    (
                        track["property"],
                        [
                            {
                                "at": keyframe["at"],
                                "value": round(
                                    float(keyframe["value"]), 5
                                ),
                                "easing": keyframe["easing"],
                            }
                            for keyframe in track["keyframes"]
                        ],
                    )
                    for track in affine_tracks
                ),
                separators=(",", ":"),
            )
            targets_by_signature.setdefault(signature, []).append(target)
        grouped_targets: set[str] = set()
        replacement_tracks: list[dict[str, Any]] = []
        group_count = 0
        member_count = 0
        for targets in targets_by_signature.values():
            if len(targets) < 2:
                continue
            source_tracks = affine_by_target[targets[0]]
            group_id = f"semantic-affine-group-{group_count:05d}"
            nodes.append({"id": group_id, "type": "group"})
            replacement_tracks.extend(
                {**track, "target": group_id} for track in source_tracks
            )
            group_count += 1
            member_count += len(targets)
            for target in targets:
                node_by_id[target]["parent"] = group_id
                grouped_targets.add(target)
        if grouped_targets:
            tracks[:] = [
                track
                for track in tracks
                if not (
                    track["target"] in grouped_targets
                    and track["property"] in affine_properties
                )
            ]
            tracks.extend(replacement_tracks)
        return (
            group_count,
            member_count,
            billboard_count,
            billboard_member_count,
        )

    @staticmethod
    def _deduplicate_track_keyframes(tracks: list[dict[str, Any]]) -> int:
        owners: dict[tuple[str, str], str] = {}
        references = 0
        for track in tracks:
            keyframes = track.get("keyframes")
            if not keyframes:
                continue
            signature = (
                track["property"],
                json.dumps(keyframes, separators=(",", ":")),
            )
            owner = owners.get(signature)
            if owner is None:
                owners[signature] = track["target"]
                continue
            track["keyframesFrom"] = owner
            del track["keyframes"]
            references += 1
        return references

    @staticmethod
    def _deduplicate_static_paths(
        nodes: list[dict[str, Any]], tracks: list[dict[str, Any]]
    ) -> tuple[int, int]:
        command_targets = {
            track["target"]
            for track in tracks
            if track["property"] in {"commands", "pathData"}
        }
        transform_targets = {
            track["target"]
            for track in tracks
            if track["property"] in {"x", "y"}
        }
        exact_owners: dict[str, str] = {}
        translated_owners: dict[
            tuple[Any, tuple[float, ...]],
            list[tuple[str, list[dict[str, Any]]]],
        ] = {}
        references = 0
        translated_references = 0
        for node in nodes:
            if (
                node.get("type") != "path"
                or node["id"] in command_targets
            ):
                continue
            exact_signature = json.dumps(
                node["commands"], separators=(",", ":")
            )
            exact_owner = exact_owners.get(exact_signature)
            if exact_owner is not None:
                node["type"] = "pathRef"
                node["source"] = exact_owner
                del node["commands"]
                references += 1
                continue
            if node["id"] in transform_targets:
                exact_owners[exact_signature] = node["id"]
                continue
            signature = CompatibilityRenderer._translation_signature(
                node["commands"]
            )
            if signature is None:
                exact_owners[exact_signature] = node["id"]
                continue
            candidates = translated_owners.setdefault(signature, [])
            match = next(
                (
                    (owner_id, offset)
                    for owner_id, owner_commands in candidates
                    if (
                        offset := CompatibilityRenderer._translation_offset(
                            owner_commands, node["commands"]
                        )
                    )
                    is not None
                ),
                None,
            )
            if match is None:
                exact_owners[exact_signature] = node["id"]
                candidates.append((node["id"], node["commands"]))
                continue
            owner, offset = match
            node["type"] = "pathRef"
            node["source"] = owner
            del node["commands"]
            if abs(offset[0]) > 0.0000001 or abs(offset[1]) > 0.0000001:
                node["transform"] = {
                    "x": offset[0],
                    "y": offset[1],
                }
                translated_references += 1
            references += 1
        return references, translated_references

    @staticmethod
    def _translation_signature(
        commands: list[dict[str, Any]],
    ) -> tuple[Any, tuple[float, ...]] | None:
        vector = CompatibilityRenderer._vectorize(commands)
        if (
            vector is None
            or not isinstance(vector[0], tuple)
            or vector[0][0] != "commands"
            or vector[1].size < 2
            or vector[1].size % 2 != 0
        ):
            return None
        points = vector[1].reshape(-1, 2)
        normalized = np.round(points - points[0], decimals=5)
        return vector[0], tuple(float(value) for value in normalized.flat)

    def receipt(self) -> dict[str, Any]:
        return {
            "schemaVersion": 1,
            "source": "ManimCE",
            "scene": self.scene_name,
            "randomSeed": self.random_seed,
            "duration": finite(self.time),
            "fps": self.fps,
            "sampledFrames": self.frames,
            "vectorObjects": len(self.object_tracks),
            "imageObjects": len(self.image_tracks),
            "pointCloudObjects": len(self.point_tracks),
            "meshObjects": len(self.mesh_tracks),
            "semanticTranslationTracks": self.semantic_translation_tracks,
            "semanticScaleTranslationTracks": (
                self.semantic_scale_translation_tracks
            ),
            "semanticRotationTranslationTracks": (
                self.semantic_rotation_translation_tracks
            ),
            "semanticSimilarityTracks": self.semantic_similarity_tracks,
            "semanticAffineTransformTracks": self.semantic_affine_transform_tracks,
            "semanticTimelineControls": self.semantic_timeline_controls,
            "semanticDrawProgressTracks": self.semantic_draw_progress_tracks,
            "semanticDrawRangeTracks": self.semantic_draw_range_tracks,
            "semanticSurfaceNodes": self.semantic_surface_nodes,
            "semanticDynamicSurfaceNodes": (
                self.semantic_dynamic_surface_nodes
            ),
            "semanticPathReferences": self.semantic_path_references,
            "semanticTranslatedPathReferences": (
                self.semantic_translated_path_references
            ),
            "semanticAffineGroups": self.semantic_affine_groups,
            "semanticAffineGroupMembers": self.semantic_affine_group_members,
            "semanticBillboardGroups": self.semantic_billboard_groups,
            "semanticBillboardMembers": self.semantic_billboard_members,
            "semanticTrackReferences": self.semantic_track_references,
            "semanticPath3dNodes": self.semantic_path_3d_nodes,
            "semanticTracePathNodes": self.semantic_trace_path_nodes,
            "semanticPathDataTracks": self.semantic_path_data_tracks,
            "semanticCamera2dTracks": self.semantic_camera_2d_tracks,
            "semanticCamera3dTracks": self.semantic_camera_3d_tracks,
            "audio": [
                {
                    "id": clip["id"],
                    "mimeType": clip["mimeType"],
                    "startTime": clip["startTime"],
                    "gainDb": clip["gainDb"],
                    "encodedBytes": len(clip["data"]),
                }
                for clip in self.audio
            ],
            "subcaptions": [
                {
                    "content": subtitle.content,
                    "start": subtitle.start.total_seconds(),
                    "end": subtitle.end.total_seconds(),
                }
                for subtitle in self.file_writer.subcaptions
            ],
            "diagnostics": sorted(self.diagnostics),
        }

    @staticmethod
    def _add_track(
        output: list[dict[str, Any]],
        target: str,
        property_name: str,
        snapshots: list[Snapshot],
        attribute: str,
    ) -> None:
        keyframes = CompatibilityRenderer._attribute_keyframes(
            snapshots, attribute, property_name
        )
        if len(keyframes) > 1:
            output.append(
                {
                    "target": target,
                    "property": property_name,
                    "keyframes": keyframes,
                }
            )

    @staticmethod
    def _add_exact_numeric_track(
        output: list[dict[str, Any]],
        target: str,
        property_name: str,
        snapshots: list[Snapshot],
        values: list[float],
    ) -> None:
        if len(values) < 2 or all(value == values[0] for value in values[1:]):
            return
        keyframes = [
            {
                "at": snapshot.at,
                "value": value,
                "easing": "linear",
            }
            for snapshot, value in zip(snapshots, values, strict=True)
        ]
        output.append(
            {
                "target": target,
                "property": property_name,
                "keyframes": CompatibilityRenderer._simplify_keyframes(
                    keyframes,
                    property_name,
                ),
            }
        )

    @staticmethod
    def _add_exact_draw_range_track(
        output: list[dict[str, Any]],
        target: str,
        snapshots: list[Snapshot],
        starts: list[float],
        ends: list[float],
    ) -> None:
        values = [
            [start, end]
            for start, end in zip(starts, ends, strict=True)
        ]
        if len(values) < 2 or all(value == values[0] for value in values[1:]):
            return
        keyframes = [
            {
                "at": snapshot.at,
                "value": value,
                "easing": "linear",
            }
            for snapshot, value in zip(snapshots, values, strict=True)
        ]
        output.append(
            {
                "target": target,
                "property": "drawRange",
                "keyframes": CompatibilityRenderer._simplify_keyframes(
                    keyframes,
                    "drawRange",
                ),
            }
        )

    @staticmethod
    def _add_camera_3d_track(
        output: list[dict[str, Any]],
        snapshots: list[Camera3dSnapshot],
    ) -> int:
        if len(snapshots) < 2:
            return 0
        if CompatibilityRenderer._is_camera_3d_orbit(snapshots):
            CompatibilityRenderer._add_track(
                output,
                "__camera__",
                "camera3dOrbit",
                snapshots,
                "position",
            )
            return 1
        first = snapshots[0]
        fields = [
            (1, "position"),
            (2, "target"),
            (4, "up"),
            (8, "fov_y"),
        ]
        mask = sum(
            bit
            for bit, attribute in fields
            if any(
                getattr(snapshot, attribute) != getattr(first, attribute)
                for snapshot in snapshots[1:]
            )
        )
        if mask == 0:
            return 0
        values: list[list[float]] = []
        for snapshot in snapshots:
            value = [float(mask)]
            for bit, attribute in fields:
                if mask & bit:
                    component = getattr(snapshot, attribute)
                    value.extend(
                        component if isinstance(component, list) else [component]
                    )
            values.append(value)
        keyframes = [
            {
                "at": snapshot.at,
                "value": value,
                "easing": "linear",
            }
            for snapshot, value in zip(snapshots, values, strict=True)
        ]
        compact = [
            {
                "target": "__camera__",
                "property": "camera3d",
                "keyframes": CompatibilityRenderer._simplify_keyframes(
                    keyframes,
                    "camera3d",
                ),
            }
        ]
        legacy: list[dict[str, Any]] = []
        for property_name, attribute in (
            ("camera3dPosition", "position"),
            ("camera3dTarget", "target"),
            ("camera3dUp", "up"),
            ("camera3dFovY", "fov_y"),
        ):
            CompatibilityRenderer._add_track(
                legacy,
                "__camera__",
                property_name,
                snapshots,
                attribute,
            )
        chosen = min(
            (compact, legacy),
            key=lambda tracks: len(json.dumps(tracks, separators=(",", ":"))),
        )
        output.extend(chosen)
        return len(chosen)

    @staticmethod
    def _is_camera_3d_orbit(
        snapshots: list[Camera3dSnapshot],
    ) -> bool:
        first = snapshots[0]
        if any(
            snapshot.target != first.target or snapshot.fov_y != first.fov_y
            for snapshot in snapshots[1:]
        ):
            return False
        for snapshot in snapshots:
            direction = np.asarray(snapshot.position) - np.asarray(snapshot.target)
            length = float(np.linalg.norm(direction))
            if length <= 0.000001:
                return False
            forward = direction / length
            expected = np.asarray([0.0, 0.0, 1.0]) - forward[2] * forward
            up_length = float(np.linalg.norm(expected))
            if up_length <= 0.000001:
                return False
            expected /= up_length
            if float(np.max(np.abs(expected - np.asarray(snapshot.up)))) > 0.000002:
                return False
        return True

    @staticmethod
    def _path_data(
        commands: list[dict[str, Any]],
    ) -> tuple[tuple[str, ...], list[float]] | None:
        signature: list[str] = []
        values: list[float] = []
        fields = {
            "moveTo": ("x", "y"),
            "lineTo": ("x", "y"),
            "quadTo": ("cx", "cy", "x", "y"),
            "cubicTo": ("c1x", "c1y", "c2x", "c2y", "x", "y"),
            "close": (),
        }
        for command in commands:
            operation = command.get("op")
            keys = fields.get(operation)
            if keys is None or any(key not in command for key in keys):
                return None
            signature.append(operation)
            values.extend(finite(command[key]) for key in keys)
        return tuple(signature), values

    @staticmethod
    def _add_path_data_track(
        output: list[dict[str, Any]],
        target: str,
        snapshots: list[Snapshot],
    ) -> bool:
        parsed = [
            CompatibilityRenderer._path_data(snapshot.commands)
            for snapshot in snapshots
        ]
        if (
            not parsed
            or any(value is None for value in parsed)
            or parsed[0] is None
            or any(
                value is not None and value[0] != parsed[0][0]
                for value in parsed[1:]
            )
        ):
            return False
        keyframes = CompatibilityRenderer._attribute_keyframes(
            snapshots, "commands", "commands"
        )
        if len(keyframes) <= 1:
            return False
        compact_keyframes: list[dict[str, Any]] = []
        for keyframe in keyframes:
            value = CompatibilityRenderer._path_data(keyframe["value"])
            if value is None or value[0] != parsed[0][0]:
                return False
            compact_keyframes.append({**keyframe, "value": value[1]})
        output.append(
            {
                "target": target,
                "property": "pathData",
                "keyframes": compact_keyframes,
            }
        )
        return True

    @staticmethod
    def _attribute_keyframes(
        snapshots: list[Snapshot], attribute: str, property_name: str
    ) -> list[dict[str, Any]]:
        keyframes: list[dict[str, Any]] = []
        previous: Any = object()
        last_snapshot: Snapshot | None = None
        for snapshot in snapshots:
            value = getattr(snapshot, attribute)
            if not keyframes:
                keyframes.append(
                    {"at": snapshot.at, "value": value, "easing": "linear"}
                )
                previous = value
                last_snapshot = snapshot
                continue
            if value == previous:
                last_snapshot = snapshot
                continue
            if (
                last_snapshot is not None
                and last_snapshot.at > keyframes[-1]["at"]
            ):
                keyframes.append(
                    {
                        "at": last_snapshot.at,
                        "value": previous,
                        "easing": "linear",
                    }
                )
            keyframes.append(
                {"at": snapshot.at, "value": value, "easing": "linear"}
            )
            previous = value
            last_snapshot = snapshot
        return CompatibilityRenderer._simplify_keyframes(
            keyframes, property_name
        )

    def _add_affine_tracks(
        self,
        output: list[dict[str, Any]],
        target: str,
        snapshots: list[Snapshot],
    ) -> tuple[str, list[float], bool] | None:
        if len(snapshots) < 2:
            return None
        base_commands = snapshots[0].commands
        transforms = [
            CompatibilityRenderer._similarity_offset(
                base_commands, snapshot.commands
            )
            for snapshot in snapshots
        ]
        if any(transform is None for transform in transforms):
            if not self.enable_full_affine_tracks:
                return None
            affine_values = [
                CompatibilityRenderer._affine_offset(
                    base_commands, snapshot.commands
                )
                for snapshot in snapshots
            ]
            if any(value is None for value in affine_values):
                return None
            matrices = [value for value in affine_values if value is not None]
            if all(value == matrices[0] for value in matrices[1:]):
                return None
            if not CompatibilityRenderer._retain_world_gradients(snapshots):
                return None
            output.append(
                {
                    "target": target,
                    "property": "affine2d",
                    "keyframes": CompatibilityRenderer._simplify_keyframes(
                        [
                            {
                                "at": snapshot.at,
                                "value": value,
                                "easing": "linear",
                            }
                            for snapshot, value in zip(
                                snapshots, matrices, strict=True
                            )
                        ],
                        "affine2d",
                    ),
                }
            )
            return "affine", [1.0] * len(snapshots), False
        values = [
            transform
            for transform in transforms
            if transform is not None
        ]
        if all(value == values[0] for value in values[1:]):
            return None
        if not CompatibilityRenderer._retain_world_gradients(snapshots):
            return None
        scales = [value[2] for value in values]
        rotations = [value[3] for value in values]
        for index in range(1, len(rotations)):
            while rotations[index] - rotations[index - 1] > math.pi:
                rotations[index] -= math.tau
            while rotations[index] - rotations[index - 1] < -math.pi:
                rotations[index] += math.tau
        has_scale = any(abs(scale - 1.0) > 0.0000001 for scale in scales)
        has_rotation = any(abs(rotation) > 0.0000001 for rotation in rotations)
        if has_rotation:
            keyframes = [
                {
                    "at": snapshot.at,
                    "value": [
                        value[0],
                        value[1],
                        rotation,
                        scale,
                        scale,
                        snapshot.stroke_width / max(scale, 1e-9),
                    ],
                    "easing": "linear",
                }
                for snapshot, value, rotation, scale in zip(
                    snapshots, values, rotations, scales, strict=True
                )
            ]
            output.append(
                {
                    "target": target,
                    "property": "transform2d",
                    "keyframes": CompatibilityRenderer._simplify_keyframes(
                        keyframes, "transform2d"
                    ),
                }
            )
        else:
            for index, property_name in enumerate(("x", "y")):
                CompatibilityRenderer._add_exact_numeric_track(
                    output,
                    target,
                    property_name,
                    snapshots,
                    [value[index] for value in values],
                )
            for property_name in ("scaleX", "scaleY"):
                CompatibilityRenderer._add_exact_numeric_track(
                    output,
                    target,
                    property_name,
                    snapshots,
                    scales,
                )
        return (
            (
                "similarity"
                if has_scale and has_rotation
                else "scaleTranslation"
                if has_scale
                else "rotationTranslation"
                if has_rotation
                else "translation"
            ),
            scales,
            has_rotation,
        )

    @staticmethod
    def _retain_world_gradients(snapshots: list[Snapshot]) -> bool:
        gradients_by_attribute: list[
            list[dict[str, Any]]
        ] = []
        for attribute in ("fill_gradient", "stroke_gradient"):
            gradients = [getattr(snapshot, attribute) for snapshot in snapshots]
            if gradients[0] is None:
                if any(gradient is not None for gradient in gradients):
                    return False
                continue
            if any(gradient is None for gradient in gradients):
                return False
            gradients_by_attribute.append(
                [gradient for gradient in gradients if gradient is not None]
            )
        for gradients in gradients_by_attribute:
            for gradient in gradients:
                gradient["space"] = "world"
        return True

    @staticmethod
    def _similarity_offset(
        base_commands: list[dict[str, Any]],
        current_commands: list[dict[str, Any]],
    ) -> list[float] | None:
        translation = CompatibilityRenderer._translation_offset(
            base_commands,
            current_commands,
        )
        if translation is not None:
            return [translation[0], translation[1], 1.0, 0.0]
        base_vector = CompatibilityRenderer._vectorize(base_commands)
        current_vector = CompatibilityRenderer._vectorize(current_commands)
        if (
            base_vector is None
            or current_vector is None
            or base_vector[0] != current_vector[0]
            or not isinstance(base_vector[0], tuple)
            or base_vector[0][0] != "commands"
            or base_vector[1].size < 4
            or base_vector[1].size % 2 != 0
        ):
            return None
        base_points = base_vector[1].reshape(-1, 2)
        current_points = current_vector[1].reshape(-1, 2)
        base_center = np.mean(base_points, axis=0)
        current_center = np.mean(current_points, axis=0)
        centered = base_points - base_center
        denominator = float(np.sum(centered * centered))
        if denominator <= 1e-12:
            return None
        target = current_points - current_center
        scale_without_rotation = float(
            np.sum(centered * target) / denominator
        )
        if scale_without_rotation > 0.0:
            offset_without_rotation = (
                current_center - scale_without_rotation * base_center
            )
            expected_without_rotation = (
                base_points * scale_without_rotation
                + offset_without_rotation
            )
            if (
                float(
                    np.max(
                        np.abs(
                            expected_without_rotation - current_points
                        )
                    )
                )
                <= 0.0000021
            ):
                return [
                    finite(offset_without_rotation[0]),
                    finite(offset_without_rotation[1]),
                    finite(scale_without_rotation),
                    0.0,
                ]
        real = float(np.sum(centered * target) / denominator)
        imaginary = float(
            np.sum(centered[:, 0] * target[:, 1])
            - np.sum(centered[:, 1] * target[:, 0])
        ) / denominator
        scale = math.hypot(real, imaginary)
        if scale <= 0.0:
            return None
        rotation = math.atan2(imaginary, real)
        matrix = np.asarray(
            [[real, -imaginary], [imaginary, real]], dtype=float
        )
        offset = current_center - matrix @ base_center
        expected = base_points @ matrix.T + offset
        # Fitting a transform between two independently six-decimal-rounded
        # paths can accumulate just over one unit of rounding residue.
        if float(np.max(np.abs(expected - current_points))) > 0.0000021:
            return None
        return [
            finite(offset[0]),
            finite(offset[1]),
            finite(scale),
            finite(rotation),
        ]

    @staticmethod
    def _affine_offset(
        base_commands: list[dict[str, Any]],
        current_commands: list[dict[str, Any]],
    ) -> list[float] | None:
        base_vector = CompatibilityRenderer._vectorize(base_commands)
        current_vector = CompatibilityRenderer._vectorize(current_commands)
        if (
            base_vector is None
            or current_vector is None
            or base_vector[0] != current_vector[0]
            or not isinstance(base_vector[0], tuple)
            or base_vector[0][0] != "commands"
            or base_vector[1].size < 6
            or base_vector[1].size % 2 != 0
        ):
            return None
        base_points = base_vector[1].reshape(-1, 2)
        current_points = current_vector[1].reshape(-1, 2)
        design = np.column_stack(
            (base_points, np.ones(len(base_points), dtype=float))
        )
        if int(np.linalg.matrix_rank(design)) < 3:
            return None
        coefficients, *_ = np.linalg.lstsq(
            design, current_points, rcond=None
        )
        expected = design @ coefficients
        if float(np.max(np.abs(expected - current_points))) > 0.0000021:
            return None
        return [
            finite(coefficients[0, 0]),
            finite(coefficients[0, 1]),
            finite(coefficients[1, 0]),
            finite(coefficients[1, 1]),
            finite(coefficients[2, 0]),
            finite(coefficients[2, 1]),
        ]

    @staticmethod
    def _translation_offset(
        base_commands: list[dict[str, Any]],
        current_commands: list[dict[str, Any]],
    ) -> list[float] | None:
        base_vector = CompatibilityRenderer._vectorize(base_commands)
        if (
            base_vector is None
            or not isinstance(base_vector[0], tuple)
            or base_vector[0][0] != "commands"
            or base_vector[1].size < 2
            or base_vector[1].size % 2 != 0
        ):
            return None
        signature = base_vector[0]
        base_points = base_vector[1].reshape(-1, 2)
        vector = CompatibilityRenderer._vectorize(current_commands)
        if vector is None or vector[0] != signature:
            return None
        points = vector[1].reshape(-1, 2)
        delta = points - base_points
        offset = np.mean(delta, axis=0)
        # Projected commands are rounded to six decimals. This permits only the
        # one-unit rounding residue, never an actual deformation.
        if float(np.max(np.abs(delta - offset))) > 0.0000011:
            return None
        return [finite(offset[0]), finite(offset[1])]

    @staticmethod
    def _sliding_cubic_trace(
        snapshots: list[Snapshot],
    ) -> dict[str, Any] | None:
        if len(snapshots) < 3:
            return None

        def segments_for(
            commands: list[dict[str, Any]],
        ) -> tuple[list[dict[str, Any]], bool] | None:
            if not commands or commands[0].get("op") != "moveTo":
                return None
            closed = commands[-1].get("op") == "close"
            body = commands[1:-1] if closed else commands[1:]
            if not body or any(command.get("op") != "cubicTo" for command in body):
                return None
            current = [commands[0]["x"], commands[0]["y"]]
            segments: list[dict[str, Any]] = []
            for command in body:
                segment = {
                    "start": current,
                    "control1": [command["c1x"], command["c1y"]],
                    "control2": [command["c2x"], command["c2y"]],
                    "end": [command["x"], command["y"]],
                }
                segments.append(segment)
                current = segment["end"]
            return segments, closed

        global_segments: list[dict[str, Any]] = []
        frames: list[dict[str, Any]] = []
        parsed_frames: list[tuple[list[dict[str, Any]], bool]] = []
        for snapshot in snapshots:
            parsed = segments_for(snapshot.commands)
            if parsed is None:
                return None
            parsed_frames.append(parsed)
            segments, closed = parsed
            start = next(
                (
                    index
                    for index in range(len(global_segments) - len(segments) + 1)
                    if global_segments[index : index + len(segments)] == segments
                ),
                None,
            )
            if start is None:
                overlap = 0
                for size in range(
                    min(len(global_segments), len(segments)),
                    0,
                    -1,
                ):
                    if global_segments[-size:] == segments[:size]:
                        overlap = size
                        break
                start = len(global_segments) - overlap
                global_segments.extend(segments[overlap:])
            frames.append(
                {
                    "at": snapshot.at,
                    "start": start,
                    "count": len(segments),
                    "closed": closed,
                }
            )

        def commands_for(frame: dict[str, Any]) -> list[dict[str, Any]]:
            selected = global_segments[
                frame["start"] : frame["start"] + frame["count"]
            ]
            commands: list[dict[str, Any]] = [
                {
                    "op": "moveTo",
                    "x": selected[0]["start"][0],
                    "y": selected[0]["start"][1],
                }
            ]
            commands.extend(
                {
                    "op": "cubicTo",
                    "c1x": segment["control1"][0],
                    "c1y": segment["control1"][1],
                    "c2x": segment["control2"][0],
                    "c2y": segment["control2"][1],
                    "x": segment["end"][0],
                    "y": segment["end"][1],
                }
                for segment in selected
            )
            if frame["closed"]:
                commands.append({"op": "close"})
            return commands

        if any(
            commands_for(frame) != snapshot.commands
            for frame, snapshot in zip(frames, snapshots, strict=True)
        ):
            return None
        # This representation is for sliding/reused segment windows. Ordinary
        # whole-shape motion is more compact as native affine tracks.
        if len(global_segments) * 3 >= sum(frame["count"] for frame in frames):
            return None
        return {"segments": global_segments, "frames": frames}

    @staticmethod
    def _draw_progress_trace(
        snapshots: list[Snapshot],
    ) -> tuple[list[dict[str, Any]], list[float]] | None:
        if len(snapshots) < 3:
            return None
        final_commands = snapshots[-1].commands
        final_curves = CompatibilityRenderer._single_cubic_subpath(
            final_commands
        )
        if final_curves is None or len(final_curves) < 2:
            return None
        progress_values: list[float] = []
        previous = -1.0
        for snapshot in snapshots:
            progress = CompatibilityRenderer._partial_cubic_progress(
                final_commands, final_curves, snapshot.commands
            )
            if progress is None or progress + 0.000001 < previous:
                return None
            progress_values.append(round(progress, 9))
            previous = progress
        if progress_values[0] > 0.00001 or progress_values[-1] < 0.99999:
            return None
        progress_values[0] = 0.0
        progress_values[-1] = 1.0
        return final_commands, progress_values

    @staticmethod
    def _draw_range_trace(
        snapshots: list[Snapshot],
    ) -> tuple[
        list[dict[str, Any]], list[float], list[float]
    ] | None:
        if len(snapshots) < 2:
            return None
        base_commands = next(
            (
                snapshot.semantic_base_commands
                for snapshot in snapshots
                if snapshot.semantic_base_commands
            ),
            [],
        )
        if (
            not base_commands
            or CompatibilityRenderer._cubic_path_curve_count(base_commands)
            is None
        ):
            return None
        starts: list[float] = []
        ends: list[float] = []
        for snapshot in snapshots:
            if snapshot.semantic_base_commands:
                if (
                    snapshot.semantic_base_commands != base_commands
                    or not 0.0
                    <= snapshot.draw_start
                    <= snapshot.draw_end
                    <= 1.0
                ):
                    return None
                starts.append(snapshot.draw_start)
                ends.append(snapshot.draw_end)
            elif snapshot.commands == base_commands:
                starts.append(0.0)
                ends.append(1.0)
            else:
                return None
        if all(
            start == starts[0] and end == ends[0]
            for start, end in zip(starts[1:], ends[1:])
        ):
            return None
        return base_commands, starts, ends

    @staticmethod
    def _cubic_path_curve_count(
        commands: list[dict[str, Any]],
    ) -> int | None:
        active_subpath = False
        curve_count = 0
        for command in commands:
            operation = command.get("op")
            if operation == "moveTo":
                active_subpath = True
            elif operation == "cubicTo":
                if not active_subpath:
                    return None
                curve_count += 1
            elif operation == "close":
                if not active_subpath:
                    return None
                active_subpath = False
            else:
                return None
        return curve_count if curve_count else None

    @staticmethod
    def _single_cubic_subpath(
        commands: list[dict[str, Any]],
    ) -> list[dict[str, Any]] | None:
        if not commands or commands[0].get("op") != "moveTo":
            return None
        body = commands[1:]
        if body and body[-1].get("op") == "close":
            body = body[:-1]
        if not body or any(command.get("op") != "cubicTo" for command in body):
            return None
        return body

    @staticmethod
    def _partial_cubic_progress(
        final_commands: list[dict[str, Any]],
        final_curves: list[dict[str, Any]],
        current_commands: list[dict[str, Any]],
    ) -> float | None:
        current_curves = CompatibilityRenderer._single_cubic_subpath(
            current_commands
        )
        if (
            current_curves is None
            or len(current_curves) > len(final_curves)
            or not CompatibilityRenderer._commands_close(
                [final_commands[0]], [current_commands[0]], 0.0000011
            )
        ):
            return None
        prefix_count = len(current_curves) - 1
        if not CompatibilityRenderer._commands_close(
            final_curves[:prefix_count],
            current_curves[:prefix_count],
            0.0000011,
        ):
            return None
        final_curve = final_curves[prefix_count]
        current_curve = current_curves[-1]
        if CompatibilityRenderer._commands_close(
            [final_curve], [current_curve], 0.0000011
        ):
            partial = 1.0
        else:
            start_command = (
                final_commands[0]
                if prefix_count == 0
                else final_curves[prefix_count - 1]
            )
            start = np.asarray(
                [start_command["x"], start_command["y"]], dtype=float
            )
            observed = np.asarray(
                [
                    current_curve["c1x"],
                    current_curve["c1y"],
                    current_curve["c2x"],
                    current_curve["c2y"],
                    current_curve["x"],
                    current_curve["y"],
                ],
                dtype=float,
            )
            low, high = 0.0, 1.0
            for _ in range(50):
                left = (2 * low + high) / 3
                right = (low + 2 * high) / 3
                left_error = float(
                    np.sum(
                        (
                            CompatibilityRenderer._left_cubic(
                                start, final_curve, left
                            )
                            - observed
                        )
                        ** 2
                    )
                )
                right_error = float(
                    np.sum(
                        (
                            CompatibilityRenderer._left_cubic(
                                start, final_curve, right
                            )
                            - observed
                        )
                        ** 2
                    )
                )
                if left_error < right_error:
                    high = right
                else:
                    low = left
            partial = (low + high) / 2
            reconstructed = CompatibilityRenderer._left_cubic(
                start, final_curve, partial
            )
            if float(np.max(np.abs(reconstructed - observed))) > 0.0000012:
                return None
        return (prefix_count + partial) / len(final_curves)

    @staticmethod
    def _left_cubic(
        start: np.ndarray, curve: dict[str, Any], amount: float
    ) -> np.ndarray:
        control_1 = np.asarray([curve["c1x"], curve["c1y"]], dtype=float)
        control_2 = np.asarray([curve["c2x"], curve["c2y"]], dtype=float)
        end = np.asarray([curve["x"], curve["y"]], dtype=float)
        first = start + (control_1 - start) * amount
        second = control_1 + (control_2 - control_1) * amount
        third = control_2 + (end - control_2) * amount
        fourth = first + (second - first) * amount
        fifth = second + (third - second) * amount
        sixth = fourth + (fifth - fourth) * amount
        return np.concatenate((first, fourth, sixth))

    @staticmethod
    def _commands_close(
        left: list[dict[str, Any]],
        right: list[dict[str, Any]],
        tolerance: float,
    ) -> bool:
        if len(left) != len(right):
            return False
        for left_command, right_command in zip(left, right, strict=True):
            if left_command.keys() != right_command.keys():
                return False
            for key, left_value in left_command.items():
                right_value = right_command[key]
                if key == "op":
                    if left_value != right_value:
                        return False
                elif abs(float(left_value) - float(right_value)) > tolerance:
                    return False
        return True

    @staticmethod
    def _simplify_keyframes(
        keyframes: list[dict[str, Any]], property_name: str
    ) -> list[dict[str, Any]]:
        if len(keyframes) <= 2:
            return keyframes
        tolerance = {
            "commands": 0.002,
            "points": 0.002,
            "vertices": 0.002,
            "fillGradient": 0.003,
            "strokeGradient": 0.003,
            "fill": 1 / 255,
            "stroke": 1 / 255,
            "colors": 1 / 255,
            "surfaceColors": 1 / 255,
            "strokeWidth": 0.0005,
            "strokeRadii": 0.0005,
            "radius": 0.0,
            "opacity": 0.0005,
            "shaderVertexData": 0.00001,
            "shaderUniformValues": 0.00001,
            # These values come from a geometry proof. Only remove keyframes
            # that are exactly collinear so the proven partial path remains
            # unchanged at every captured frame.
            "drawStart": 0.0,
            "drawProgress": 0.0,
            "drawRange": 0.0,
            "x": 0.0,
            "y": 0.0,
            "scaleX": 0.0,
            "scaleY": 0.0,
            "transform2d": 0.0,
            # Affine coefficients are independently rounded to six decimals;
            # permit only that serialization residue when proving collinearity.
            "affine2d": 0.000003,
            "trackerValue": 0.000001,
            "cameraX": 0.0,
            "cameraY": 0.0,
            "cameraZoom": 0.0,
            "billboardAnchor": 0.0,
            "camera3d": 0.0,
        }.get(property_name, 0.0005)
        vectors = [CompatibilityRenderer._vectorize(frame["value"]) for frame in keyframes]
        kept = {0, len(keyframes) - 1}

        def simplify(start: int, end: int) -> None:
            if end <= start + 1:
                return
            start_time = keyframes[start]["at"]
            end_time = keyframes[end]["at"]
            start_vector = vectors[start]
            end_vector = vectors[end]
            if (
                start_vector is None
                or end_vector is None
                or start_vector[0] != end_vector[0]
                or end_time <= start_time
            ):
                middle = start + (end - start) // 2
                kept.add(middle)
                simplify(start, middle)
                simplify(middle, end)
                return
            signature = start_vector[0]
            maximum_error = -1.0
            maximum_index = start + 1
            for index in range(start + 1, end):
                vector = vectors[index]
                if vector is None or vector[0] != signature:
                    error = math.inf
                else:
                    amount = (keyframes[index]["at"] - start_time) / (
                        end_time - start_time
                    )
                    expected = start_vector[1] + (
                        end_vector[1] - start_vector[1]
                    ) * amount
                    error = float(np.max(np.abs(vector[1] - expected)))
                if error > maximum_error:
                    maximum_error = error
                    maximum_index = index
            if maximum_error > tolerance:
                kept.add(maximum_index)
                simplify(start, maximum_index)
                simplify(maximum_index, end)

        simplify(0, len(keyframes) - 1)
        return [keyframes[index] for index in sorted(kept)]

    @staticmethod
    def _vectorize(value: Any) -> tuple[Any, np.ndarray] | None:
        if isinstance(value, (int, float)):
            return ("number", np.asarray([float(value)], dtype=float))
        if isinstance(value, str) and value.startswith("#") and len(value) in (7, 9):
            channels = [
                int(value[index : index + 2], 16) / 255
                for index in range(1, len(value), 2)
            ]
            if len(channels) == 3:
                channels.append(1.0)
            return ("color", np.asarray(channels, dtype=float))
        if isinstance(value, list):
            if all(isinstance(item, (int, float)) for item in value):
                return (
                    ("numbers", len(value)),
                    np.asarray(value, dtype=float),
                )
            if all(
                isinstance(item, str)
                and item.startswith("#")
                and len(item) in (7, 9)
                for item in value
            ):
                channels: list[float] = []
                for item in value:
                    color_vector = CompatibilityRenderer._vectorize(item)
                    if color_vector is None:
                        return None
                    channels.extend(color_vector[1].tolist())
                return (
                    ("colors", len(value)),
                    np.asarray(channels, dtype=float),
                )
            if all(
                isinstance(item, list)
                and len(item) == 2
                and all(isinstance(channel, (int, float)) for channel in item)
                for item in value
            ):
                return (
                    ("points", len(value)),
                    np.asarray(value, dtype=float).reshape(-1),
                )
            if all(
                isinstance(item, list)
                and len(item) == 3
                and all(
                    isinstance(channel, (int, float)) for channel in item
                )
                for item in value
            ):
                return (
                    ("vertices", len(value)),
                    np.asarray(value, dtype=float).reshape(-1),
                )
            if all(isinstance(item, dict) and "op" in item for item in value):
                signature: list[tuple[str, tuple[str, ...]]] = []
                channels: list[float] = []
                for command in value:
                    keys = tuple(sorted(key for key in command if key != "op"))
                    signature.append((command["op"], keys))
                    channels.extend(float(command[key]) for key in keys)
                return (
                    ("commands", tuple(signature)),
                    np.asarray(channels, dtype=float),
                )
            return None
        if isinstance(value, dict) and {"from", "to", "stops"} <= value.keys():
            stops = value["stops"]
            if not isinstance(stops, list):
                return None
            channels = [*value["from"], *value["to"]]
            for stop in stops:
                color_vector = CompatibilityRenderer._vectorize(stop["color"])
                if color_vector is None:
                    return None
                channels.append(float(stop["offset"]))
                channels.extend(color_vector[1].tolist())
            return (
                ("gradient", len(stops)),
                np.asarray(channels, dtype=float),
            )
        return None

    def _add_optional_track(
        self,
        output: list[dict[str, Any]],
        target: str,
        property_name: str,
        snapshots: list[Snapshot],
        attribute: str,
    ) -> None:
        values = [getattr(snapshot, attribute) for snapshot in snapshots]
        if all(value is None for value in values):
            return
        if any(value is None for value in values):
            self.diagnostics.add(f"internal-{property_name}-normalization-failed")
            return
        self._add_track(output, target, property_name, snapshots, attribute)

    @staticmethod
    def _normalize_optional_gradient(
        snapshots: list[Snapshot], gradient_attribute: str, color_attribute: str
    ) -> None:
        gradients = [
            getattr(snapshot, gradient_attribute)
            for snapshot in snapshots
            if getattr(snapshot, gradient_attribute) is not None
        ]
        if not gradients:
            return
        axis = {
            "from": gradients[0]["from"],
            "to": gradients[0]["to"],
        }
        for snapshot in snapshots:
            if getattr(snapshot, gradient_attribute) is not None:
                continue
            color = getattr(snapshot, color_attribute)
            setattr(
                snapshot,
                gradient_attribute,
                {
                    **axis,
                    "stops": [
                        {"offset": 0.0, "color": color},
                        {"offset": 1.0, "color": color},
                    ],
                },
            )


def import_scene(path: Path, class_name: str) -> type[Scene]:
    module_name = f"_realtime_manim_source_{path.stem}"
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Could not import {path}.")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    candidate = getattr(module, class_name, None)
    if not isinstance(candidate, type) or not issubclass(candidate, Scene):
        raise TypeError(f"{class_name} is not a Manim Scene in {path}.")
    return candidate


def compile_scene(
    source: Path,
    class_name: str,
    fps: int,
    renderer_name: str = "auto",
    *,
    semantic_billboards: bool = True,
    compact_surface_lifetimes: bool = True,
    full_affine_tracks: bool = True,
) -> tuple[dict[str, Any], dict[str, Any]]:
    if renderer_name == "auto":
        source_text = source.read_text()
        renderer_name = (
            "opengl"
            if "RendererType.OPENGL" in source_text
            or "renderer = \"opengl\"" in source_text
            or "renderer = 'opengl'" in source_text
            or "manim.mobject.opengl" in source_text
            else "cairo"
        )
    config.renderer = (
        RendererType.OPENGL
        if renderer_name == "opengl"
        else RendererType.CAIRO
    )
    config.frame_rate = fps
    config.disable_caching = True
    config.preview = False
    config.progress_bar = "none"
    if media_dir := os.environ.get("REALTIME_MANIM_MEDIA_DIR"):
        config.media_dir = Path(media_dir).resolve()
    # The retained artifact must be reproducible across compiles and seeks.
    # User code can still choose a different seed explicitly in construct().
    random.seed(0)
    np.random.seed(0)
    config.seed = 0
    REGISTERED_VALUE_TRACKERS.clear()
    renderer = CompatibilityRenderer(
        fps,
        renderer_name,
        semantic_billboards=semantic_billboards,
        compact_surface_lifetimes=compact_surface_lifetimes,
        full_affine_tracks=full_affine_tracks,
    )
    scene_class = import_scene(source, class_name)
    scene = scene_class(renderer=renderer)
    scene.render(preview=False)
    return renderer.to_scene(), renderer.receipt()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compile a regular ManimCE Scene into realtime-manim Rust IR."
    )
    parser.add_argument("source", type=Path)
    parser.add_argument("scene_class")
    parser.add_argument("--fps", type=int, default=30, choices=range(1, 121))
    parser.add_argument(
        "--renderer",
        choices=("auto", "cairo", "opengl"),
        default="auto",
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--allow-partial",
        action="store_true",
        help="write output even when compatibility diagnostics are present",
    )
    parser.add_argument(
        "--disable-semantic-billboards",
        action="store_true",
        help="retain camera-baked fixed-orientation paths for differential testing",
    )
    parser.add_argument(
        "--disable-compact-surface-lifetimes",
        action="store_true",
        help="retain topology-changing Cairo surfaces as expanded meshes for differential testing",
    )
    parser.add_argument(
        "--disable-full-affine-tracks",
        action="store_true",
        help="retain non-similarity affine path motion as sampled path data for differential testing",
    )
    arguments = parser.parse_args()
    source = arguments.source.expanduser().resolve()
    scene, receipt = compile_scene(
        source,
        arguments.scene_class,
        arguments.fps,
        arguments.renderer,
        semantic_billboards=not arguments.disable_semantic_billboards,
        compact_surface_lifetimes=not arguments.disable_compact_surface_lifetimes,
        full_affine_tracks=not arguments.disable_full_affine_tracks,
    )
    diagnostics = receipt["diagnostics"]
    if diagnostics and not arguments.allow_partial:
        print(
            "Compatibility compilation found unsupported capabilities: "
            + ", ".join(diagnostics),
            file=sys.stderr,
        )
        return 2
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(scene, separators=(",", ":")))
    receipt_path = arguments.output.with_suffix(".receipt.json")
    receipt_path.write_text(json.dumps(receipt, indent=2))
    print(
        json.dumps(
            {
                "output": str(arguments.output.resolve()),
                "duration": scene["duration"],
                "fps": scene["fps"],
                "nodes": len(scene["nodes"]),
                "tracks": len(scene["tracks"]),
                "sampledFrames": receipt["sampledFrames"],
                "receipt": str(receipt_path.resolve()),
                "diagnostics": diagnostics,
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    os.environ.setdefault("TEXMFROOT", "/opt/homebrew/opt/texlive/share")
    os.environ.setdefault(
        "TEXMFCNF", "/opt/homebrew/opt/texlive/share/texmf-dist/web2c"
    )
    raise SystemExit(main())
