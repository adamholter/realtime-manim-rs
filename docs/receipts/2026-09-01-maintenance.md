# 2026-09-01 maintenance receipt

Base revision: `039b0d0`

This tranche fixes nondeterministic Python compiler output and refreshes the
documentation site's compatible dependency set. It does not change the public
npm package, GitHub branch, or hosted site.

## QuickHull compiler correctness

Manim Community 0.20.1 calls `np.random.default_rng()` without a seed in
`QuickHull.initialize`. That bypassed the compiler's existing Python, NumPy,
Manim, and hash seeds. `ConvexHull3D` could therefore change node count, draw
order, and JSON bytes between fresh processes.

Before this change, four controlled entropy runs produced four JSON hashes and
16 or 17 nodes for the same tetrahedron. Repeated full
`CairoConstructorSweep02` compiles also alternated between two outputs, with 74
or 75 nodes. A valid five-point pyramid could emit zero nodes when the initial
seed-zero simplex was coplanar.

The compiler now uses a local seed-zero generator. It keeps that simplex when
it has full affine rank. If it does not, the compiler selects a deterministic
full-rank basis in input order. Lower-dimensional input fails with a clear
error.

After the change:

- tetrahedron, four entropy seeds, 16 nodes each:
  `f0d606fdfe0901c2f05eeee3bf9d7580cc19389c400410fd5243d85b838ea7c1`
- five-point pyramid, four entropy seeds, 24 nodes each:
  `9df983839f886f9ce8ec4ff3c9a9064c254e38c0ad11d71791aee22ea0ed0087`
- `CairoConstructorSweep02`, three fresh processes, 74 nodes each:
  `4ef66134364c4f4228cc24c52e9a47cd2dd60ee5eaabea7527d3ff84955b7926`
- constructor receipt, three fresh processes:
  `d8e2e4ef105547b18c149c38184dd664fe5c92a410842eff37d8ee5f0fc294a5`

Regression command:

```sh
.venv-manim-reference/bin/python scripts/test-compile-manim-determinism.py
```

## Documentation site maintenance

The site now uses compatible patch releases for React 19.2, vinext, Vite 8,
the Cloudflare Vite plugin, Wrangler, and their direct type and build
dependencies. The no-force npm audit repair updated vulnerable transitive
packages. High-severity findings fell from 15 to zero. Four moderate findings
remain in Drizzle Kit's development-only esbuild loader chain. npm offers only
a breaking Drizzle Kit downgrade, so it was not applied.

The site test now runs TypeScript before its production build. Cloudflare
worker bindings have explicit types. Vite's future native config-loader warning
is resolved. The repository guide link now targets the tracked
`site/public/agent-guide.md` file.

## Verification

- Rust format, workspace check, clippy, and 133 tests passed.
- Pinned Rust 1.88 Wasm check, clippy, release build, native release build, and
  strict native smoke passed.
- Python corpus compiled 20 of 20 scenes with 1,140 nodes, 1,038 tracks, and
  zero diagnostics.
- Python scene subclass, constructor, animation, stroke, transform, and new
  entropy regressions passed.
- JavaScript server tests passed 39 of 39. Package tests passed 35 of 35, plus
  TypeScript, tarball install, multiplayer, and device recovery gates.
- WebGPU rendered 20 scenes at 60 seek points with zero errors. Combined
  readback time was 0.7 ms p50 and 2.2 ms p95.
- Site TypeScript, lint, production build, and three rendered HTML tests passed.
- Isolated desktop and 390 by 844 mobile browser runs rendered three distinct
  scene hashes, accepted slider and editor changes, and reported no console,
  HTTP, or horizontal-overflow errors.
- Root npm production and development audit reported zero findings. Site
  production-only audit reported zero findings.

The compile, first-frame, steady-state FPS, CPU time, observable combined GPU
completion, memory-growth, and large-scene throughput baselines remain recorded
in `docs/receipts/P-12-paused-redraw.md`. This correctness-only compiler change
does not touch the Rust renderer or browser playback loop.

## Public state

- npm and jsDelivr still serve `realtime-manim@0.6.0` JavaScript and Wasm.
- The repository and nested GitHub Pages package example return HTTP 200.
- The GitHub Pages root still returns HTTP 404. The repository homepage points
  to the working nested example.
- No release, push, Pages change, or site deployment was made.
