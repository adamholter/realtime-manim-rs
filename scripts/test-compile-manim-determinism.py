#!/usr/bin/env python3
"""Prove that Manim QuickHull compilation ignores process entropy."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

import numpy as np
from manim import ConvexHull3D, Scene


ROOT = Path(__file__).resolve().parents[1]
COMPILER = ROOT / "scripts" / "compile-manim.py"
CONTROLLED_ENTROPY_SEEDS = (0, 1, 2, 4)


class ConvexHull3DDeterminism(Scene):
    def construct(self) -> None:
        self.add(
            ConvexHull3D(
                [-1, -1, -1],
                [1, -1, -1],
                [0, 1, -1],
                [0, 0, 1],
            )
        )
        self.wait(1 / 30)


class DegenerateSeedConvexHull3D(Scene):
    def construct(self) -> None:
        self.add(
            ConvexHull3D(
                [0, 0, 1],
                [-1, -1, 0],
                [1, -1, 0],
                [1, 1, 0],
                [-1, 1, 0],
            )
        )
        self.wait(1 / 30)


def load_compiler() -> Any:
    spec = importlib.util.spec_from_file_location(
        "_determinism_compiler", COMPILER
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Could not import {COMPILER}.")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def compile_worker(entropy_seed: int, scene_class: str) -> None:
    compiler = load_compiler()
    original_default_rng = np.random.default_rng
    np.random.default_rng = lambda *args, **kwargs: original_default_rng(
        entropy_seed
    )
    try:
        scene, receipt = compiler.compile_scene(
            Path(__file__),
            scene_class,
            30,
            "cairo",
        )
    finally:
        np.random.default_rng = original_default_rng
    raw = json.dumps(scene, separators=(",", ":")).encode()
    print(
        json.dumps(
            {
                "entropySeed": entropy_seed,
                "scene": scene_class,
                "sha256": hashlib.sha256(raw).hexdigest(),
                "nodes": len(scene["nodes"]),
                "diagnostics": receipt["diagnostics"],
            }
        )
    )


def main() -> int:
    if len(sys.argv) == 4 and sys.argv[1] == "--worker":
        compile_worker(int(sys.argv[2]), sys.argv[3])
        return 0

    environment = os.environ.copy()
    environment["PYTHONHASHSEED"] = "0"
    results = {}
    expected_nodes = {
        "ConvexHull3DDeterminism": 16,
        "DegenerateSeedConvexHull3D": 24,
    }
    for scene_class, expected_node_count in expected_nodes.items():
        rows = []
        for entropy_seed in CONTROLLED_ENTROPY_SEEDS:
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--worker",
                    str(entropy_seed),
                    scene_class,
                ],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=True,
            )
            rows.append(json.loads(result.stdout.strip().splitlines()[-1]))

        hashes = {row["sha256"] for row in rows}
        node_counts = {row["nodes"] for row in rows}
        diagnostics = [
            item for row in rows for item in row["diagnostics"]
        ]
        assert len(hashes) == 1, (
            f"QuickHull output changed with entropy: {rows}"
        )
        assert node_counts == {expected_node_count}, (
            f"ConvexHull3D fidelity changed: {rows}"
        )
        assert not diagnostics, diagnostics
        results[scene_class] = rows
    print(json.dumps({"ok": True, "runs": results}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
