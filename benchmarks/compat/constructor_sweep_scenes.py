"""Generated scene classes that render every constructible public mobject."""

from __future__ import annotations

import importlib
import json
from pathlib import Path
from typing import Any

from manim import Scene

from constructor_fixtures import construct_fixture


ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "benchmarks" / "corpus" / "manim-constructor-audit.json"
BATCH_SIZE = 12
OPENGL_CATEGORIES = {
    "opengl-vector",
    "opengl-points",
    "opengl-surface",
    "opengl-custom",
}


def _renderable_entries(opengl: bool) -> list[dict[str, Any]]:
    audit = json.loads(AUDIT.read_text())
    return [
        result
        for result in audit["results"]
        if result["status"] == "constructed"
        and result["pointCount"] > 0
        and result.get("visualIntent", True)
        and ((result["category"] in OPENGL_CATEGORIES) is opengl)
    ]


def _construct(scene: Scene, entries: list[dict[str, Any]]) -> None:
    columns = 4
    cell_width = 3.35
    cell_height = 2.3
    for index, entry in enumerate(entries):
        candidate = getattr(
            importlib.import_module(entry["module"]), entry["name"]
        )
        mobject = construct_fixture(candidate)
        width = float(mobject.get_width())
        height = float(mobject.get_height())
        scale = min(
            1.0,
            2.7 / max(width, 1e-6),
            1.65 / max(height, 1e-6),
        )
        mobject.scale(scale)
        column = index % columns
        row = index // columns
        mobject.move_to(
            [
                (column - 1.5) * cell_width,
                (1.0 - row) * cell_height,
                0,
            ]
        )
        scene.add(mobject)
    scene.wait(1 / 30)


def _install_batches(prefix: str, entries: list[dict[str, Any]]) -> None:
    for start in range(0, len(entries), BATCH_SIZE):
        batch = entries[start : start + BATCH_SIZE]
        batch_index = start // BATCH_SIZE

        def construct(scene: Scene, selected: list[dict[str, Any]] = batch) -> None:
            _construct(scene, selected)

        class_name = f"{prefix}{batch_index:02d}"
        globals()[class_name] = type(
            class_name,
            (Scene,),
            {
                "__module__": __name__,
                "construct": construct,
                "constructor_classes": tuple(entry["name"] for entry in batch),
            },
        )


CAIRO_ENTRIES = _renderable_entries(False)
OPENGL_ENTRIES = _renderable_entries(True)
_install_batches("CairoConstructorSweep", CAIRO_ENTRIES)
_install_batches("OpenGLConstructorSweep", OPENGL_ENTRIES)
