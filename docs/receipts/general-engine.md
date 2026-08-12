# General retained engine receipt

Updated: 2026-08-10

## Delivered

- scene IR version `2`
- retained hierarchy with stable string IDs, parent validation, z-order, lifetimes,
  transforms, styles, and camera
- node algebra: group, circle, rectangle, line, arrow, polyline, quadratic/cubic
  Bézier path, OpenType-shaped vector text, MathTex, styled SVG, point cloud,
  indexed surface patches, triangle meshes, and translated custom shaders
- explicit-time Rust evaluator with numeric/color/point tracks, seven easing modes,
  reusable numeric signals, and bounded expression bindings
- random-seek equivalence tests and parent/signal validation tests
- generic lyon tessellation into a batched WebGPU vertex/index stream
- 4x-MSAA GPU depth testing for opaque 3D meshes/surfaces with view-depth
  projection and near/far-plane polygon clipping
- dynamic GPU buffer growth, resize handling, pause/reset, and live frame metrics
- exact-frame browser timeline scrubbing with resume-from-scrub engine timing
- GPT-5.6 Terra scene compiler using the OpenRouter Agent SDK
- JavaScript and Rust validation; no evaluation of model-generated JavaScript
- declared signal controls rendered as live sliders and evaluated in Rust
- user-authored monotonic Manim `ValueTracker` timelines automatically exposed as
  synchronized semantic sliders that invert value to exact compiled scene time
- exact-frame WebCodecs/WebM export, playback, download, and timing, with pixel
  resolution independent from mathematical scene dimensions
- MediaRecorder fallback for browsers without WebCodecs
- pure translated paths lowered to native `x`/`y` transform tracks while path
  morphs retain their full command geometry
- provable multi-subpath creation/passing-flash traces lowered to independent Rust
  draw bounds
- paired partial-path bounds lowered to one validated atomic Rust `drawRange` track
- uniform scale-plus-translation traces lowered to retained transforms and shared
  affine parent groups with stroke-width compensation
- exact and translated static path instances reuse one concrete geometry node
- identical property timelines reuse one validated concrete keyframe sequence
- topology-stable Cairo surfaces lowered to indexed patches with native animated
  control-vertex, fill/stroke-material, and wire-radius tracks
- topology-changing Cairo surfaces split at connectivity transitions into exact
  compact retained lifetimes instead of expanded-mesh animation
- camera-independent 3D cubic paths retained in world coordinates and projected
  by Rust at explicit camera time
- Manim `TracedPath` trails retained as shared cubic segments with explicit-time
  sliding windows evaluated by Rust
- compatible path morphs retain command topology once and animate flat numeric
  `pathData` vectors
- proven rotation/scale/translation paths use one composite Rust `transform2d`
  timeline with embedded stroke compensation
- proven general affine paths use one native Rust `affine2d` matrix timeline
  instead of sampled path coordinates, with world-space stroke tessellation
- compatible `MovingCameraScene` vectors remain in world space and animate direct
  Rust `cameraX`, `cameraY`, and `cameraZoom` tracks
- compatible `MovingCameraScene` gradients, images, and point clouds also remain
  in world space; point sprites use a compact screen-space-radius mode under camera zoom
- proven roll-free 3D camera motion uses one native orbit-position track and derives
  the orthogonal up-vector in Rust
- fixed-in-frame vectors remain 2D and fixed-orientation families, including
  single, self-animated, and topology-changing objects, use native animatable 3D
  billboard anchors projected by Rust
- styled SVG group and linked clip paths remain vector geometry and use nested
  GPU stencil intersections, including multi-shape unions and clipped strokes
- SVG alpha and luminance masks render full premultiplied vector paint into
  lazily allocated 4x-MSAA GPU texture-array layers; linked masks multiply at sampling time
- deterministic compiler seeds for Python, NumPy, and Manim configuration

## Export resolution regression

An isolated headless Chrome run at a responsive `1512×982` viewport kept the
preview canvas contained at `595×334`, with no horizontal overflow. Export used
an independent `854×480` target and produced a playable VP9 WebM:

- `854×480`, `15 fps`, four exact frames
- `37,227` bytes; SHA-256
  `a76efb94039fe2249e378727b9f50a399b9a4a93c7e24436878257e37c080131`
- `ffprobe` duration `0.200000 s`
- zero page or console errors
- preview restored to `595×334` after export

Artifact: `benchmarks/compat/local-app-export-smoke.webm`

## Held-out live runs

### Pareto frontier

- title: `Pareto Frontier: Best Trade-offs`
- nodes: `31`
- node types: group, rectangle, text, line, arrow, point cloud, circle, polyline
- animation tracks: `32`
- forbidden sine/wave substitution: absent
- agent: `69.98 s`
- render/record: `5.04 s`
- total: `75.02 s`
- output: playable five-second WebM, `477 KB`
- receipt: `docs/receipts/general-engine-pareto.png`

### Pythagorean proof

- title: `Pythagorean Theorem: A Visual Proof`
- nodes: `21`
- node types: rectangle, text, group, path, polyline, arrow
- animation tracks: `27`
- agent: `50.13 s`
- render/record: `5.04 s`
- total: `55.17 s`
- output: playable five-second WebM, `334 KB`
- receipt: `docs/receipts/general-engine-pythagorean.png`

## Automated verification

```sh
npm run check:web
npm run test:web
cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p realtime-manim-web-preview --target wasm32-unknown-unknown
cargo clippy -p realtime-manim-web-preview --target wasm32-unknown-unknown -- -D warnings
./scripts/build-web.sh
```

## Honest boundaries

- This is a general retained 2D/3D engine with broad regular-Manim execution and
  compilation, not yet a claim of complete arbitrary-Python/third-party compatibility.
- Text is shaped and vector, but full system-font fallback, rich markup, complex
  script coverage, and color emoji are not complete.
- MathTex is real LaTeX/vector output. SVG multistop linear and focal radial
  gradients, stop opacity, arbitrary gradient transforms, and
  pad/repeat/reflect spread are native per-fragment GPU paints. SVG group and
  linked clip paths are native stencil geometry, and fractional alpha/luminance
  masks, including masks nested in mask content, are native GPU layers. Transformed
  vector pattern fills/strokes retain gradient children, tile-local clips, recursively
  nested pattern paints, masks on tile children, and pattern paints inside mask
  definitions. Nested clip paths inside clip definitions are recursive vector coverage
  layers. Filters and embedded images are not complete.
- WebCodecs export uses explicit scene times and timestamps. The fallback
  `captureStream` path remains real-time and is labeled as such.
- Third-party render hooks that bypass `ShaderWrapper`, continuously interpolated
  topology correspondence inside one surface lifetime,
  broader native camera cases, updater-closure operators, and topology-changing semantic
  path-correspondence operators remain;
  see `benchmarks/corpus/capability-matrix.json`.
- Translucent intersecting meshes still use sorted alpha blending; order-independent
  transparency is not complete.
- The experimental lyon + wgpu backend has not yet completed the Vello shootout.
