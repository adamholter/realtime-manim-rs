from pathlib import Path

import moderngl
import numpy as np
from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_mobject import OpenGLMobject

config.renderer = RendererType.OPENGL


class IntegerAttributeTriangle(OpenGLMobject):
    shader_folder = (
        Path(__file__).parent / "custom_shader" / "integer_attributes"
    )
    shader_dtype = [
        ("point", np.float32, (2,)),
        ("band", np.int32, (1,)),
    ]

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.render_primitive = moderngl.TRIANGLES

    def init_points(self):
        self.points = np.array(
            [[-0.65, -0.55, 0], [0.65, -0.55, 0], [0, 0.7, 0]],
            dtype=np.float32,
        )

    def get_shader_data(self):
        data = np.zeros(len(self.points), dtype=self.shader_dtype)
        data["point"] = self.points[:, :2]
        data["band"] = np.ones((len(self.points), 1), dtype=np.int32)
        return data


class OpenGLIntegerAttributeShaderCompatibility(Scene):
    def construct(self):
        triangle = IntegerAttributeTriangle()
        self.add(triangle)
        self.play(triangle.animate.shift(RIGHT * 0.4 + UP * 0.25), run_time=1)


class OpenGLIntegerAttributeShaderMidframe(Scene):
    def construct(self):
        self.add(IntegerAttributeTriangle().shift(RIGHT * 0.2 + UP * 0.125))


class OpenGLDynamicShaderTopologyCompatibility(Scene):
    def construct(self):
        triangle = IntegerAttributeTriangle()
        first = triangle.points.copy()
        second = np.array(
            [[-0.35, -0.2, 0], [0.35, -0.2, 0], [0, 0.5, 0]],
            dtype=np.float32,
        )
        self.add(triangle)
        self.play(
            UpdateFromAlphaFunc(
                triangle,
                lambda mob, alpha: setattr(
                    mob,
                    "points",
                    first if alpha < 0.5 else np.vstack([first, second]),
                ),
            ),
            run_time=1,
        )
