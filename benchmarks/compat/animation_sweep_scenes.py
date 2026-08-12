"""Scene batches that execute every working public Manim animation class."""

from __future__ import annotations

import importlib
import json
from pathlib import Path
from typing import Any

from manim import Scene

from animation_fixtures import construct_animation_case


ROOT = Path(__file__).resolve().parents[2]
AUDIT = ROOT / "benchmarks" / "corpus" / "manim-animation-audit.json"
BATCH_SIZE = 8


def _entries() -> list[dict[str, Any]]:
    audit = json.loads(AUDIT.read_text())
    return [
        result
        for result in audit["results"]
        if result["status"] == "interpolated"
    ]


def _construct(scene: Scene, entries: list[dict[str, Any]]) -> None:
    for entry in entries:
        candidate = getattr(
            importlib.import_module(entry["module"]), entry["name"]
        )
        case = construct_animation_case(candidate)
        scene.add(*case.mobjects)
        scene.play(case.animation, run_time=0.12)
        scene.remove(*case.mobjects)


def _install_batches(entries: list[dict[str, Any]]) -> None:
    for start in range(0, len(entries), BATCH_SIZE):
        batch = entries[start : start + BATCH_SIZE]
        batch_index = start // BATCH_SIZE

        def construct(scene: Scene, selected: list[dict[str, Any]] = batch) -> None:
            _construct(scene, selected)

        class_name = f"AnimationSweep{batch_index:02d}"
        globals()[class_name] = type(
            class_name,
            (Scene,),
            {
                "__module__": __name__,
                "construct": construct,
                "animation_classes": tuple(entry["name"] for entry in batch),
            },
        )


ANIMATION_ENTRIES = _entries()
_install_batches(ANIMATION_ENTRIES)
