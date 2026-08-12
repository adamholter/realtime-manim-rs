#!/usr/bin/env python3
"""Validate corpus structure without importing Manim."""

from __future__ import annotations

import ast
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "benchmarks" / "corpus"


def main() -> None:
    manifest = json.loads((CORPUS / "manifest.json").read_text())
    scenes = manifest["scenes"]
    assert 12 <= len(scenes) <= 24, "R-01 requires a 12–20 scene corpus"
    ids = [scene["id"] for scene in scenes]
    classes = [scene["class"] for scene in scenes]
    assert len(ids) == len(set(ids)), "corpus ids must be unique"
    assert len(classes) == len(set(classes)), "corpus class names must be unique"
    assert all(scene["capabilities"] and scene["assertions"] for scene in scenes)

    source = (CORPUS / "manim_reference_scenes.py").read_text()
    module = ast.parse(source)
    defined = {
        node.name
        for node in module.body
        if isinstance(node, ast.ClassDef)
    }
    missing = sorted(set(classes) - defined)
    assert not missing, f"missing reference classes: {missing}"

    tiers = {scene["tier"] for scene in scenes}
    required = {"core-2d", "typography", "animation", "dynamic", "camera", "media", "3d", "audio", "international"}
    assert required <= tiers, f"missing tiers: {sorted(required - tiers)}"
    print(f"corpus ok: {len(scenes)} scenes, {len(tiers)} tiers, {sum(len(scene['capabilities']) for scene in scenes)} capability checks")


if __name__ == "__main__":
    main()
