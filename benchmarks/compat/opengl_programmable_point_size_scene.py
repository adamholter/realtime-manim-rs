from pathlib import Path

import moderngl
import numpy as np
from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_mobject import OpenGLMobject

config.renderer = RendererType.OPENGL


class ProgrammablePoints(OpenGLMobject):
    shader_folder = (
        Path(__file__).parent / "custom_shader" / "programmable_points"
    )
    shader_dtype = [("point", np.float32, (2,))]

    def __init__(self, **kwargs):
        self.programmable_size = 44.0
        super().__init__(**kwargs)
        self.render_primitive = moderngl.POINTS

    def init_points(self):
        self.points = np.array(
            [[-0.55, -0.35, 0], [0.0, 0.45, 0], [0.6, -0.2, 0]],
            dtype=np.float32,
        )

    def get_shader_data(self):
        data = np.zeros(len(self.points), dtype=self.shader_dtype)
        data["point"] = self.points[:, :2]
        return data

    def get_shader_uniforms(self):
        return {"point_size": self.programmable_size}


class OpenGLProgrammablePointSizeCompatibility(Scene):
    def construct(self):
        points = ProgrammablePoints()
        self.add(points)
        self.play(
            UpdateFromAlphaFunc(
                points,
                lambda mob, alpha: setattr(
                    mob, "programmable_size", 44.0 + 32.0 * alpha
                ),
            ),
            run_time=1,
        )
