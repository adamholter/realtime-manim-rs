"""Behavioral fixtures for every public Manim Scene subclass."""

from manim import *


class SceneCompatibility(Scene):
    def construct(self):
        square = Square().set_fill(BLUE, opacity=0.4)
        self.play(FadeIn(square), run_time=0.2)


class MovingCameraSceneCompatibility(MovingCameraScene):
    def construct(self):
        target = Circle(radius=0.5).shift(RIGHT * 2)
        self.add(target)
        self.play(self.camera.frame.animate.move_to(target).set(width=4), run_time=0.3)


class ThreeDSceneCompatibility(ThreeDScene):
    def construct(self):
        axes = ThreeDAxes()
        surface = Surface(
            lambda u, v: np.array([u, v, 0.25 * np.sin(2 * u) * np.cos(2 * v)]),
            u_range=(-1.5, 1.5),
            v_range=(-1.0, 1.0),
            resolution=(8, 6),
        )
        self.set_camera_orientation(phi=60 * DEGREES, theta=-35 * DEGREES)
        self.add(axes, surface)
        self.begin_ambient_camera_rotation(rate=0.15)
        self.wait(0.3)


class SpecialThreeDSceneCompatibility(SpecialThreeDScene):
    def __init__(self, **kwargs):
        # Manim 0.20.1's public constructor reads self.renderer before calling
        # Scene.__init__. Initialize the same scene behavior around that upstream
        # ordering bug so its camera helpers remain covered.
        self.default_angled_camera_position = {
            "phi": 70 * DEGREES,
            "theta": -110 * DEGREES,
        }
        ThreeDScene.__init__(self, **kwargs)

    def construct(self):
        sphere = Sphere(radius=1, resolution=(8, 6))
        self.set_camera_to_default_position()
        self.add(sphere)
        self.wait(0.2)


class VectorSceneCompatibility(VectorScene):
    def construct(self):
        self.add_plane()
        vector = self.add_vector([2, 1], color=YELLOW)
        self.vector_to_coords(vector)
        self.wait(0.2)


class LinearTransformationSceneCompatibility(LinearTransformationScene):
    def construct(self):
        square = Square().set_fill(BLUE, opacity=0.35)
        self.add_transformable_mobject(square)
        self.apply_matrix([[1.0, 0.45], [0.2, 0.9]], run_time=0.3)


class ZoomedSceneCompatibility(ZoomedScene):
    def construct(self):
        dot = Dot(LEFT * 2)
        self.add(dot)
        self.activate_zooming(animate=False)
        self.zoomed_camera.frame.move_to(dot)
        self.zoomed_display.move_to(RIGHT * 3)
        self.play(dot.animate.shift(UP * 0.5), run_time=0.3)
