from manim import *


class FullAffineScene(Scene):
    def construct(self):
        shape = (
            RoundedRectangle(width=4.2, height=2.4, corner_radius=0.35)
            .set_fill(color=["#264653", "#2A9D8F", "#E9C46A"], opacity=1)
            .set_stroke(color=["#EAF7F4", "#F4A261"], width=5)
        )
        cutout = (
            Triangle()
            .scale(0.65)
            .shift(LEFT * 0.8)
            .set_fill("#E9C46A", opacity=1)
            .set_stroke("#5B4520", width=4)
        )
        group = VGroup(shape, cutout)
        self.add(group)
        self.play(
            ApplyMatrix([[1.0, 0.62], [0.28, 1.0]], group),
            run_time=1.2,
            rate_func=linear,
        )
