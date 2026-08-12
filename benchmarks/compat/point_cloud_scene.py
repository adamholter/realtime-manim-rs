from manim import *
import numpy as np


class PointCloudCompatibility(Scene):
    def construct(self):
        cloud = PointCloudDot(
            radius=1.35,
            density=8,
            stroke_width=3,
            color=BLUE,
        ).shift(LEFT * 2)
        palette = color_gradient([BLUE, PURPLE, PINK], len(cloud.points))
        cloud.rgbas = np.asarray([color.to_rgba() for color in palette])
        self.play(FadeIn(cloud))
        self.play(cloud.animate.shift(RIGHT * 4).scale(0.72).rotate(PI / 5))
