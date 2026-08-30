# Heavy runtime fixtures

These deterministic gzip files keep the measured browser benchmark reproducible from
a clean checkout without committing roughly 11 MiB of generated JSON.

`VectorFieldAndStreamLines`, `TextAndMath`, `ThreeDSurface`, and
`PolyhedraAndFixedLabels` are current outputs from:

```sh
.venv-manim-reference/bin/python scripts/compile-corpus.py --workers 4
```

`ThreeDSurfaceCompact2` preserves the existing 30 fps, 669-node, 2,983-track
retained-scene stress artifact. It intentionally remains larger than the current
compiler's compact 3D output so browser parsing, allocation, seeking, and memory
growth stay under pressure.

All archives use `gzip -n` so gzip headers contain no source path or timestamp.
Decompressed SHA-256 values:

| Fixture | SHA-256 |
| --- | --- |
| PolyhedraAndFixedLabels | `6bd2b4fc5ecae6509416b83616c67d29e219d949eb197183fa0a887b2ed56968` |
| TextAndMath | `9e2dfd670acfdb2cc792ceac0d8459dd786327a3f7234f5468ed2e22ada49a3d` |
| ThreeDSurface | `21a3c895e425849d786480e0ef3306cd5cb9341c88b6c192a0ac7848d903c6f9` |
| ThreeDSurfaceCompact2 | `567abc61f736e12d1a12959357e35dba0b71a8d28a59f4495590d2f1a6ec2d2b` |
| VectorFieldAndStreamLines | `35a7eb25efa647ee77d7b732e00cfc2813f077790593ee71901e7a152ad4dcd4` |

Changing a fixture requires updating this table and recording a new before/after
receipt. Do not silently replace benchmark inputs with newer compiler output.
