#!/usr/bin/env python3
"""Byte-level differential gate for deferred Cairo surface expansion."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SOURCE = """
from manim import *

class SurfaceMaterials(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=65*DEGREES, theta=-45*DEGREES)
        s = Surface(lambda u,v: [u,v,.4*np.sin(u)*np.cos(v)],
                    resolution=(4,5), u_range=[-2,2], v_range=[-2,2])
        s.set_fill_by_checkerboard(BLUE_D, GOLD, opacity=.65)
        s.set_stroke(RED, width=2, opacity=.4)
        self.play(FadeIn(s), run_time=.5)
        self.play(s.animate.stretch(1.3, 2).set_fill(GREEN, opacity=.3)
                  .set_stroke(YELLOW, width=4), run_time=.5)
        self.play(FadeOut(s), run_time=.5)

class SurfaceTopology(ThreeDScene):
    def construct(self):
        s = Sphere(resolution=(4,6), fill_opacity=.6)
        self.add(s)
        self.wait(.25)
        s.become(Sphere(resolution=(5,7), fill_opacity=.8))
        self.wait(.25)
        s.scale([1.2,.7,1.8])
        self.wait(.25)

class InvisibleSurface(ThreeDScene):
    def construct(self):
        s = Surface(lambda u,v: [u,v,u*v], resolution=(2,3))
        s.set_fill(opacity=0).set_stroke(opacity=0)
        self.add(s)
        self.wait(.25)
        s.shift(UP)
        self.wait(.25)

class CollapsedSurface(ThreeDScene):
    def construct(self):
        s = Surface(lambda u,v: [0.,0.,0.], resolution=(2,2))
        self.add(s)
        self.wait(.25)
"""


def main():
    with tempfile.TemporaryDirectory(prefix="manim-surfaces-") as directory:
        root = Path(directory)
        source = root / "surface_cases.py"
        source.write_text(SOURCE)
        for name in ("SurfaceMaterials", "SurfaceTopology", "InvisibleSurface", "CollapsedSurface"):
            results = []
            for eager in (False, True):
                output = root / f"{name}-{eager}.json"
                command = [sys.executable, str(ROOT / "scripts/compile-manim.py"),
                           str(source), name, "--fps", "8", "--output", str(output)]
                if eager:
                    command.append("--eager-surface-capture")
                result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                                        env={**os.environ, "REALTIME_MANIM_MEDIA_DIR": str(root / "media")})
                assert result.returncode == 0, result.stdout + result.stderr
                receipt = json.loads(output.with_suffix(".receipt.json").read_text())
                assert receipt["diagnostics"] == [], receipt
                results.append((output.read_bytes(), receipt))
            assert results[0] == results[1], f"Deferred/eager mismatch: {name}"
            scene = json.loads(results[0][0])
            if name in ("SurfaceMaterials", "SurfaceTopology"):
                assert any(node["type"] == "surface" for node in scene["nodes"])
            if name == "InvisibleSurface":
                assert any(node["type"] == "mesh" for node in scene["nodes"])
            print(json.dumps({"scene": name, "identical": True,
                              "sha256": hashlib.sha256(results[0][0]).hexdigest()}))


if __name__ == "__main__":
    main()
