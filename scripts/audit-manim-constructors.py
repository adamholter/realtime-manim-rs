#!/usr/bin/env python3
"""Instantiate every public Manim mobject class with a deterministic fixture."""

from __future__ import annotations

import argparse
import importlib
import json
from pathlib import Path
from typing import Any

from manim import Mobject, __version__ as manim_version


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "benchmarks" / "corpus" / "manim-api-inventory.json"
OUTPUT = ROOT / "benchmarks" / "corpus" / "manim-constructor-audit.json"
FIXTURE_ROOT = ROOT / "benchmarks" / "compat"
INVISIBLE_HELPERS = {"OpenGLPoint", "OpenGLVectorizedPoint", "VectorizedPoint"}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=OUTPUT)
    arguments = parser.parse_args()

    import sys

    sys.path.insert(0, str(FIXTURE_ROOT))
    from constructor_fixtures import (  # pylint: disable=import-error
        UpstreamConstructorFailure,
        construct_fixture,
    )

    inventory = json.loads(INVENTORY.read_text())
    categories = {
        "vector",
        "points",
        "image",
        "opengl-vector",
        "opengl-points",
        "opengl-surface",
        "opengl-custom",
        "composite-or-nonvisual",
    }
    results: list[dict[str, Any]] = []
    for entry in inventory["classes"]:
        if entry["category"] not in categories:
            continue
        result = {
            "name": entry["name"],
            "module": entry["module"],
            "category": entry["category"],
        }
        try:
            candidate = getattr(
                importlib.import_module(entry["module"]), entry["name"]
            )
            instance = construct_fixture(candidate)
            if not isinstance(instance, Mobject) and not hasattr(instance, "get_family"):
                raise TypeError("constructor did not return a renderable Manim object")
            family = instance.get_family()
            result.update(
                {
                    "status": "constructed",
                    "familyMembers": len(family),
                    "pointCount": sum(
                        len(getattr(member, "points", ())) for member in family
                    ),
                    "visualIntent": entry["name"] not in INVISIBLE_HELPERS,
                }
            )
        except UpstreamConstructorFailure as error:
            result.update(
                {"status": "upstream-failure", "detail": str(error)}
            )
        except Exception as error:  # Constructor audit must retain every failure.
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
        "manimVersion": manim_version,
        "classesAudited": len(results),
        "statusCounts": counts,
        "results": results,
    }
    output = arguments.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps({key: value for key, value in payload.items() if key != "results"}, indent=2))
    for result in results:
        if result["status"] == "failed":
            print(f"FAILED {result['name']}: {result['detail']}")
    return 1 if counts.get("failed", 0) else 0


if __name__ == "__main__":
    raise SystemExit(main())
