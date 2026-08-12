#!/usr/bin/env python3
"""Compile all constructor-sweep batches and publish a machine-readable receipt."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "benchmarks" / "compat" / "constructor_sweep_scenes.py"
OUTPUT_DIR = ROOT / "benchmarks" / "compat" / "constructor-sweep"
RECEIPT = ROOT / "docs" / "receipts" / "constructor-sweep.json"


def load_sweeps() -> Any:
    compat_root = str(SOURCE.parent)
    if compat_root not in sys.path:
        sys.path.insert(0, compat_root)
    spec = importlib.util.spec_from_file_location("_constructor_sweeps", SOURCE)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Could not import {SOURCE}.")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fps", type=int, default=30)
    arguments = parser.parse_args()
    module = load_sweeps()
    scene_names = sorted(
        name
        for name in vars(module)
        if name.startswith(("CairoConstructorSweep", "OpenGLConstructorSweep"))
        and name[-2:].isdigit()
    )
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    rows: list[dict[str, Any]] = []
    environment = {
        **os.environ,
        "PYTHONPATH": str(SOURCE.parent)
        + os.pathsep
        + os.environ.get("PYTHONPATH", ""),
        "TEXMFROOT": "/opt/homebrew/opt/texlive/share",
        "TEXMFCNF": "/opt/homebrew/opt/texlive/share/texmf-dist/web2c",
    }
    for scene_name in scene_names:
        renderer = "opengl" if scene_name.startswith("OpenGL") else "cairo"
        output = OUTPUT_DIR / f"{scene_name}.json"
        command = [
            sys.executable,
            str(ROOT / "scripts" / "compile-manim.py"),
            str(SOURCE),
            scene_name,
            "--renderer",
            renderer,
            "--fps",
            str(arguments.fps),
            "--output",
            str(output),
        ]
        result = subprocess.run(
            command,
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
        scene_class = getattr(module, scene_name)
        rows.append(
            {
                "scene": scene_name,
                "renderer": renderer,
                "classes": list(scene_class.constructor_classes),
                "status": "compiled" if result.returncode == 0 else "failed",
                "diagnostics": receipt.get("diagnostics", []),
                "output": str(output.relative_to(ROOT)),
                "stderr": result.stderr[-1000:],
            }
        )
        print(
            f"{scene_name}: {'ok' if result.returncode == 0 else 'failed'} "
            f"({len(scene_class.constructor_classes)} classes)"
        )
    payload = {
        "batches": len(rows),
        "classes": sum(len(row["classes"]) for row in rows),
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
