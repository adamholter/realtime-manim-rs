# Deferred Cairo surface compilation

The compiler used to expand every Cairo surface into per-patch triangle and
wireframe vertices, normals, and colors at every sampled frame. It then discarded
those arrays when emitting a retained `surface` node.

The default path now keeps compact patch data. It preserves unrounded coordinates
for the legacy fallback, including fully invisible lifetimes. The eager path is
still available with `--eager-surface-capture` for differential tests. Material
normalization reuses the previous color strings only when the RGBA arrays are
exactly equal. There is no tolerance-based color approximation.

## Measurements

Three paired runs per scene, alternating order, at 15 fps. Baseline compiler
`f523b45adf2e80d0988b795ddeccd084fff6f74f`. Apple M4, 16 GB, macOS 27,
Python 3.13.5, Manim 0.20.1. Other applications remained active. Other validation
work overlapped later Polyhedra runs. Wall times are not controlled-idle results.
CPU time and peak RSS come from each compiler process, not an aggregate across
children. MB below means decimal megabytes.

| Scene and metric | Before median | After median |
| --- | ---: | ---: |
| ThreeDSurface wall time | 30.781 s | 10.566 s |
| ThreeDSurface CPU time | 22.708 s | 8.009 s |
| ThreeDSurface peak RSS | 342.213 MB | 148.406 MB |
| PolyhedraAndFixedLabels wall time | 48.625 s | 32.539 s |
| PolyhedraAndFixedLabels CPU time | 32.327 s | 15.528 s |
| PolyhedraAndFixedLabels peak RSS | 390.136 MB | 118.800 MB |

The surface wall-time improvement is 2.91x and peak RSS falls 56.6%. Polyhedra
CPU time falls 52.0% and peak RSS falls 69.5%. This does not measure playback FPS,
GPU time, or imply equivalent gains for unrelated scenes.

All 12 compiled scenes have byte-identical output within their scene group.
ThreeDSurface is 183,388 bytes with SHA-256
`21a3c895e425849d786480e0ef3306cd5cb9341c88b6c192a0ac7848d903c6f9`.
PolyhedraAndFixedLabels is 407,675 bytes with SHA-256
`6bd2b4fc5ecae6509416b83616c67d29e219d949eb197183fa0a887b2ed56968`.

Raw samples: [compiler timings](../../benchmarks/runtime/2026-09-07-surface-compiler.json).
Schema-validated [receipt](../../benchmarks/runtime/2026-09-07-surface-compiler.receipt.json).

## Reproduce

Use the pinned environment in [CONTRIBUTING.md](../../CONTRIBUTING.md).
Export `scripts/compile-manim.py` from the baseline revision into
`benchmarks/results/compile-manim-before.py`, then run:

```sh
npm run benchmark:surface-compiler -- \
  --before benchmarks/results/compile-manim-before.py \
  --runs 3 --fps 15 \
  --output benchmarks/results/surface-compiler.json
```

Without `--before`, the benchmark compares the current eager and deferred paths.
That comparison includes the shared color-cache improvement on both sides, so
it is not the historical baseline above.

## Correctness and release gates

- Four focused scenes compare eager and deferred JSON and diagnostic receipts
  exactly. They cover fade-in/out, shaded checkerboards, material and stroke
  changes, nonuniform transforms, sphere poles, topology changes, fully invisible
  lifetimes, and collapsed cells.
- The reference corpus compiles all 20 scenes with zero diagnostics. Totals
  remain 2,798,693 bytes, 1,140 nodes, and 1,038 tracks.
- All 180 constructor classes and 70 animation classes compile in their sweeps.
- The WebGPU corpus sweep renders 60 seeks across 20 scenes with no errors.
- Workspace Rust, Python, server, package, TypeScript, and docs checks pass.
  Browser checks cover paused redraw, multiple players, device recovery,
  typography, transparency, and desktop/mobile playground interaction.

This release does not support every Python Manim program. This change preserves
the supported scene data and renderer quality; it does not resolve the remaining
compatibility gaps listed in the capability matrix.
