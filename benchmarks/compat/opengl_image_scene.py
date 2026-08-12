from pathlib import Path

from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_image_mobject import OpenGLImageMobject

config.renderer = RendererType.OPENGL


class OpenGLImageCompatibility(ThreeDScene):
    def construct(self):
        fixture = (
            Path(__file__).resolve().parents[1]
            / "corpus"
            / "reference"
            / "images"
            / "manim_reference_scenes"
            / "PrimitiveGallery_ManimCE_v0.20.1.png"
        )
        image = OpenGLImageMobject(fixture, width=5.2, gloss=0, shadow=0)
        # Manim 0.20.1 stores PIL objects where its OpenGL renderer expects
        # texture paths, so normalize the official object's shader inputs.
        image.texture_paths = {
            "LightTexture": str(fixture),
            "DarkTexture": str(fixture),
        }
        image.rotate(18 * DEGREES, axis=UP)
        image.rotate(-10 * DEGREES, axis=RIGHT)
        self.set_camera_orientation(phi=58 * DEGREES, theta=-32 * DEGREES)
        self.add(image)
        self.begin_ambient_camera_rotation(rate=0.22)
        self.wait(1)
