import numpy as np

from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_mobject import OpenGLMobject

config.renderer = RendererType.OPENGL


class PluginShaderTriangle(OpenGLMobject):
    shader_folder = "surface"
    shader_dtype = [
        ("point", np.float32, (3,)),
        ("du_point", np.float32, (3,)),
        ("dv_point", np.float32, (3,)),
        ("color", np.float32, (4,)),
    ]

    def __init__(self, **kwargs):
        self.vertex_colors = np.array(
            [
                ManimColor(RED).to_rgba(),
                ManimColor(GREEN).to_rgba(),
                ManimColor(BLUE).to_rgba(),
            ],
            dtype=np.float32,
        )
        super().__init__(gloss=0, shadow=0, **kwargs)
        self.shader_indices = np.array([0, 1, 2], dtype=np.int32)

    def init_points(self):
        self.points = np.array(
            [[-2.2, -1.4, 0], [2.2, -1.4, 0], [0, 2.0, 0]],
            dtype=np.float32,
        )

    def get_shader_data(self):
        data = np.zeros(len(self.points), dtype=self.shader_dtype)
        data["point"] = self.points
        data["du_point"] = self.points + [0.01, 0, 0]
        data["dv_point"] = self.points + [0, 0.01, 0]
        data["color"] = self.vertex_colors
        return data


class OpenGLPluginCompatibility(ThreeDScene):
    def construct(self):
        triangle = PluginShaderTriangle()
        self.set_camera_orientation(phi=52 * DEGREES, theta=-28 * DEGREES)
        self.add(triangle)
        self.play(triangle.animate.rotate(0.45, axis=UP), run_time=1)
