#!/usr/bin/env python3
"""Compile every pinned Manim corpus scene and emit one aggregate receipt."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "benchmarks/corpus/manifest.json"
SOURCE = ROOT / "benchmarks/corpus/manim_reference_scenes.py"
OUTPUT = ROOT / "benchmarks/compat/full-corpus"
COMPILER = ROOT / "scripts/compile-manim.py"


def compile_entry(entry: dict[str, object], fps: int) -> dict[str, object]:
    class_name = str(entry["class"])
    output = OUTPUT / f"{class_name}.json"
    with tempfile.TemporaryDirectory(
        prefix=f"media-{class_name}-", dir=OUTPUT
    ) as media_dir:
        environment = os.environ.copy()
        environment["REALTIME_MANIM_MEDIA_DIR"] = media_dir
        result = subprocess.run(
            [
                sys.executable,
                str(COMPILER),
                str(SOURCE),
                class_name,
                "--fps",
                str(fps),
                "--output",
                str(output),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            env=environment,
        )
    if result.returncode != 0:
        raise RuntimeError(
            f"{class_name} failed ({result.returncode})\n{result.stdout}\n{result.stderr}"
        )
    scene = json.loads(output.read_text())
    receipt = json.loads(output.with_suffix(".receipt.json").read_text())
    return {
        "scene": class_name,
        "bytes": output.stat().st_size,
        "nodes": len(scene["nodes"]),
        "tracks": len(scene["tracks"]),
        "sampledFrames": receipt["sampledFrames"],
        "diagnostics": receipt["diagnostics"],
        "semanticAffineTransformTracks": receipt.get(
            "semanticAffineTransformTracks", 0
        ),
        "semanticTimelineControls": receipt.get(
            "semanticTimelineControls", 0
        ),
        "semanticMatchingCorrespondences": receipt.get(
            "semanticMatchingCorrespondences", 0
        ),
        "semanticMatchingPathTracks": receipt.get(
            "semanticMatchingPathTracks", 0
        ),
        "semanticMatchingFallbacks": receipt.get(
            "semanticMatchingFallbacks", []
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workers", type=int, default=4)
    arguments = parser.parse_args()
    manifest = json.loads(MANIFEST.read_text())
    fps = int(manifest["reference"]["fps"])
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(
        max_workers=max(1, arguments.workers)
    ) as executor:
        futures = {
            executor.submit(compile_entry, entry, fps): str(entry["class"])
            for entry in manifest["scenes"]
        }
        results = []
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            print(
                f"compiled {result['scene']}: {result['nodes']} nodes, "
                f"{result['tracks']} tracks, {result['bytes']} bytes"
            )
    order = {
        str(entry["class"]): index
        for index, entry in enumerate(manifest["scenes"])
    }
    results.sort(key=lambda result: order[str(result["scene"])])
    report = {
        "fps": fps,
        "scenes": len(results),
        "bytes": sum(int(result["bytes"]) for result in results),
        "nodes": sum(int(result["nodes"]) for result in results),
        "tracks": sum(int(result["tracks"]) for result in results),
        "diagnostics": sum(
            len(result["diagnostics"]) for result in results  # type: ignore[arg-type]
        ),
        "semanticAffineTransformTracks": sum(
            int(result["semanticAffineTransformTracks"]) for result in results
        ),
        "semanticTimelineControls": sum(
            int(result["semanticTimelineControls"]) for result in results
        ),
        "semanticMatchingCorrespondences": sum(
            int(result["semanticMatchingCorrespondences"])
            for result in results
        ),
        "semanticMatchingPathTracks": sum(
            int(result["semanticMatchingPathTracks"])
            for result in results
        ),
        "semanticMatchingFallbacks": sorted(
            {
                str(fallback)
                for result in results
                for fallback in result["semanticMatchingFallbacks"]
            }
        ),
        "results": results,
    }
    (OUTPUT / "compile-report.json").write_text(
        json.dumps(report, indent=2) + "\n"
    )
    print(
        f"corpus: {report['scenes']} scenes, {report['bytes']} bytes, "
        f"{report['nodes']} nodes, {report['tracks']} tracks, "
        f"{report['diagnostics']} diagnostics"
    )
    return 1 if report["diagnostics"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
