from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_surface import OpenGLSurface

config.renderer = RendererType.OPENGL


class OpenGLSurfaceCompatibility(ThreeDScene):
    def construct(self):
        surface = OpenGLSurface(
            lambda u, v: [u, v, 0.45 * np.sin(u * 2) * np.cos(v * 2)],
            u_range=[-2.4, 2.4],
            v_range=[-1.8, 1.8],
            resolution=(20, 16),
            color=BLUE,
            gloss=0.25,
            shadow=0.35,
        )
        self.set_camera_orientation(phi=65 * DEGREES, theta=-35 * DEGREES)
        self.add(surface)
        self.begin_ambient_camera_rotation(rate=0.3)
        self.wait(2)


class DynamicOpenGLColorCompatibility(ThreeDScene):
    def construct(self):
        surface = OpenGLSurface(
            lambda u, v: [u, v, 0.35 * np.sin(u * 2) * np.cos(v * 2)],
            u_range=[-2.2, 2.2],
            v_range=[-1.5, 1.5],
            resolution=(12, 10),
            color=BLUE,
            gloss=0.2,
            shadow=0.3,
        )
        self.set_camera_orientation(phi=62 * DEGREES, theta=-38 * DEGREES)
        self.add(surface)
        self.play(surface.animate.set_color(RED), run_time=1)
