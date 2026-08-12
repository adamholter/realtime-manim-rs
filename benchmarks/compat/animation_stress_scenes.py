from manim import *


class MatchingAndNumbersCompatibility(Scene):
    def construct(self):
        equation = MathTex("a^2", "+", "b^2", "=", "c^2").scale(1.3)
        value = DecimalNumber(0, num_decimal_places=2).next_to(equation, DOWN)
        self.add(equation, value)
        target = MathTex("c^2", "-", "b^2", "=", "a^2").scale(1.3)
        self.play(
            TransformMatchingTex(equation, target),
            ChangeDecimalToValue(value, 3.75),
            run_time=1,
        )
        self.play(Circumscribe(target, color=YELLOW), run_time=0.7)


class DeformationAndMotionCompatibility(Scene):
    def construct(self):
        path = ParametricFunction(
            lambda t: [3 * np.cos(t), 1.3 * np.sin(2 * t), 0],
            t_range=[0, TAU],
            color=BLUE,
        )
        dot = Dot(path.get_start(), color=YELLOW)
        square = Square(1.4, color=TEAL).shift(UP * 1.8)
        self.add(path, dot, square)
        self.play(
            MoveAlongPath(dot, path),
            Homotopy(
                lambda x, y, z, t: [
                    x + 0.35 * np.sin(y * 2 + t * TAU),
                    y + 0.2 * np.sin(x * 2 - t * TAU),
                    z,
                ],
                square,
            ),
            run_time=1.2,
        )


class EffectsAndCompositionCompatibility(Scene):
    def construct(self):
        shapes = VGroup(
            Circle(color=RED),
            Square(color=GREEN),
            Triangle(color=BLUE),
        ).arrange(RIGHT, buff=0.8)
        self.play(
            LaggedStart(
                *(DrawBorderThenFill(shape) for shape in shapes),
                lag_ratio=0.2,
            ),
            run_time=1,
        )
        self.play(
            AnimationGroup(
                Wiggle(shapes[0]),
                Indicate(shapes[1]),
                Rotate(shapes[2], angle=PI),
                lag_ratio=0,
            ),
            run_time=0.8,
        )
        self.play(Flash(shapes[1].get_center()), run_time=0.6)


class MatrixAndFunctionsCompatibility(Scene):
    def construct(self):
        grid = VGroup(
            *[
                Line([x, -2, 0], [x, 2, 0], stroke_opacity=0.45)
                for x in np.linspace(-3, 3, 7)
            ],
            *[
                Line([-3, y, 0], [3, y, 0], stroke_opacity=0.45)
                for y in np.linspace(-2, 2, 5)
            ],
        )
        self.add(grid)
        self.play(ApplyMatrix([[1, 0.55], [-0.25, 1]], grid), run_time=0.8)
        self.play(
            ApplyPointwiseFunction(
                lambda point: [
                    point[0],
                    point[1] + 0.18 * np.sin(point[0] * 2),
                    point[2],
                ],
                grid,
            ),
            run_time=0.8,
        )
