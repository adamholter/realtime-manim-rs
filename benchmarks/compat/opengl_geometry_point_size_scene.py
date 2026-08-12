from pathlib import Path

import moderngl
import numpy as np
from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_mobject import OpenGLMobject

config.renderer = RendererType.OPENGL


class GeometrySizedPoints(OpenGLMobject):
    shader_folder = (
        Path(__file__).parent / "custom_shader" / "geometry_sized_points"
    )
    shader_dtype = [
        ("point", np.float32, (2,)),
        ("size", np.float32, (1,)),
        ("tint", np.float32, (3,)),
    ]

    def __init__(self, **kwargs):
        self.pulse = 1.0
        super().__init__(**kwargs)
        self.render_primitive = moderngl.POINTS

    def init_points(self):
        self.points = np.array(
            [[-0.62, -0.32, 0.0], [0.0, 0.42, 0.0], [0.62, -0.2, 0.0]],
            dtype=np.float32,
        )
        self.sizes = np.array([38.0, 54.0, 70.0], dtype=np.float32)
        self.tints = np.array(
            [[0.12, 0.82, 1.0], [1.0, 0.24, 0.65], [0.6, 0.92, 0.18]],
            dtype=np.float32,
        )

    def get_shader_data(self):
        data = np.zeros(len(self.points), dtype=self.shader_dtype)
        data["point"] = self.points[:, :2]
        data["size"][:, 0] = self.sizes
        data["tint"] = self.tints
        return data

    def get_shader_uniforms(self):
        return {"pulse": self.pulse}


class OpenGLGeometryPointSizeCompatibility(Scene):
    def construct(self):
        points = GeometrySizedPoints()
        self.add(points)
        self.play(
            UpdateFromAlphaFunc(
                points,
                lambda mob, alpha: setattr(mob, "pulse", 1.0 + 0.45 * alpha),
            ),
            run_time=1,
        )
