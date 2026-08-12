import numpy as np
from manim import *


class IntersectingOpaqueSurfacesCompatibility(ThreeDScene):
    def construct(self):
        horizontal = Surface(
            lambda u, v: np.array([u, v, 0.0]),
            u_range=[-2.5, 2.5],
            v_range=[-2.0, 2.0],
            resolution=(8, 8),
        )
        vertical = Surface(
            lambda u, v: np.array([u, 0.0, v]),
            u_range=[-2.5, 2.5],
            v_range=[-2.0, 2.0],
            resolution=(8, 8),
        )
        horizontal.set_style(
            fill_color=RED_D,
            fill_opacity=1.0,
            stroke_color=RED_D,
            stroke_width=0.0,
        )
        vertical.set_style(
            fill_color=BLUE_D,
            fill_opacity=1.0,
            stroke_color=BLUE_D,
            stroke_width=0.0,
        )
        self.set_camera_orientation(phi=68 * DEGREES, theta=-38 * DEGREES)
        self.add(horizontal, vertical)
        self.wait(0.5)
