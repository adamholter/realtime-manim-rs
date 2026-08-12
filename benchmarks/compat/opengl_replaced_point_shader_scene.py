from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_point_cloud_mobject import OpenGLPMPoint
import numpy as np

config.renderer = RendererType.OPENGL


class ReplacedShaderPoint(OpenGLPMPoint):
    """Exercise a changed stock true-dot program, including its geometry stage."""

    def get_shader_wrapper(self):
        wrapper = super().get_shader_wrapper()
        wrapper.replace_code(
            r"frag_color\.a \*=",
            "frag_color.rgb = vec3(0.98, 0.18, 0.62);\n"
            "    frag_color.a *=",
        )
        return wrapper


class OpenGLReplacedPointShaderCompatibility(Scene):
    def construct(self):
        point = ReplacedShaderPoint(
            [-1.3, -0.5, 0],
            stroke_width=76,
            color=BLUE,
        )
        self.add(point)
        self.play(
            point.animate.shift(RIGHT * 2.6 + UP),
            run_time=1,
        )


class OpenGLReplacedPointShaderMidframe(Scene):
    def construct(self):
        self.add(
            ReplacedShaderPoint(
                [0, 0, 0],
                stroke_width=76,
                color=BLUE,
            )
        )


class OpenGLDynamicGeometryShaderTopologyCompatibility(Scene):
    def construct(self):
        point = ReplacedShaderPoint(
            [-0.8, -0.4, 0],
            stroke_width=54,
            color=BLUE,
        )
        first = point.points.copy()
        second = np.vstack([first, [[0.8, 0.4, 0]]]).astype(np.float32)
        self.add(point)
        self.play(
            UpdateFromAlphaFunc(
                point,
                lambda mob, alpha: setattr(
                    mob,
                    "points",
                    first if alpha < 0.5 else second,
                ),
            ),
            run_time=1,
        )
