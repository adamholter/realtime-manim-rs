from pathlib import Path

import numpy as np
from manim import *


class MovingCameraImageCompatibility(MovingCameraScene):
    def construct(self):
        image_path = (
            Path(__file__).resolve().parents[1]
            / "corpus"
            / "reference"
            / "images"
            / "manim_reference_scenes"
            / "PrimitiveGallery_ManimCE_v0.20.1.png"
        )
        image = ImageMobject(image_path).set_height(4.2)
        border = SurroundingRectangle(image, color=YELLOW, buff=0.08)
        marker = Dot(image.get_corner(UR), color=RED, radius=0.12)
        point_cloud = PMobject()
        point_cloud.add_points(
            np.array([[-3.2, -1.4, 0], [-2.6, -0.9, 0], [-2.0, -1.3, 0]]),
            color=GREEN,
        )
        point_cloud.stroke_width = 18
        gradient = Circle(radius=0.62).shift(LEFT * 2.6 + UP * 1.1)
        gradient.set_fill(color=[BLUE, GREEN, YELLOW], opacity=0.72)
        gradient.set_stroke(color=[PURPLE, YELLOW], width=5)
        self.add(image, border, marker, point_cloud, gradient)
        self.play(
            self.camera.frame.animate.scale(0.58).move_to(marker),
            run_time=1.2,
        )
