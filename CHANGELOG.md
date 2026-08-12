# Changelog

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
