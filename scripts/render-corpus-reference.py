#!/usr/bin/env python3
"""Render every Manim reference scene independently and keep a JSON receipt."""

from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "benchmarks" / "corpus" / "manifest.json"
SOURCE = ROOT / "benchmarks" / "corpus" / "manim_reference_scenes.py"
MEDIA = ROOT / "benchmarks" / "corpus" / "reference"
MANIM = ROOT / ".venv-manim-reference" / "bin" / "manim"
RECEIPT = MEDIA / "receipt.json"
TEXLIVE_ROOT = Path("/opt/homebrew/opt/texlive/share")


def main() -> None:
    manifest = json.loads(MANIFEST.read_text())
    MEDIA.mkdir(parents=True, exist_ok=True)
    results = []
    for entry in manifest["scenes"]:
        scene_class = entry["class"]
        started = time.perf_counter()
        process = subprocess.run(
            [
                str(MANIM),
                "-ql",
                "-s",
                "--resolution",
                "854,480",
                "--media_dir",
                str(MEDIA),
                str(SOURCE),
                scene_class,
            ],
            cwd=ROOT,
            env={
                **os.environ,
                # Homebrew's dvisvgm bottle currently resolves its own Cellar
                # prefix instead of the linked TeX Live tree on Apple silicon.
                "TEXMFROOT": str(TEXLIVE_ROOT),
                "TEXMFCNF": str(TEXLIVE_ROOT / "texmf-dist" / "web2c"),
            },
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=120,
        )
        result = {
            "id": entry["id"],
            "class": scene_class,
            "ok": process.returncode == 0,
            "seconds": round(time.perf_counter() - started, 3),
            "outputTail": process.stdout[-3000:],
        }
        results.append(result)
        print(f"{'PASS' if result['ok'] else 'FAIL'} {scene_class} {result['seconds']:.3f}s")
    payload = {
        "manimVersion": manifest["reference"]["version"],
        "sceneCount": len(results),
        "passed": sum(result["ok"] for result in results),
        "failed": sum(not result["ok"] for result in results),
        "results": results,
    }
    RECEIPT.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"reference corpus: {payload['passed']}/{payload['sceneCount']} passed")
    if payload["failed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
