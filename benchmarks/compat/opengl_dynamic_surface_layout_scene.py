import numpy as np
from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_surface import OpenGLSurface

config.renderer = RendererType.OPENGL


class OpenGLDynamicSurfaceLayoutCompatibility(ThreeDScene):
    def construct(self):
        surface = OpenGLSurface(
            lambda u, v: [u, v, 0.25 * np.sin(u) * np.cos(v)],
            u_range=[-2.4, 2.4],
            v_range=[-1.5, 1.5],
            resolution=(8, 6),
            gloss=0.05,
            shadow=0.1,
        )
        full_indices = surface.triangle_indices.copy()
        reduced_indices = full_indices[:-6].copy()
        self.set_camera_orientation(phi=58 * DEGREES, theta=-34 * DEGREES)
        self.add(surface)

        def update_layout(mob, alpha):
            mob.gloss = 0.05 + 0.75 * alpha
            mob.shadow = 0.1 + 0.6 * alpha
            mob.triangle_indices = (
                full_indices if alpha < 0.5 else reduced_indices
            )

        self.play(
            UpdateFromAlphaFunc(surface, update_layout), run_time=0.8
        )
