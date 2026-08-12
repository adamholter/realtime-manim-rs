import numpy as np
from manim import *


class TopologyChangingCompactSurfaceCompatibility(ThreeDScene):
    def construct(self):
        surface = Surface(
            lambda u, v: np.array([u, v, 0.15 * u * v]),
            u_range=[-2.4, 2.4],
            v_range=[-1.4, 1.4],
            resolution=(2, 1),
        )
        surface.set_style(
            fill_color=BLUE_D,
            fill_opacity=0.86,
            stroke_color=WHITE,
            stroke_width=1.0,
        )
        original = surface.copy()
        self.set_camera_orientation(phi=62 * DEGREES, theta=-48 * DEGREES)
        self.add(surface)

        def change_connectivity(mobject, alpha):
            mobject.become(original)
            if alpha >= 0.5:
                right = mobject.submobjects[1]
                right.points += np.array([0.42, 0.0, 0.34])
            mobject.set_fill(
                interpolate_color(BLUE_D, TEAL_D, alpha), opacity=0.86
            )
            mobject.set_stroke(
                interpolate_color(WHITE, YELLOW, alpha),
                width=1.0 + 0.5 * alpha,
            )

        self.play(
            UpdateFromAlphaFunc(surface, change_connectivity), run_time=1.2
        )
