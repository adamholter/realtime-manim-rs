"""Adversarial retained-correspondence fixtures for ManimCE 0.20.1."""

from manim import *


class TransformMatchingTexSemantic(Scene):
    def construct(self):
        source = MathTex("x", "+", "x", "+", "y", "=", "z")
        target = MathTex("z", "=", "x", "+", "y", "+", "x")
        self.add(source)
        self.play(TransformMatchingTex(source, target), run_time=1)


class TransformMatchingTexKeyMap(Scene):
    def construct(self):
        source = MathTex("a", "+", "b")
        target = MathTex("c", "-", "b")
        self.add(source)
        self.play(
            TransformMatchingTex(
                source,
                target,
                key_map={"a": "c"},
                fade_transform_mismatches=True,
            ),
            run_time=1,
        )


class TransformMatchingShapesSemantic(Scene):
    def construct(self):
        source = Text("abba")
        target = Text("baba")
        self.add(source)
        self.play(TransformMatchingShapes(source, target), run_time=1)


class TransformMatchingArcFallback(Scene):
    def construct(self):
        source = MathTex("x", "+", "y")
        target = MathTex("y", "+", "x")
        self.add(source)
        self.play(
            TransformMatchingTex(source, target, path_arc=PI / 2),
            run_time=1,
        )


class NestedTransformMatchingTexSemantic(Scene):
    def construct(self):
        source = MathTex("a", "+", "b")
        target = MathTex("b", "+", "a")
        self.add(source)
        self.play(
            Succession(
                Wait(0.25),
                TransformMatchingTex(source, target),
            ),
            run_time=1.25,
        )


class NestedCompositeRateFallback(Scene):
    def construct(self):
        source = MathTex("p", "+", "q")
        target = MathTex("q", "+", "p")
        self.add(source)
        self.play(
            AnimationGroup(
                TransformMatchingTex(source, target),
                rate_func=smooth,
            ),
            run_time=1,
        )


class OneSidedMatchingMismatches(Scene):
    def construct(self):
        source = MathTex("a")
        target = MathTex("a", "+", "c")
        self.add(source)
        self.play(
            TransformMatchingTex(
                source,
                target,
                transform_mismatches=True,
            ),
            run_time=1,
        )


class OneSidedMatchingFade(Scene):
    def construct(self):
        source = MathTex("a", "+", "b")
        target = MathTex("a")
        self.add(source)
        self.play(TransformMatchingTex(source, target), run_time=1)
