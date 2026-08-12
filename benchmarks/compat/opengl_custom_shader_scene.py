import numpy as np
from pathlib import Path

from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_surface import OpenGLSurface

config.renderer = RendererType.OPENGL


class WaveShaderSurface(OpenGLSurface):
    shader_folder = Path(__file__).parent / "custom_shader" / "wave_surface"

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.uniforms.update({"bands": 5, "invert": False})


def wave_surface():
    return WaveShaderSurface(
            lambda u, v: np.array(
                [u, v, 0.45 * np.sin(1.3 * u) * np.cos(1.6 * v)]
            ),
            u_range=(-3.2, 3.2),
            v_range=(-2.0, 2.0),
            resolution=(33, 25),
            color=WHITE,
            gloss=0,
            shadow=0,
        )


class OpenGLCustomShaderCompatibility(ThreeDScene):
    def construct(self):
        surface = wave_surface()
        self.set_camera_orientation(phi=58 * DEGREES, theta=-32 * DEGREES)
        self.add(surface)
        self.play(
            surface.animate.rotate(0.35, axis=UP),
            UpdateFromAlphaFunc(
                surface,
                lambda mob, alpha: mob.uniforms.update(
                    {
                        "bands": int(3 + 5 * alpha),
                        "invert": alpha > 0.72,
                    }
                ),
            ),
            run_time=1,
        )


class OpenGLCustomShaderMidframe(ThreeDScene):
    def construct(self):
        surface = wave_surface().rotate(0.175, axis=UP)
        surface.uniforms.update({"bands": 5, "invert": False})
        self.set_camera_orientation(phi=58 * DEGREES, theta=-32 * DEGREES)
        self.add(surface)
