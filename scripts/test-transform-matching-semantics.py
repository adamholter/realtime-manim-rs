#!/usr/bin/env python3
"""Prove compact TransformMatchingTex/Shapes correspondence retention."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
COMPILER = ROOT / "scripts" / "compile-manim.py"
SOURCE = ROOT / "benchmarks" / "compat" / "transform_matching_semantic_scenes.py"


def compile_scene(
    class_name: str,
    directory: Path,
    renderer: str = "cairo",
    *,
    semantic_matching: bool = True,
) -> tuple[dict, dict]:
    mode = "compact" if semantic_matching else "sampled"
    output = directory / f"{class_name}-{renderer}-{mode}.json"
    environment = os.environ.copy()
    environment.setdefault("TEXMFROOT", "/opt/homebrew/opt/texlive/share")
    environment.setdefault(
        "TEXMFCNF", "/opt/homebrew/opt/texlive/share/texmf-dist/web2c"
    )
    environment["REALTIME_MANIM_MEDIA_DIR"] = str(directory / "media")
    command = [
        sys.executable,
        str(COMPILER),
        str(SOURCE),
        class_name,
        "--renderer",
        renderer,
        "--fps",
        "30",
        "--output",
        str(output),
    ]
    if not semantic_matching:
        command.append("--disable-semantic-matching")
    subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    )
    return (
        json.loads(output.read_text()),
        json.loads(output.with_suffix(".receipt.json").read_text()),
    )


def main() -> int:
    with tempfile.TemporaryDirectory(
        prefix="realtime-manim-matching-"
    ) as temporary:
        directory = Path(temporary)
        tex_scene, tex_receipt = compile_scene(
            "TransformMatchingTexSemantic", directory
        )
        repeat_directory = directory / "repeat"
        repeat_directory.mkdir()
        repeated_scene, repeated_receipt = compile_scene(
            "TransformMatchingTexSemantic", repeat_directory
        )
        assert repeated_scene == tex_scene
        assert repeated_receipt == tex_receipt
        sampled_scene, sampled_receipt = compile_scene(
            "TransformMatchingTexSemantic",
            directory,
            semantic_matching=False,
        )
        compact_bytes = len(
            json.dumps(tex_scene, separators=(",", ":")).encode()
        )
        sampled_bytes = len(
            json.dumps(sampled_scene, separators=(",", ":")).encode()
        )
        assert compact_bytes < sampled_bytes
        assert sampled_scene["correspondences"] == tex_scene["correspondences"]
        assert sampled_receipt["semanticMatchingPathTracks"] == 0
        opengl_directory = directory / "opengl"
        opengl_directory.mkdir()
        _, opengl_receipt = compile_scene(
            "TransformMatchingTexSemantic",
            opengl_directory,
            renderer="opengl",
        )
        assert opengl_receipt["semanticMatchingCorrespondences"] == 5
        assert opengl_receipt["semanticMatchingPathTracks"] == 7
        assert opengl_receipt["semanticMatchingFallbacks"] == []
        correspondences = tex_scene["correspondences"]
        assert tex_receipt["semanticMatchingCorrespondences"] == 5
        assert tex_receipt["semanticMatchingPathTracks"] == 7
        assert tex_receipt["semanticMatchingFallbacks"] == []
        assert {item["kind"] for item in correspondences} == {
            "transformMatchingTex"
        }
        by_key = {
            item["keys"][0]: item
            for item in correspondences
            if item["mode"] == "transform"
        }
        assert set(by_key) == {"+", "=", "x", "y", "z"}
        assert len(by_key["x"]["sourceNodes"]) == 2
        assert len(by_key["x"]["targetNodes"]) == 2
        semantic_tracks = [
            track
            for track in tex_scene["tracks"]
            if track["property"] in {"pathData", "x", "y", "scaleX", "scaleY"}
            and len(track.get("keyframes", [])) == 2
            and track["keyframes"][-1].get("easing") == "manimSmooth"
        ]
        assert len(semantic_tracks) == 6

        key_scene, key_receipt = compile_scene(
            "TransformMatchingTexKeyMap", directory
        )
        modes = {item["mode"] for item in key_scene["correspondences"]}
        assert "keyMapped" in modes
        assert "fadeTransformMismatches" in modes
        assert key_receipt["semanticMatchingCorrespondences"] == 3
        assert key_receipt["semanticMatchingFallbacks"] == []

        shapes_scene, shapes_receipt = compile_scene(
            "TransformMatchingShapesSemantic", directory
        )
        shape_correspondences = shapes_scene["correspondences"]
        assert shapes_receipt["semanticMatchingCorrespondences"] > 0
        assert all(
            item["kind"] == "transformMatchingShapes"
            for item in shape_correspondences
        )
        assert all(
            key.startswith("shape:")
            for item in shape_correspondences
            for key in item["keys"] + item["targetKeys"]
        )
        assert all(
            item["keys"] == item["targetKeys"]
            for item in shape_correspondences
            if item["mode"] == "transform"
        )
        assert shapes_receipt["semanticMatchingFallbacks"] == []

        arc_scene, arc_receipt = compile_scene(
            "TransformMatchingArcFallback", directory
        )
        assert arc_scene["correspondences"]
        assert "nonlinear-path" in arc_receipt["semanticMatchingFallbacks"]
        assert arc_receipt["semanticMatchingPathTracks"] == 0
        assert all(
            abs(item["pathArc"] - 1.570796) < 0.000001
            for item in arc_scene["correspondences"]
        )

        nested_scene, nested_receipt = compile_scene(
            "NestedTransformMatchingTexSemantic", directory
        )
        assert nested_scene["correspondences"]
        assert all(
            abs(item["start"] - 0.25) < 0.000001
            and abs(item["end"] - 1.25) < 0.000001
            for item in nested_scene["correspondences"]
        )
        assert nested_receipt["semanticMatchingFallbacks"] == []
        assert nested_receipt["semanticMatchingPathTracks"] == 3

        composite_scene, composite_receipt = compile_scene(
            "NestedCompositeRateFallback", directory
        )
        assert composite_scene["correspondences"] == []
        assert composite_receipt["semanticMatchingPathTracks"] == 0
        assert composite_receipt["semanticMatchingFallbacks"] == [
            "nested-composite-rate-function"
        ]

        one_sided_transform, _ = compile_scene(
            "OneSidedMatchingMismatches", directory
        )
        transform_mismatch = next(
            item
            for item in one_sided_transform["correspondences"]
            if item["mode"] == "transformMismatches"
        )
        assert transform_mismatch["sourceNodes"] == []
        assert transform_mismatch["targetNodes"]

        one_sided_fade, _ = compile_scene(
            "OneSidedMatchingFade", directory
        )
        fade_modes = [
            item["mode"] for item in one_sided_fade["correspondences"]
        ]
        assert "fadeOut" in fade_modes
        assert "fadeIn" not in fade_modes

    print(
        json.dumps(
            {
                "ok": True,
                "texCorrespondences": len(correspondences),
                "texSemanticTracks": len(semantic_tracks),
                "compactBytes": compact_bytes,
                "sampledBytes": sampled_bytes,
                "byteReductionPercent": round(
                    (sampled_bytes - compact_bytes) * 100 / sampled_bytes, 2
                ),
                "deterministicReplay": True,
                "openglSemanticTracks": opengl_receipt[
                    "semanticMatchingPathTracks"
                ],
                "shapeCorrespondences": len(shape_correspondences),
                "arcFallbacks": arc_receipt["semanticMatchingFallbacks"],
                "nestedStart": nested_scene["correspondences"][0]["start"],
                "compositeFallbacks": composite_receipt[
                    "semanticMatchingFallbacks"
                ],
                "oneSidedTransformTargetNodes": len(
                    transform_mismatch["targetNodes"]
                ),
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
