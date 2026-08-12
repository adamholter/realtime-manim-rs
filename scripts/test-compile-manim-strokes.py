#!/usr/bin/env python3
"""Focused retained-stroke compatibility check for compile-manim.py.

Run with the pinned reference environment:
    .venv-manim-reference/bin/python scripts/test-compile-manim-strokes.py
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
COMPILER = ROOT / "scripts" / "compile-manim.py"
SOURCE = """
from manim import *


class StrokeCompatibility(Scene):
    def construct(self):
        automatic = Line(LEFT * 3, RIGHT * 3, color=RED)
        rounded = Line(
            LEFT * 3,
            RIGHT * 3,
            color=GREEN,
            cap_style=CapStyleType.ROUND,
            joint_type=LineJointType.ROUND,
        ).shift(UP)
        square_bevel = VMobject(
            color=BLUE,
            cap_style=CapStyleType.SQUARE,
            joint_type=LineJointType.BEVEL,
        ).set_points_as_corners([LEFT * 2, UP, RIGHT * 2]).shift(DOWN)
        explicit_dash = Line(LEFT * 3, RIGHT * 3, color=YELLOW).shift(DOWN * 2)
        explicit_dash.dash_array = [0.4, 0.2, 0.1]
        explicit_dash.dash_offset = -0.15
        canonical_dash = DashedLine(
            LEFT * 3,
            RIGHT * 3,
            color=PURPLE,
            dash_length=0.5,
            dashed_ratio=0.4,
        ).shift(UP * 2)
        changing = Line(LEFT, RIGHT, color=ORANGE).shift(UP * 3)
        self.add(
            automatic,
            rounded,
            square_bevel,
            explicit_dash,
            canonical_dash,
            changing,
        )
        self.wait(0.25)
        changing.set_cap_style(CapStyleType.ROUND)
        self.wait(0.25)
"""


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="realtime-manim-strokes-") as temp:
        temp_path = Path(temp)
        source = temp_path / "stroke_scene.py"
        output = temp_path / "stroke_scene.json"
        source.write_text(SOURCE)
        subprocess.run(
            [
                sys.executable,
                str(COMPILER),
                str(source),
                "StrokeCompatibility",
                "--fps",
                "8",
                "--output",
                str(output),
            ],
            cwd=ROOT,
            check=True,
        )
        scene = json.loads(output.read_text())

    paths = [
        node
        for node in scene["nodes"]
        if node["type"] in {"path", "pathRef"}
    ]
    styles = [node["style"] for node in paths]
    assert any(
        style["strokeCap"] == "butt"
        and style["strokeJoin"] == "miter"
        for style in styles
    )
    assert any(
        style["strokeCap"] == "round"
        and style["strokeJoin"] == "round"
        for style in styles
    )
    assert any(
        style["strokeCap"] == "square"
        and style["strokeJoin"] == "bevel"
        for style in styles
    )
    dashed = [style for style in styles if style["dashArray"]]
    assert len(dashed) == 1
    assert dashed[0]["dashArray"] == [0.45, 0.225, 0.1125]
    assert dashed[0]["dashOffset"] == -0.16875

    # ManimCE implements DashedLine/DashedVMobject as child geometry. Those
    # children must remain undashed in native style to avoid double dashing.
    assert sum(style["dashArray"] == [] for style in styles) > 6

    # Static retained enum fields cannot be keyframed. A changed Manim cap is
    # represented as consecutive lifetimes so the transition remains exact.
    orange = [
        node
        for node in paths
        if node["style"]["stroke"].lower().startswith("#ff862f")
    ]
    assert len(orange) == 2
    assert [node["style"]["strokeCap"] for node in orange] == [
        "butt",
        "round",
    ]
    print(
        json.dumps(
            {
                "ok": True,
                "paths": len(paths),
                "nativeDashStyles": len(dashed),
                "dynamicCapLifetimes": len(orange),
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
