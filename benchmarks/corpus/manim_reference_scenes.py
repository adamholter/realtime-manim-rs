"""Golden ManimCE reference scenes for capability and visual conformance."""

from __future__ import annotations

import math
import os
import tempfile
import wave
from pathlib import Path

import numpy as np
from manim import *


FIXTURE_SVG = Path(__file__).with_name("fixture.svg")


class PrimitiveGallery(Scene):
    def construct(self):
        shapes = VGroup(
            Circle(color=BLUE),
            Square(color=GREEN),
            Triangle(color=YELLOW),
            RoundedRectangle(corner_radius=0.25, color=ORANGE),
            Arc(angle=1.6 * PI, color=PURPLE),
            Annulus(inner_radius=0.3, outer_radius=0.6, color=TEAL),
        ).scale(0.6).arrange(RIGHT, buff=0.45)
        details = VGroup(
            DashedLine(LEFT, RIGHT, color=GRAY),
            Arrow(LEFT, RIGHT, color=RED),
            Polygon(LEFT, UP, RIGHT, DOWN, color=PINK),
        ).scale(0.65).arrange(RIGHT, buff=0.8).next_to(shapes, DOWN)
        self.play(LaggedStart(*[Create(shape) for shape in shapes], lag_ratio=0.12))
        self.play(FadeIn(details, shift=UP * 0.2))


class TextAndMath(Scene):
    def construct(self):
        heading = Text("Vector typography — AV office", font_size=36)
        markup = MarkupText("<b>bold</b> · <i>italic</i> · <span foreground='#38bdf8'>color</span>", font_size=28)
        equation = MathTex(r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}")
        target = MathTex(r"\sqrt{\pi}=2\int_0^\infty e^{-x^2}\,dx")
        group = VGroup(heading, markup, equation).arrange(DOWN, buff=0.55)
        self.play(Write(heading), FadeIn(markup))
        self.play(Write(equation))
        self.play(TransformMatchingTex(equation, target))


class AxesPlotsAndArea(Scene):
    def construct(self):
        axes = Axes(x_range=[-3, 3, 1], y_range=[-1, 4, 1], x_length=8, y_length=5, tips=False)
        graph = axes.plot(lambda x: 0.45 * x * x + 0.2, color=BLUE)
        area = axes.get_area(graph, x_range=[-1.5, 1.5], color=BLUE, opacity=0.3)
        tracker = ValueTracker(-2.5)
        dot = always_redraw(lambda: Dot(axes.c2p(tracker.get_value(), 0.45 * tracker.get_value() ** 2 + 0.2), color=YELLOW))
        label = axes.get_graph_label(graph, MathTex("f(x)=0.45x^2+0.2"))
        self.play(Create(axes), Create(graph), FadeIn(area), FadeIn(label))
        self.add(dot)
        self.play(tracker.animate.set_value(2.5), run_time=2.5, rate_func=linear)


class ParametricAndPolar(Scene):
    def construct(self):
        plane = NumberPlane(x_range=[-4, 4], y_range=[-3, 3]).scale(0.8).shift(LEFT * 3.2)
        curve = ParametricFunction(
            lambda t: plane.c2p(2 * np.cos(3 * t), 2 * np.sin(2 * t)),
            t_range=[0, TAU],
            color=YELLOW,
        )
        polar = PolarPlane(radius_max=2.5).scale(0.85).shift(RIGHT * 3.2)
        rose = polar.plot_polar_graph(lambda theta: 2 * np.cos(4 * theta), [0, TAU], color=PINK)
        self.play(Create(plane), Create(polar))
        self.play(Create(curve), Create(rose), run_time=2)


class TransformFamily(Scene):
    def construct(self):
        square = Square(color=BLUE).shift(LEFT * 3)
        circle = Circle(color=GREEN).shift(RIGHT * 3)
        path = ArcBetweenPoints(LEFT * 3, RIGHT * 3, angle=PI / 2)
        self.play(Create(square))
        self.play(square.animate.shift(RIGHT).rotate(PI / 4).set_fill(BLUE, 0.4))
        self.play(TransformFromCopy(square, circle))
        self.play(MoveAlongPath(square, path), Rotate(circle, PI))
        self.play(ReplacementTransform(square, Triangle(color=YELLOW).shift(RIGHT * 3)))


class CreationAndIndication(Scene):
    def construct(self):
        items = VGroup(Square(), Circle(), Star(), Triangle()).arrange(RIGHT)
        self.play(DrawBorderThenFill(items[0]), GrowFromCenter(items[1]), SpiralIn(items[2]), Create(items[3]))
        self.play(Circumscribe(items[0]), Indicate(items[1]), Flash(items[2]), Wiggle(items[3]))
        self.play(Uncreate(items))


class AnimationComposition(Scene):
    def construct(self):
        dots = VGroup(*[Dot(color=interpolate_color(BLUE, YELLOW, i / 7)) for i in range(8)]).arrange(RIGHT)
        self.play(LaggedStart(*[FadeIn(dot, shift=UP) for dot in dots], lag_ratio=0.12))
        self.play(AnimationGroup(*[dot.animate.shift(UP * (0.4 + i * 0.08)) for i, dot in enumerate(dots)], lag_ratio=0.05))
        self.play(Succession(dots.animate.scale(1.6), dots.animate.scale(1 / 1.6)), rate_func=there_and_back)


class UpdatersAndTracedPath(Scene):
    def construct(self):
        tracker = ValueTracker(0)
        center = LEFT * 2
        dot = Dot(color=YELLOW)
        dot.add_updater(lambda mob: mob.move_to(center + 2 * np.array([np.cos(tracker.get_value()), np.sin(tracker.get_value()), 0])))
        number = DecimalNumber(0, num_decimal_places=2).to_corner(UR)
        number.add_updater(lambda mob: mob.set_value(tracker.get_value()))
        trace = TracedPath(dot.get_center, stroke_color=BLUE, dissipating_time=1.2)
        self.add(trace, dot, number)
        self.play(tracker.animate.set_value(TAU * 1.5), run_time=3, rate_func=linear)


class MovingCamera(MovingCameraScene):
    def construct(self):
        grid = NumberPlane()
        target = Star(7, outer_radius=1.2, color=YELLOW).shift(RIGHT * 3 + UP)
        self.add(grid, target)
        self.camera.frame.save_state()
        self.play(self.camera.frame.animate.scale(0.45).move_to(target))
        self.play(Restore(self.camera.frame))


class SvgAndImage(Scene):
    def construct(self):
        svg = SVGMobject(str(FIXTURE_SVG)).set_height(3).shift(LEFT * 3)
        yy, xx = np.mgrid[0:128, 0:128]
        pixels = np.zeros((128, 128, 4), dtype=np.uint8)
        pixels[..., 0] = (xx * 2).astype(np.uint8)
        pixels[..., 1] = (yy * 2).astype(np.uint8)
        pixels[..., 2] = 180
        pixels[..., 3] = np.where((xx - 64) ** 2 + (yy - 64) ** 2 < 58 ** 2, 255, 0)
        image = ImageMobject(pixels).set_height(3).shift(RIGHT * 3)
        image.set_resampling_algorithm(RESAMPLING_ALGORITHMS["bilinear"])
        self.play(FadeIn(svg), FadeIn(image))
        self.play(svg.animate.rotate(PI / 3), image.animate.scale(0.8))


class MatrixTableAndCode(Scene):
    def construct(self):
        matrix = Matrix([[1, 2], [3, 4]]).scale(0.7).shift(LEFT * 4)
        table = Table([["x", "x²"], ["2", "4"], ["3", "9"]], include_outer_lines=True).scale(0.45)
        code = Code(
            code_string="def f(x):\n    return x * x",
            language="python",
            background="window",
        ).scale(0.55).shift(RIGHT * 4)
        brace = Brace(matrix, DOWN)
        box = SurroundingRectangle(table, color=BLUE)
        self.play(Write(matrix), Create(table), FadeIn(code))
        self.play(GrowFromCenter(brace), Create(box))


class GraphNetwork(Scene):
    def construct(self):
        graph = Graph(
            [1, 2, 3, 4, 5],
            [(1, 2), (2, 3), (3, 4), (4, 5), (5, 1), (1, 3)],
            labels=True,
            layout="circular",
        )
        digraph = DiGraph(
            [1, 2, 3],
            [(1, 2), (2, 3), (3, 1)],
            labels=True,
            layout_config={"seed": 7},
        ).scale(0.8).shift(RIGHT * 4)
        graph.shift(LEFT * 2)
        self.play(Create(graph), Create(digraph))
        self.play(graph.animate.change_layout("spring", layout_config={"seed": 7}))


class VectorFieldAndStreamLines(Scene):
    def construct(self):
        func = lambda pos: np.array([-pos[1], pos[0], 0]) / 2
        field = ArrowVectorField(func, x_range=[-4, 4, 1], y_range=[-2, 2, 1], length_func=lambda n: 0.35)
        streams = StreamLines(func, x_range=[-4, 4, 0.7], y_range=[-2, 2, 0.7], virtual_time=1.5)
        self.play(Create(field))
        self.add(streams)
        streams.start_animation(warm_up=True, flow_speed=1.2)
        self.wait(2)
        streams.end_animation()


class ChartsAndProbability(Scene):
    def construct(self):
        chart = BarChart([2, 5, 3, 6], y_range=[0, 7, 1], y_length=4, x_length=6, bar_colors=[BLUE, TEAL, GREEN, YELLOW]).scale(0.8).shift(LEFT * 2)
        sample = SampleSpace(width=3, height=4).shift(RIGHT * 4)
        sample.divide_horizontally([0.25, 0.5], colors=[BLUE, GREEN, YELLOW])
        number = DecimalNumber(2).to_edge(UP)
        self.play(Create(chart), Create(sample), FadeIn(number))
        self.play(ChangeDecimalToValue(number, 6), chart.animate.change_bar_values([6, 3, 5, 2]))


class BooleanGeometry(Scene):
    def construct(self):
        left = Circle(radius=1.4).shift(LEFT * 0.7)
        right = Circle(radius=1.4).shift(RIGHT * 0.7)
        results = VGroup(
            Union(left, right, color=BLUE),
            Intersection(left, right, color=GREEN),
            Difference(left, right, color=YELLOW),
            Exclusion(left, right, color=PINK),
        ).scale(0.55).arrange(RIGHT, buff=0.8)
        self.play(LaggedStart(*[FadeIn(result) for result in results], lag_ratio=0.2))


class CustomVMobject(Scene):
    def construct(self):
        curve = VMobject(color=BLUE)
        points = [LEFT * 3, UP * 1.5 + LEFT, DOWN + RIGHT, RIGHT * 3]
        curve.set_points_smoothly(points)
        dot = Dot(color=YELLOW)
        dot.add_updater(lambda mob: mob.move_to(curve.point_from_proportion((self.renderer.time / 3) % 1)))
        self.play(Create(curve))
        self.add(dot)
        self.play(
            ApplyPointwiseFunction(
                lambda p: p + 0.35 * np.sin(p[0]) * UP,
                curve,
            ),
            run_time=2,
        )


class ThreeDSurface(ThreeDScene):
    def construct(self):
        axes = ThreeDAxes()
        surface = Surface(
            lambda u, v: axes.c2p(u, v, 0.6 * np.sin(u) * np.cos(v)),
            u_range=[-PI, PI],
            v_range=[-PI, PI],
            resolution=(24, 24),
        )
        surface.set_fill_by_checkerboard(BLUE_D, BLUE_E, opacity=0.75)
        self.set_camera_orientation(phi=65 * DEGREES, theta=-45 * DEGREES)
        self.play(Create(axes), FadeIn(surface))
        self.begin_ambient_camera_rotation(rate=0.25)
        self.wait(2)
        self.stop_ambient_camera_rotation()


class PolyhedraAndFixedLabels(ThreeDScene):
    def construct(self):
        cube = Cube(side_length=2, fill_opacity=0.55).shift(LEFT * 2)
        solid = Dodecahedron().set_fill(opacity=0.55).scale(1.2).shift(RIGHT * 2)
        fixed = Text("fixed in frame", font_size=28).to_corner(UL)
        oriented = Text("fixed orientation", font_size=24).move_to(solid.get_center() + UP * 1.8)
        self.set_camera_orientation(phi=70 * DEGREES, theta=-35 * DEGREES)
        self.add(cube, solid)
        self.add_fixed_in_frame_mobjects(fixed)
        self.add_fixed_orientation_mobjects(oriented)
        self.begin_ambient_camera_rotation(rate=0.35)
        self.wait(2)


class AudioTimeline(Scene):
    audio_path: str | None = None

    def construct(self):
        handle = tempfile.NamedTemporaryFile(suffix=".wav", delete=False)
        handle.close()
        self.audio_path = handle.name
        sample_rate = 22_050
        samples = (0.18 * np.sin(2 * PI * 440 * np.arange(sample_rate) / sample_rate) * 32767).astype(np.int16)
        with wave.open(self.audio_path, "wb") as output:
            output.setnchannels(1)
            output.setsampwidth(2)
            output.setframerate(sample_rate)
            output.writeframes(samples.tobytes())
        pulse = Circle(radius=0.8, color=BLUE)
        self.play(Create(pulse))
        self.add_sound(self.audio_path, time_offset=0.25)
        self.play(pulse.animate.scale(2), run_time=1)

    def tear_down(self):
        super().tear_down()
        if self.audio_path and os.path.exists(self.audio_path):
            os.unlink(self.audio_path)


class UnicodeColorAndAccessibility(Scene):
    def construct(self):
        lines = VGroup(
            Text("Café · naïve · A\u030a", font_size=34),
            Text("العَرَبِيَّة", font_size=38),
            Text("हिन्दी · 日本語 · 한글", font_size=34),
            Text("👩🏽‍💻  🌍  ∑  ∞", font_size=38),
        ).arrange(DOWN, aligned_edge=LEFT)
        lines.set_color_by_gradient(BLUE, PURPLE, PINK)
        self.add_subcaption("International text and grapheme clusters", duration=2)
        self.play(LaggedStart(*[Write(line) for line in lines], lag_ratio=0.2), run_time=2)
