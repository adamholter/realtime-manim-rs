from pathlib import Path

from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_surface import (
    OpenGLSurface,
    OpenGLTexturedSurface,
)

config.renderer = RendererType.OPENGL


class OpenGLDarkTextureCompatibility(ThreeDScene):
    def construct(self):
        image_root = (
            Path(__file__).resolve().parents[1]
            / "corpus"
            / "reference"
            / "images"
            / "manim_reference_scenes"
        )
        light_image = image_root / "PrimitiveGallery_ManimCE_v0.20.1.png"
        dark_image = image_root / "TextAndMath_ManimCE_v0.20.1.png"
        surface = OpenGLSurface(
            lambda u, v: [u, v, 0.45 * np.sin(u * 1.7) * np.cos(v * 2.1)],
            u_range=[-2.5, 2.5],
            v_range=[-1.6, 1.6],
            resolution=(20, 16),
            gloss=0.22,
            shadow=0.32,
        )
        textured = OpenGLTexturedSurface(
            surface,
            light_image,
            dark_image_file=dark_image,
        )
        textured.resolution = surface.resolution
        textured.compute_triangle_indices()
        # Normalize Manim 0.20.1's PIL/path mismatch for its own renderer.
        textured.texture_paths = {
            "LightTexture": str(light_image),
            "DarkTexture": str(dark_image),
        }
        self.set_camera_orientation(phi=61 * DEGREES, theta=-34 * DEGREES)
        self.add(textured)
        self.begin_ambient_camera_rotation(rate=0.16)
        self.wait(1)
