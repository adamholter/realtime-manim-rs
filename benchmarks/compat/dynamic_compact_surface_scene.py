import numpy as np
from manim import *


class DynamicCompactSurfaceCompatibility(ThreeDScene):
    def construct(self):
        surface = Surface(
            lambda u, v: np.array(
                [u, v, 0.18 * np.sin(1.7 * u) * np.cos(1.4 * v)]
            ),
            u_range=[-2.8, 2.8],
            v_range=[-1.8, 1.8],
            resolution=(12, 8),
        )
        surface.set_style(
            fill_color=BLUE_D,
            fill_opacity=0.82,
            stroke_color=WHITE,
            stroke_width=0.65,
        )
        original = surface.copy()
        self.set_camera_orientation(phi=62 * DEGREES, theta=-42 * DEGREES)
        self.add(surface)

        def deform(mobject, alpha):
            mobject.become(original)
            mobject.apply_function(
                lambda point: point
                + OUT
                * alpha
                * 0.55
                * np.sin(1.3 * point[0])
                * np.cos(1.1 * point[1])
            )
            mobject.set_style(
                fill_color=interpolate_color(BLUE_D, RED_D, alpha),
                fill_opacity=0.82,
                stroke_color=interpolate_color(WHITE, YELLOW, alpha),
                stroke_width=0.65 + 0.7 * alpha,
            )

        self.play(UpdateFromAlphaFunc(surface, deform), run_time=1.2)
