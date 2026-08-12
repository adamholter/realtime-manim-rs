from pathlib import Path

import moderngl
import numpy as np
from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_mobject import OpenGLMobject

config.renderer = RendererType.OPENGL


class MatrixAttributeTriangle(OpenGLMobject):
    shader_folder = (
        Path(__file__).parent / "custom_shader" / "matrix_attributes"
    )
    shader_dtype = [
        ("point", np.float32, (2,)),
        ("transform", np.float32, (2, 2)),
    ]

    def __init__(self, **kwargs):
        self.matrix = np.eye(2, dtype=np.float32)
        super().__init__(**kwargs)
        self.render_primitive = moderngl.TRIANGLES

    def init_points(self):
        self.points = np.array(
            [[-0.65, -0.5, 0], [0.65, -0.5, 0], [0, 0.72, 0]],
            dtype=np.float32,
        )

    def get_shader_data(self):
        data = np.zeros(len(self.points), dtype=self.shader_dtype)
        data["point"] = self.points[:, :2]
        data["transform"] = self.matrix
        return data


class OpenGLMatrixAttributeShaderCompatibility(Scene):
    def construct(self):
        triangle = MatrixAttributeTriangle()
        self.add(triangle)
        self.play(
            UpdateFromAlphaFunc(
                triangle,
                lambda mob, alpha: setattr(
                    mob,
                    "matrix",
                    np.array(
                        [
                            [1.0, 0.25 * alpha],
                            [-0.2 * alpha, 1.0 - 0.15 * alpha],
                        ],
                        dtype=np.float32,
                    ),
                ),
            ),
            run_time=1,
        )
