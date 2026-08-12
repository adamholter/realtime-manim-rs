from manim import *
from manim.mobject.opengl.opengl_geometry import OpenGLCircle, OpenGLSquare
from manim.mobject.opengl.opengl_point_cloud_mobject import OpenGLPMobject


class OpenGLCompatibility(Scene):
    def construct(self):
        circle = OpenGLCircle(radius=1.2, color=BLUE).shift(LEFT * 2)
        square = OpenGLSquare(side_length=2, color=PINK).shift(RIGHT * 2)
        cloud = OpenGLPMobject(color=YELLOW, stroke_width=6)
        cloud.set_points([UP * 1.2, DOWN * 1.2, LEFT * 1.2, RIGHT * 1.2])
        self.add(circle, square, cloud)
        self.play(circle.animate.rotate(PI / 2), square.animate.scale(0.65))
