from manim import *


class SingleFixedOrientationCompatibility(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=68 * DEGREES, theta=-48 * DEGREES)
        marker = Triangle(color=YELLOW).scale(0.62)
        marker.set_fill(BLUE, opacity=0.72)
        marker.move_to([1.7, 0.9, 1.15])
        self.add_fixed_orientation_mobjects(marker)
        self.play(
            marker.animate.scale(
                1.55, about_point=marker.get_center()
            ).rotate(PI / 3, about_point=marker.get_center()),
            self.camera.theta_tracker.animate.set_value(38 * DEGREES),
            self.camera.phi_tracker.animate.set_value(42 * DEGREES),
            run_time=1.2,
        )


class SingleStaticFixedOrientationCompatibility(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=68 * DEGREES, theta=-48 * DEGREES)
        marker = Circle(radius=0.48, color=YELLOW)
        marker.set_fill(BLUE, opacity=0.72)
        marker.move_to([1.7, 0.9, 1.15])
        self.add_fixed_orientation_mobjects(marker)
        self.play(
            self.camera.theta_tracker.animate.set_value(38 * DEGREES),
            self.camera.phi_tracker.animate.set_value(42 * DEGREES),
            run_time=1.2,
        )


class TopologyChangingFixedOrientationCompatibility(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=68 * DEGREES, theta=-48 * DEGREES)
        center = np.array([1.7, 0.9, 1.15])
        marker = RegularPolygon(3, color=YELLOW).scale(0.62).move_to(center)
        marker.set_fill(BLUE, opacity=0.72)
        self.add_fixed_orientation_mobjects(marker)

        def replace_topology(mobject, alpha):
            sides = 3 if alpha < 1 / 3 else 5 if alpha < 2 / 3 else 8
            replacement = RegularPolygon(sides).scale(0.62 + 0.18 * alpha)
            replacement.rotate(alpha * PI / 2).move_to(center)
            mobject.set_points(replacement.get_points())

        self.play(
            UpdateFromAlphaFunc(marker, replace_topology),
            self.camera.theta_tracker.animate.set_value(38 * DEGREES),
            self.camera.phi_tracker.animate.set_value(42 * DEGREES),
            run_time=1.2,
        )
