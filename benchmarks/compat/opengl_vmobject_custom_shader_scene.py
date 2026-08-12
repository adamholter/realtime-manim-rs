from pathlib import Path

from manim import *
from manim.constants import RendererType

config.renderer = RendererType.OPENGL

SHADER_FOLDER = Path(__file__).parent / "custom_shader" / "vmobject_passthrough"


class ShaderSquare(Square):
    fill_shader_folder = SHADER_FOLDER

    def __init__(self, **kwargs):
        super().__init__(
            side_length=2.4,
            fill_color=RED,
            fill_opacity=1,
            stroke_opacity=0,
            **kwargs,
        )


class OpenGLVMobjectCustomShaderCompatibility(Scene):
    def construct(self):
        square = ShaderSquare()
        self.add(square)
        self.play(square.animate.shift(RIGHT * 1.2 + UP * 0.6), run_time=1)


class OpenGLVMobjectCustomShaderMidframe(Scene):
    def construct(self):
        self.add(ShaderSquare().shift(RIGHT * 0.6 + UP * 0.3))
