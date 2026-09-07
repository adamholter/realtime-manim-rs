#!/usr/bin/env python3
"""Paired Cairo compiler timings with byte equality and per-process peak RSS."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
CHILD = """
import json, resource, runpy, sys
compiler = sys.argv.pop(1)
sys.argv[0] = compiler
try:
    runpy.run_path(compiler, run_name='__main__')
finally:
    usage = resource.getrusage(resource.RUSAGE_SELF)
    print('RESOURCE ' + json.dumps({'cpuSeconds': usage.ru_utime + usage.ru_stime,
          'peakRssBytes': usage.ru_maxrss * (1 if sys.platform == 'darwin' else 1024)}))
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--fps", type=int, default=15)
    parser.add_argument("--before", type=Path, help="Optional pre-change compiler; defaults to eager mode")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1 or not 1 <= args.fps <= 120:
        parser.error("runs must be positive; fps must be 1..120")
    compiler = ROOT / "scripts/compile-manim.py"
    before = args.before.resolve() if args.before else compiler
    records = []
    with tempfile.TemporaryDirectory(prefix="manim-compiler-bench-") as directory:
        temp = Path(directory)
        for name in ("ThreeDSurface", "PolyhedraAndFixedLabels"):
            expected = None
            for run in range(args.runs):
                # Alternate order to reduce warm-cache and shared-load bias.
                for mode in (("before", "after") if run % 2 == 0 else ("after", "before")):
                    output = temp / f"{name}-{mode}.json"
                    command = [sys.executable, "-c", CHILD,
                               str(before if mode == "before" else compiler),
                               str(ROOT / "benchmarks/corpus/manim_reference_scenes.py"),
                               name, "--fps", str(args.fps), "--output", str(output)]
                    if mode == "before" and not args.before:
                        command.append("--eager-surface-capture")
                    started = time.perf_counter()
                    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                                            env={**os.environ, "PYTHONHASHSEED": "0",
                                                 "REALTIME_MANIM_MEDIA_DIR": str(temp / "media")})
                    elapsed = time.perf_counter() - started
                    if result.returncode:
                        raise RuntimeError(result.stdout + result.stderr)
                    resource_line = next(line[9:] for line in result.stdout.splitlines()
                                         if line.startswith("RESOURCE "))
                    receipt = json.loads(output.with_suffix(".receipt.json").read_text())
                    assert not receipt["diagnostics"], receipt
                    data = output.read_bytes()
                    if expected is None:
                        expected = data
                    assert data == expected, f"Output changed: {name} {mode} run {run}"
                    record = {"scene": name, "mode": mode, "run": run,
                              "wallSeconds": elapsed, **json.loads(resource_line),
                              "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
                    records.append(record)
                    print(json.dumps(record), flush=True)
    summaries = []
    for name in ("ThreeDSurface", "PolyhedraAndFixedLabels"):
        summary = {"scene": name}
        for mode in ("before", "after"):
            samples = [r for r in records if r["scene"] == name and r["mode"] == mode]
            summary[mode] = {key: statistics.median(r[key] for r in samples)
                             for key in ("wallSeconds", "cpuSeconds", "peakRssBytes")}
        summary["wallSpeedup"] = summary["before"]["wallSeconds"] / summary["after"]["wallSeconds"]
        summaries.append(summary)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({"fps": args.fps, "runs": args.runs,
        "baseline": "pre-change compiler" if args.before else "eager expansion",
        "environment": {"os": sys.platform, "python": sys.version.split()[0],
                        "load": "shared workstation; other applications active; no process inventory"},
        "byteEquality": True, "records": records, "summaries": summaries}, indent=2) + "\n")


if __name__ == "__main__":
    main()
