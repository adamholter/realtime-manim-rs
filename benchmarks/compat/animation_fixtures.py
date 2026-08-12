"""Deterministic fixtures for every public Manim animation class."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

import numpy as np
from manim import (
    Animation,
    Arrow,
    Circle,
    DecimalNumber,
    Dot,
    DOWN,
    FadeIn,
    LEFT,
    Line,
    MathTex,
    ORIGIN,
    RIGHT,
    Square,
    Text,
    UP,
    VGroup,
)


@dataclass
class AnimationCase:
    animation: Animation
    mobjects: list[Any]


class UpstreamAnimationBase(RuntimeError):
    """The public class is an intentionally incomplete base implementation."""


def _pair() -> tuple[Square, Circle]:
    return Square(side_length=1.2).shift(LEFT), Circle(radius=0.7).shift(RIGHT)


def construct_animation_case(candidate: type[Animation]) -> AnimationCase:
    name = candidate.__name__
    source, target = _pair()
    text = Text("Rust")
    decimal = DecimalNumber(1.0)
    path = Line(LEFT * 1.5, RIGHT * 1.5)

    if name == "Add":
        return AnimationCase(candidate(source), [source])
    if name in {"AddTextLetterByLetter", "RemoveTextLetterByLetter"}:
        return AnimationCase(candidate(text, run_time=0.2), [text])
    if name == "AddTextWordByWord":
        return AnimationCase(candidate(text, run_time=0.2), [text])
    if name == "Animation":
        return AnimationCase(candidate(source), [source])
    if name == "AnimationGroup":
        return AnimationCase(candidate(FadeIn(source), FadeIn(target)), [source, target])
    if name == "ApplyComplexFunction":
        return AnimationCase(candidate(lambda z: z + 0.2j, source), [source])
    if name == "ApplyFunction":
        return AnimationCase(candidate(lambda mob: mob.shift(UP * 0.2), source), [source])
    if name == "ApplyMatrix":
        return AnimationCase(candidate(np.array([[0.9, -0.2], [0.2, 0.9]]), source), [source])
    if name == "ApplyMethod":
        return AnimationCase(candidate(source.shift, RIGHT * 0.4), [source])
    if name == "ApplyPointwiseFunction":
        return AnimationCase(candidate(lambda point: point + [0.2 * point[1], 0, 0], source), [source])
    if name == "ApplyPointwiseFunctionToCenter":
        raise UpstreamAnimationBase(
            "Manim 0.20.1 omits the mobject argument in this constructor's super() call."
        )
    if name in {"ApplyWave", "Blink", "Broadcast", "Circumscribe", "Indicate", "Wiggle"}:
        return AnimationCase(candidate(source), [source])
    if name == "ChangeDecimalToValue":
        return AnimationCase(candidate(decimal, 4), [decimal])
    if name == "ChangeSpeed":
        return AnimationCase(candidate(FadeIn(source), {0.0: 1.0, 1.0: 1.0}), [source])
    if name == "ChangingDecimal":
        return AnimationCase(candidate(decimal, lambda alpha: 1 + 3 * alpha), [decimal])
    if name in {"ClockwiseTransform", "CounterclockwiseTransform", "ReplacementTransform", "Transform", "TransformFromCopy"}:
        return AnimationCase(candidate(source, target), [source])
    if name == "ComplexHomotopy":
        return AnimationCase(candidate(lambda z, t: z + 0.2j * t, source), [source])
    if name in {"Create", "DrawBorderThenFill", "ShowPassingFlash", "Uncreate", "Unwrite", "Write"}:
        return AnimationCase(candidate(source), [source])
    if name in {"CyclicReplace", "Swap"}:
        return AnimationCase(candidate(source, target), [source, target])
    if name in {"FadeIn", "FadeOut"}:
        return AnimationCase(candidate(source), [source])
    if name == "FadeToColor":
        return AnimationCase(candidate(source, "#ff8844"), [source])
    if name in {"FadeTransform", "FadeTransformPieces"}:
        return AnimationCase(candidate(source, target), [source])
    if name == "Flash":
        return AnimationCase(candidate(ORIGIN), [])
    if name == "FocusOn":
        return AnimationCase(candidate(source), [source])
    if name == "GrowArrow":
        arrow = Arrow(LEFT, RIGHT)
        return AnimationCase(candidate(arrow), [arrow])
    if name in {"GrowFromCenter", "ShrinkToCenter", "SpinInFromNothing"}:
        return AnimationCase(candidate(source), [source])
    if name == "GrowFromEdge":
        return AnimationCase(candidate(source, LEFT), [source])
    if name == "GrowFromPoint":
        return AnimationCase(candidate(source, DOWN), [source])
    if name == "SmoothedVectorizedHomotopy":
        raise UpstreamAnimationBase(
            "Manim 0.20.1 references VMobject without importing it in this animation module."
        )
    if name == "Homotopy":
        return AnimationCase(
            candidate(lambda x, y, z, t: (x, y + 0.2 * np.sin(x + t), z), source),
            [source],
        )
    if name == "LaggedStart":
        return AnimationCase(candidate(FadeIn(source), FadeIn(target)), [source, target])
    if name == "LaggedStartMap":
        group = VGroup(source, target)
        return AnimationCase(candidate(FadeIn, group), [group])
    if name == "MaintainPositionRelativeTo":
        tracker = Dot(RIGHT)
        return AnimationCase(candidate(source, tracker), [source, tracker])
    if name == "MoveAlongPath":
        return AnimationCase(candidate(source, path), [source, path])
    if name == "MoveToTarget":
        source.generate_target()
        source.target.shift(RIGHT)
        return AnimationCase(candidate(source), [source])
    if name == "PhaseFlow":
        return AnimationCase(candidate(lambda point: np.array([-point[1], point[0], 0]), source), [source])
    if name == "Restore":
        source.save_state()
        source.shift(RIGHT)
        return AnimationCase(candidate(source), [source])
    if name in {"Rotate", "Rotating"}:
        return AnimationCase(candidate(source, angle=np.pi / 2), [source])
    if name == "ScaleInPlace":
        return AnimationCase(candidate(source, 1.4), [source])
    if name == "ShowIncreasingSubsets":
        group = VGroup(Dot(LEFT), Dot(), Dot(RIGHT))
        return AnimationCase(candidate(group), [group])
    if name == "ShowPartial":
        raise UpstreamAnimationBase("ShowPartial requires a subclass implementation.")
    if name == "ShowPassingFlashWithThinningStrokeWidth":
        return AnimationCase(candidate(Circle()), [])
    if name == "ShowSubmobjectsOneByOne":
        group = VGroup(Dot(LEFT), Dot(), Dot(RIGHT))
        return AnimationCase(candidate(group), [group])
    if name == "SpiralIn":
        group = VGroup(source, target)
        return AnimationCase(candidate(group), [group])
    if name == "Succession":
        return AnimationCase(candidate(FadeIn(source), FadeIn(target)), [source, target])
    if name == "TransformAnimations":
        raise UpstreamAnimationBase(
            "Manim 0.20.1 fails while aligning this class's rewired animation families."
        )
    if name == "TransformMatchingShapes":
        first, second = Text("abc"), Text("cab")
        return AnimationCase(candidate(first, second), [first])
    if name == "TransformMatchingTex":
        first, second = MathTex("x^2"), MathTex("x^3")
        return AnimationCase(candidate(first, second), [first])
    if name == "TypeWithCursor":
        cursor = Line(DOWN * 0.2, UP * 0.2)
        return AnimationCase(candidate(text, cursor, time_per_char=0.05), [text, cursor])
    if name == "UntypeWithCursor":
        cursor = Line(DOWN * 0.2, UP * 0.2)
        return AnimationCase(candidate(text, cursor, time_per_char=0.05), [text, cursor])
    if name == "UpdateFromAlphaFunc":
        return AnimationCase(candidate(source, lambda mob, alpha: mob.set_opacity(alpha)), [source])
    if name == "UpdateFromFunc":
        return AnimationCase(candidate(source, lambda mob: mob.shift(RIGHT * 0.01)), [source])
    if name == "Wait":
        return AnimationCase(candidate(run_time=0.1), [])
    raise KeyError(f"No animation fixture for {name}.")
