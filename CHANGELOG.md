# Changelog

## 0.4.0 — 2026-08-12

- Added retained `NumberLine`, `Axes`, `NumberPlane`, `FunctionGraph`, and
  `ParametricFunction` browser authoring APIs with reversible coordinates.
- Added native butt/square/round caps, miter/miter-clip/round/bevel joins, and
  SVG-style curved-path dash patterns to the Rust/Wasm and Metal renderers.
- Added deterministic grapheme-safe registered-font fallback with explicit
  missing-vector-glyph errors instead of tofu or disappearing text.
- Replaced the native preview scaffold with a real Metal player, strict GPU
  smoke checker, and headless PNG renderer.
- Added strict TypeScript checking to the package release gate.

## 0.3.0 — 2026-08-12

- Published `realtime-manim` as a dependency-free typed ESM browser package.
- Added independent per-canvas Rust/Wasm/WebGPU players with deterministic
  destruction and isolated device-loss recovery.
- Added retained Image, Mesh, Surface, TracePath, MathTex, and low-level node
  authoring APIs.
- Added Transform, ReplacementTransform, AnimationGroup, LaggedStart, and
  Succession with retained timing semantics.
- Added per-player OpenType font registration, family selection, memory limits,
  synchronous missing-family errors, and recovery persistence.
- Published the complete sanitized Rust/JavaScript source, CI, compatibility
  corpus, and benchmark receipts.

The project does not claim full Manim Community parity yet. The current gaps are
tracked explicitly in `benchmarks/corpus/capability-matrix.json`.
