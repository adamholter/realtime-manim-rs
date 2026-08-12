from pathlib import Path

import moderngl
import numpy as np
from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_mobject import OpenGLMobject

config.renderer = RendererType.OPENGL


class UniformArrayTriangle(OpenGLMobject):
    shader_folder = (
        Path(__file__).parent / "custom_shader" / "uniform_arrays"
    )
    shader_dtype = [("point", np.float32, (2,))]

    def __init__(self, **kwargs):
        self.weights = (0.15, 0.85)
        super().__init__(**kwargs)
        self.render_primitive = moderngl.TRIANGLES

    def init_points(self):
        self.points = np.array(
            [[-0.7, -0.55, 0], [0.7, -0.55, 0], [0, 0.72, 0]],
            dtype=np.float32,
        )

    def get_shader_data(self):
        data = np.zeros(len(self.points), dtype=self.shader_dtype)
        data["point"] = self.points[:, :2]
        return data

    def get_shader_uniforms(self):
        return {"weights": self.weights}


class OpenGLUniformArrayShaderCompatibility(Scene):
    def construct(self):
        triangle = UniformArrayTriangle()
        self.add(triangle)
        self.play(
            UpdateFromAlphaFunc(
                triangle,
                lambda mob, alpha: setattr(
                    mob, "weights", (0.15 + 0.7 * alpha, 0.85 - 0.7 * alpha)
                ),
            ),
            run_time=1,
        )
