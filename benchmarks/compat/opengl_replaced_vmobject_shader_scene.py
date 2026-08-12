from manim import *
from manim.constants import RendererType

config.renderer = RendererType.OPENGL


def replaced_square():
    square = Square(
        side_length=2.5,
        fill_color=WHITE,
        fill_opacity=1,
        stroke_opacity=0,
    )
    square.replace_shader_code(
        "frag_color = color;",
        "frag_color = vec4(0.1, 0.82, 1.0, color.a);",
    )
    return square


class OpenGLReplacedVMobjectShaderCompatibility(Scene):
    def construct(self):
        square = replaced_square()
        self.add(square)
        self.play(square.animate.rotate(0.5).shift(RIGHT), run_time=1)


class OpenGLReplacedVMobjectShaderMidframe(Scene):
    def construct(self):
        self.add(replaced_square().rotate(0.25).shift(RIGHT * 0.5))
