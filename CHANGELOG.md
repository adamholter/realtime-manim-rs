# Changelog

## 0.6.0 — 2026-08-12

- Added strict nested Pango-style `MarkupText` parsing and attributed shaping
  across native Metal and browser WebGPU, including inherited fonts, bidi-safe
  paint spans, background paint, rise/tracking, underline and strikethrough.
- Added production native SVG rendering for fills, strokes, gradients, clips,
  masks, nested patterns and embedded raster images with bounded diagnostics.
- Added deterministic semantic `TransformMatchingTex` and
  `TransformMatchingShapes` compilation with stable correspondence metadata,
  compact endpoint lowering and exact sampled fallbacks.
- Added a reproducible Vello-versus-lyon renderer shootout with native Metal
  and matched browser WebGPU receipts; lyon remains the experimental baseline.
- Added lossless correspondence validation through the studio/server schema.
- Published an agent-ready documentation site, `/llms.txt`, agent guide and
  complete public source references.

## 0.5.0 — 2026-08-12

- Added Manim-style retained spatial layout and exact nested 2D bounds,
  including Bezier extrema, hierarchy-safe `copy()`, `nextTo`, `alignTo`,
  `toEdge`, `toCorner`, and `Group.arrange`.
- Changed `Mobject.moveTo(...)` from setting the retained transform origin to
  Manim-compatible center/critical-point alignment. Use `node.transform.x/y`
  directly only when raw retained-origin placement is intentionally required.
- Added native UAX #9 bidirectional layout, explicit script shaping, joined
  Arabic, Hebrew, mixed LTR/RTL text, and bundled portable vector fallbacks.
- Expanded the native Metal renderer with raster images, Path3d, lit/color and
  dual-textured meshes, patch surfaces, depth/culling/clipping, and all six
  image reconstruction modes used by the browser renderer.
- Corrected transparent 3D compositing in both renderers with opaque-first
  depth, global far-to-near triangle ordering, and depth-tested blending.
- Reused WebGPU frame-geometry allocations and added a headless WebGPU corpus
  sweep that records per-frame hashes and machine-readable benchmark receipts.
- Added synchronous text-resource validation so unsupported vector glyphs fail
  scene loading with an explicit error instead of producing a blank frame.
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
