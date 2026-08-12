#!/usr/bin/env python3
"""Compile behavioral fixtures for Manim's seven public Scene subclasses."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "benchmarks" / "compat" / "scene_subclass_scenes.py"
OUTPUT_DIR = ROOT / "benchmarks" / "compat" / "scene-subclasses"
RECEIPT = ROOT / "docs" / "receipts" / "scene-subclasses.json"
SCENES = [
    "SceneCompatibility",
    "MovingCameraSceneCompatibility",
    "ThreeDSceneCompatibility",
    "SpecialThreeDSceneCompatibility",
    "VectorSceneCompatibility",
    "LinearTransformationSceneCompatibility",
    "ZoomedSceneCompatibility",
]


def main() -> int:
    environment = {
        **os.environ,
        "TEXMFROOT": "/opt/homebrew/opt/texlive/share",
        "TEXMFCNF": "/opt/homebrew/opt/texlive/share/texmf-dist/web2c",
    }
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    rows: list[dict[str, Any]] = []
    for scene_name in SCENES:
        output = OUTPUT_DIR / f"{scene_name}.json"
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts" / "compile-manim.py"),
                str(SOURCE),
                scene_name,
                "--renderer",
                "cairo",
                "--fps",
                "30",
                "--output",
                str(output),
            ],
            cwd=ROOT,
            env=environment,
            text=True,
            capture_output=True,
            check=False,
        )
        receipt_path = output.with_suffix(".receipt.json")
        receipt = (
            json.loads(receipt_path.read_text())
            if receipt_path.exists()
            else {"diagnostics": ["compile-failed"]}
        )
        rows.append(
            {
                "scene": scene_name,
                "status": "compiled" if result.returncode == 0 else "failed",
                "diagnostics": receipt.get("diagnostics", []),
                "output": str(output.relative_to(ROOT)),
                "stderr": result.stderr[-1500:],
            }
        )
        print(f"{scene_name}: {'ok' if result.returncode == 0 else 'failed'}")
    payload = {
        "scenes": len(rows),
        "compiled": sum(row["status"] == "compiled" for row in rows),
        "failed": sum(row["status"] == "failed" for row in rows),
        "rows": rows,
    }
    RECEIPT.parent.mkdir(parents=True, exist_ok=True)
    RECEIPT.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps({key: value for key, value in payload.items() if key != "rows"}, indent=2))
    return 1 if payload["failed"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
