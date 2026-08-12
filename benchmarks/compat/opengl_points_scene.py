from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_point_cloud_mobject import OpenGLPMPoint

config.renderer = RendererType.OPENGL


class OpenGLPointsCompatibility(ThreeDScene):
    def construct(self):
        points = [
            OpenGLPMPoint([-2.4, -0.8, -0.5], stroke_width=12, color=RED),
            OpenGLPMPoint([-0.8, 0.7, 0.2], stroke_width=16, color=YELLOW),
            OpenGLPMPoint([0.9, -0.3, 0.8], stroke_width=20, color=GREEN),
            OpenGLPMPoint([2.4, 0.9, 1.4], stroke_width=24, color=BLUE),
        ]
        self.set_camera_orientation(phi=60 * DEGREES, theta=-35 * DEGREES)
        self.add(*points)
        self.begin_ambient_camera_rotation(rate=0.18)
        self.wait(1)


class DynamicOpenGLPointColorCompatibility(Scene):
    def construct(self):
        point = OpenGLPMPoint([0, 0, 0], stroke_width=36, color=BLUE)
        self.add(point)
        self.play(point.animate.set_color(RED).shift(RIGHT * 2), run_time=1)
