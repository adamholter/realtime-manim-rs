#!/usr/bin/env python3
"""Inventory Manim's public class surface against retained compiler paths."""

from __future__ import annotations

import inspect
import importlib
import json
import pkgutil
from pathlib import Path
from typing import Any

import manim
import manim.mobject.opengl
from manim.animation.animation import Animation
from manim.mobject.mobject import Mobject
from manim.mobject.opengl.opengl_mobject import OpenGLMobject
from manim.mobject.opengl.opengl_point_cloud_mobject import OpenGLPMobject
from manim.mobject.opengl.opengl_surface import OpenGLSurface
from manim.mobject.opengl.opengl_vectorized_mobject import OpenGLVMobject
from manim.mobject.types.image_mobject import AbstractImageMobject
from manim.mobject.types.point_cloud_mobject import PMobject
from manim.mobject.types.vectorized_mobject import VMobject
from manim.scene.scene import Scene


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "benchmarks" / "corpus" / "manim-api-inventory.json"


def safe_subclass(candidate: type[Any], base: type[Any]) -> bool:
    try:
        return issubclass(candidate, base)
    except TypeError:
        return False


def classify(candidate: type[Any]) -> tuple[str, str]:
    if safe_subclass(candidate, Scene):
        return ("scene", "manim-semantics")
    if safe_subclass(candidate, Animation):
        return ("animation", "manim-semantics")
    if safe_subclass(candidate, OpenGLSurface):
        return ("opengl-surface", "retained-mesh")
    if safe_subclass(candidate, OpenGLPMobject):
        return ("opengl-points", "retained-point-cloud")
    if safe_subclass(candidate, OpenGLVMobject):
        return ("opengl-vector", "retained-path")
    if safe_subclass(candidate, OpenGLMobject):
        return ("opengl-custom", "shader-data-inspection")
    if safe_subclass(candidate, AbstractImageMobject):
        return ("image", "retained-image")
    if safe_subclass(candidate, PMobject):
        return ("points", "retained-point-cloud")
    if safe_subclass(candidate, VMobject):
        return ("vector", "retained-path")
    if safe_subclass(candidate, Mobject):
        return ("composite-or-nonvisual", "family-traversal")
    return ("other", "manual-review")


def main() -> None:
    candidates: dict[tuple[str, str], type[Any]] = {}
    for name in sorted(dir(manim)):
        candidate = getattr(manim, name)
        if not name.startswith("_") and inspect.isclass(candidate):
            candidates[(name, candidate.__module__)] = candidate
    for module_info in pkgutil.walk_packages(
        manim.mobject.opengl.__path__,
        prefix=f"{manim.mobject.opengl.__name__}.",
    ):
        module = importlib.import_module(module_info.name)
        for name, candidate in inspect.getmembers(module, inspect.isclass):
            if (
                not name.startswith("_")
                and candidate.__module__ == module.__name__
            ):
                candidates[(name, candidate.__module__)] = candidate
    entries = []
    for (name, module), candidate in sorted(candidates.items()):
        category, compiler_path = classify(candidate)
        entries.append(
            {
                "name": name,
                "module": module,
                "category": category,
                "compilerPath": compiler_path,
                "abstract": inspect.isabstract(candidate),
            }
        )
    category_counts: dict[str, int] = {}
    compiler_path_counts: dict[str, int] = {}
    for entry in entries:
        category_counts[entry["category"]] = (
            category_counts.get(entry["category"], 0) + 1
        )
        compiler_path_counts[entry["compilerPath"]] = (
            compiler_path_counts.get(entry["compilerPath"], 0) + 1
        )
    payload = {
        "manimVersion": manim.__version__,
        "publicClasses": len(entries),
        "categoryCounts": dict(sorted(category_counts.items())),
        "compilerPathCounts": dict(sorted(compiler_path_counts.items())),
        "classes": entries,
    }
    OUTPUT.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps({key: payload[key] for key in payload if key != "classes"}, indent=2))


if __name__ == "__main__":
    main()
