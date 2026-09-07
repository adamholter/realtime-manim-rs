#!/usr/bin/env python3
"""Prove that Manim QuickHull compilation ignores process entropy."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import math
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

import numpy as np
from manim import ConvexHull3D, Scene
from manim.utils.qhull import QuickHull
from scipy.spatial import ConvexHull


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


class SmallInputConvexHull3D(Scene):
    def construct(self) -> None:
        points = np.array(
            [[-1, -1, -1], [1, -1, -1], [0, 1, -1], [0, 0, 1]],
            dtype=float,
        )
        self.add(ConvexHull3D(*(points * 1e-6)).scale(1e6, about_point=np.zeros(3)))
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


def check_hull_geometry() -> int:
    """Compare hull volume and supporting planes with independent Qhull output."""
    load_compiler()
    clouds = [
        np.array([[-1, -1, -1], [1, -1, -1], [0, 1, -1], [0, 0, 1]], dtype=float),
        # Seed zero chooses the four coplanar base vertices first.
        np.array(
            [[0, 0, 1], [-1, -1, 0], [1, -1, 0], [1, 1, 0], [-1, 1, 0]],
            dtype=float,
        ),
        # Repeated and interior points must not add spurious hull facets.
        np.array([[0, 0], [0, 0], [-1, -1], [1, -1], [1, 1], [-1, 1]], dtype=float),
    ]
    checks = 0
    for cloud in clouds:
        dimension = cloud.shape[1]
        reference_volume = ConvexHull(cloud).volume
        for scale in (1e-6, 1.0, 1e6):
            # Keep the outside classification tolerance proportional to scale,
            # then also exercise the default tolerance on a tiny tetrahedron.
            tolerances = [1e-5 * scale]
            if len(cloud) == 4 and scale == 1e-6:
                tolerances.append(1e-5)
            for tolerance in tolerances:
                hull = QuickHull(tolerance=tolerance)
                hull.build(cloud * scale)
                facets = [
                    facet for facet in hull.facets if facet not in hull.removed
                ]
                center = np.mean(cloud, axis=0)
                volume = 0.0
                vertices = set()
                for facet in facets:
                    coordinates = facet.coordinates / scale
                    assert np.isfinite(facet.normal).all()
                    assert np.max((cloud - coordinates[0]) @ facet.normal) <= 1e-9
                    volume += abs(np.linalg.det(coordinates - center)) / math.factorial(
                        dimension
                    )
                    vertices.update(map(tuple, coordinates))
                expected_vertices = set(map(tuple, cloud[ConvexHull(cloud).vertices]))
                assert vertices == expected_vertices, (
                    scale, vertices, expected_vertices
                )
                assert np.isclose(volume, reference_volume, rtol=1e-10), (
                    scale, volume, reference_volume
                )
                checks += 1
    for cloud in (np.zeros((4, 3)), np.array([[0, 0], [1, 0], [2, 0]], dtype=float)):
        try:
            QuickHull().build(cloud)
        except ValueError as error:
            assert "full-dimensional simplex" in str(error)
        else:
            raise AssertionError("Rank-deficient input should fail explicitly")
    return checks


def main() -> int:
    if len(sys.argv) == 4 and sys.argv[1] == "--worker":
        compile_worker(int(sys.argv[2]), sys.argv[3])
        return 0

    geometry_checks = check_hull_geometry()
    environment = os.environ.copy()
    environment["PYTHONHASHSEED"] = "0"
    results = {}
    expected_nodes = {
        "ConvexHull3DDeterminism": 16,
        "DegenerateSeedConvexHull3D": 24,
        # Matches upstream Manim with seed zero for the same small input.
        "SmallInputConvexHull3D": 17,
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
                timeout=60,
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
    print(json.dumps(
        {"ok": True, "geometryChecks": geometry_checks, "runs": results}, indent=2
    ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
