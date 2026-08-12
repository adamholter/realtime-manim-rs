from pathlib import Path

import moderngl
import numpy as np
from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_mobject import OpenGLMobject

config.renderer = RendererType.OPENGL


class DiamondPointCloud(OpenGLMobject):
    shader_folder = (
        Path(__file__).parent / "custom_shader" / "diamond_points"
    )
    shader_dtype = [
        ("point", np.float32, (3,)),
        ("color", np.float32, (4,)),
    ]
    render_primitive = moderngl.POINTS

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.render_primitive = moderngl.POINTS

    def init_points(self):
        self.points = np.array(
            [[-3.0, -1.0, 0.0], [0.0, 1.2, 0.0], [3.0, -0.4, 0.0]],
            dtype=np.float32,
        )
        self.rgbas = np.array(
            [RED.to_rgba(), GREEN.to_rgba(), BLUE.to_rgba()],
            dtype=np.float32,
        )

    def get_shader_data(self):
        data = np.zeros(len(self.points), dtype=self.shader_dtype)
        data["point"] = self.points
        data["color"] = self.rgbas
        return data


class OpenGLGeometryShaderCompatibility(Scene):
    def construct(self):
        cloud = DiamondPointCloud()
        self.add(cloud)
        self.play(cloud.animate.shift(UP * 1.4 + RIGHT * 0.8), run_time=1)


class OpenGLGeometryShaderMidframe(Scene):
    def construct(self):
        self.add(DiamondPointCloud().shift(UP * 0.7 + RIGHT * 0.4))
