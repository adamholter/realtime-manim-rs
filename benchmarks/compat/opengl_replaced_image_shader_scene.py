from pathlib import Path

from manim import *
from manim.constants import RendererType
from manim.mobject.opengl.opengl_image_mobject import OpenGLImageMobject

config.renderer = RendererType.OPENGL

FIXTURE = (
    Path(__file__).resolve().parents[1]
    / "corpus"
    / "reference"
    / "images"
    / "manim_reference_scenes"
    / "PrimitiveGallery_ManimCE_v0.20.1.png"
)


class ReplacedShaderImage(OpenGLImageMobject):
    """Exercise a changed stock textured-surface fragment program."""

    def get_shader_wrapper(self):
        wrapper = super().get_shader_wrapper()
        wrapper.replace_code(
            r"frag_color\.a = v_opacity;",
            "frag_color.rgb = mix("
            "frag_color.rgb, vec3(0.05, 0.92, 0.78), 0.58"
            ");\n"
            "    frag_color.a = v_opacity;",
        )
        return wrapper


def replaced_image():
    image = ReplacedShaderImage(FIXTURE, width=5.2, gloss=0, shadow=0)
    image.texture_paths = {
        "LightTexture": str(FIXTURE),
        "DarkTexture": str(FIXTURE),
    }
    return image


class OpenGLReplacedImageShaderCompatibility(Scene):
    def construct(self):
        image = replaced_image()
        self.add(image)
        self.play(image.animate.rotate(0.3).shift(RIGHT), run_time=1)


class OpenGLReplacedImageShaderMidframe(Scene):
    def construct(self):
        self.add(replaced_image().rotate(0.15).shift(RIGHT * 0.5))
