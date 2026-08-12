#!/usr/bin/env python3
"""Construct and interpolate every public Manim animation class."""

from __future__ import annotations

import importlib
import json
import sys
from pathlib import Path
from typing import Any

from manim import Scene


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "benchmarks" / "corpus" / "manim-api-inventory.json"
OUTPUT = ROOT / "benchmarks" / "corpus" / "manim-animation-audit.json"
sys.path.insert(0, str(ROOT / "benchmarks" / "compat"))

from animation_fixtures import (  # pylint: disable=wrong-import-position
    UpstreamAnimationBase,
    construct_animation_case,
)


def main() -> int:
    inventory = json.loads(INVENTORY.read_text())
    results: list[dict[str, Any]] = []
    for entry in inventory["classes"]:
        if entry["category"] != "animation":
            continue
        result = {"name": entry["name"], "module": entry["module"]}
        try:
            candidate = getattr(
                importlib.import_module(entry["module"]), entry["name"]
            )
            case = construct_animation_case(candidate)
            scene = Scene()
            scene.add(*case.mobjects)
            case.animation._setup_scene(scene)
            case.animation.begin()
            case.animation.interpolate(0.5)
            case.animation.finish()
            result.update(
                {
                    "status": "interpolated",
                    "mobjects": len(case.mobjects),
                }
            )
        except UpstreamAnimationBase as error:
            result.update({"status": "upstream-failure", "detail": str(error)})
        except Exception as error:
            result.update(
                {
                    "status": "failed",
                    "detail": f"{type(error).__name__}: {error}",
                }
            )
        results.append(result)
    counts: dict[str, int] = {}
    for result in results:
        counts[result["status"]] = counts.get(result["status"], 0) + 1
    payload = {
        "classesAudited": len(results),
        "statusCounts": counts,
        "results": results,
    }
    OUTPUT.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps({key: value for key, value in payload.items() if key != "results"}, indent=2))
    for result in results:
        if result["status"] == "failed":
            print(f"FAILED {result['name']}: {result['detail']}")
    return 1 if counts.get("failed", 0) else 0


if __name__ == "__main__":
    raise SystemExit(main())
