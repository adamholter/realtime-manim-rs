from pathlib import Path

from PIL import Image
from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_surface import (
    OpenGLSurface,
    OpenGLTexturedSurface,
)

config.renderer = RendererType.OPENGL


class OpenGLTextureResamplingCompatibility(ThreeDScene):
    def construct(self):
        image_path = (
            Path(__file__).resolve().parents[1]
            / "corpus"
            / "reference"
            / "images"
            / "manim_reference_scenes"
            / "PrimitiveGallery_ManimCE_v0.20.1.png"
        )
        filters = [
            Image.Resampling.NEAREST,
            Image.Resampling.BOX,
            Image.Resampling.BILINEAR,
            Image.Resampling.HAMMING,
            Image.Resampling.BICUBIC,
            Image.Resampling.LANCZOS,
        ]
        positions = [
            [-4.5, 1.7, 0.0],
            [0.0, 1.7, 0.0],
            [4.5, 1.7, 0.0],
            [-4.5, -1.7, 0.0],
            [0.0, -1.7, 0.0],
            [4.5, -1.7, 0.0],
        ]
        surfaces = []
        for algorithm, position in zip(filters, positions, strict=True):
            plane = OpenGLSurface(
                lambda u, v: [u, v, 0.0],
                u_range=[-1.7, 1.7],
                v_range=[-1.0, 1.0],
                resolution=(3, 3),
                gloss=0.0,
                shadow=0.0,
            )
            textured = OpenGLTexturedSurface(plane, image_path)
            textured.resolution = plane.resolution
            textured.compute_triangle_indices()
            textured.resampling_algorithm = algorithm
            textured.shift(position)
            surfaces.append(textured)
        self.set_camera_orientation(phi=0, theta=-90 * DEGREES)
        self.add(*surfaces)
        self.wait(0.25)
