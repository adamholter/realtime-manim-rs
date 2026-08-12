# realtime-manim-rs

A high-performance, real-time mathematical animation runtime in Rust.

[![npm](https://img.shields.io/npm/v/realtime-manim)](https://www.npmjs.com/package/realtime-manim)
[![CI](https://github.com/adamholter/realtime-manim-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/adamholter/realtime-manim-rs/actions/workflows/ci.yml)

```sh
npm install realtime-manim
```

The published browser package is dependency-free ESM with bundled Rust/Wasm and
typed JavaScript authoring APIs. See [`packages/manim-web`](./packages/manim-web)
for a copy-paste example and [`CHANGELOG.md`](./CHANGELOG.md) for releases.

## Current state

The repository now contains:

- Cargo workspace and fixed Rust/Wasm toolchain
- versioned benchmark receipt schema with round-trip tests
- privacy-safe environment capture
- renderer-free native smoke target
- versioned retained 2D scene IR and deterministic explicit-time evaluator
- general batched Rust/Wasm/WebGPU vector renderer
- installable `realtime-manim` browser package with a typed JavaScript scene API
- multisampled GPU depth for opaque 3D geometry plus near/far-plane triangle clipping
- local OpenRouter Agent SDK server with streamed GPT-5.6 Terra Manim generation
- regular Manim Community compatibility compiler for Scene/Mobject/Animation,
  updater, moving-camera, and projected ThreeDScene semantics
- network-denied and write-isolated sandbox for generated Python
- shaped vector text with real bold/italic faces, runtime OpenType family
  registration, and mixed-style spans,
  real MathTex-to-SVG, and styled SVG ingestion with native multistop linear
  and focal radial gradients, pad/repeat/reflect spread modes, and intersected
  vector clip paths plus fractional, recursively nested alpha/luminance masks
- raster and perspective-textured images with nearest, box, bilinear, Hamming,
  bicubic, and Lanczos GPU reconstruction
- Cairo/OpenGL point clouds, static and animated compact indexed Manim surface patches, and smooth
  or textured OpenGL triangle meshes
- exact-frame WebCodecs/WebM export with Opus audio, WebVTT captions, and a
  real-time browser fallback; mathematical scene units and pixel export
  resolution are independent
- safe in-memory user asset uploads for images, SVG, audio, and fonts
- declared signal sliders for live interactive parameters
- CI skeleton

The browser tool now runs general retained scenes rather than a curve-specific shader.
Its scene algebra includes groups, circles, rectangles, lines, arrows, polylines,
arbitrary quadratic/cubic Bézier paths, shaped and mixed-style vector text, MathTex,
styled SVG with per-fragment linear and transformed radial gradients plus native
vector clip paths and GPU alpha/luminance masks, raster images, point clouds, compact 3D surfaces,
smooth/textured 3D meshes, hierarchy, object
lifetimes, transforms/styles, property and camera tracks, reusable signals, and
computed bindings with browser sliders. Rust validates and evaluates the graph at explicit time `t`,
tessellates generic geometry through lyon, batches it into one WebGPU vertex/index
stream, and owns animation timing, uploads, command encoding, submission, and
presentation.

The renderer remains an experimental lyon + wgpu backend, not the frozen final
renderer choice. Text uses Rustybuzz/OpenType outlines instead of a bitmap font;
MathTex uses Tectonic and vector SVG paths; WebCodecs exports explicitly timestamped
frames. Regular Manim Python now executes through Manim itself and compiles the
resulting projected cubic states into Rust tracks, including transforms, animation
composition, updaters, moving cameras, Cairo 3D, OpenGL VMobjects and PMobjects,
smooth lit surfaces, and perspective-textured OpenGL images. The 20-scene retained
compatibility corpus currently compiles without diagnostics and has full isolated
browser differential output. This is still not a full-Manim completion claim:
complete SVG filter edge cases, third-party renderer hooks that bypass
ShaderWrapper, compact semantic 3D/depth, broader
native-node font fallback, and the native interactive host remain open. Normal
Manim ShaderWrapper vertex/geometry/fragment programs now translate to WebGPU,
including typed arrays and attributes, textures, programmable-size points, and
dynamic shader/topology replacement.
Pure translation and uniform scale-plus-translation traces are lowered into native
Rust transform tracks instead of duplicating every Bézier coordinate at every
sampled frame. Proven shear and non-uniform affine deformations now use one native
2×3 Rust matrix track; their outlines are tessellated after the affine transform so
stroke width stays uniform, while multistop fill and stroke gradients remain fixed
in Manim world space. Repeated similarity timelines are hoisted onto shared
parent groups.
Exact and translated path instances reuse concrete geometry, and repeated property
timelines reuse one validated keyframe sequence. Non-affine morphs retain their full
geometry tracks. Provable multi-subpath `Create`, `Write`, `Uncreate`,
`DrawBorderThenFill`, and `ShowPassingFlash` traces store final geometry once and run
through Rust-owned de Casteljau draw-range evaluation.
Topology-stable Cairo `Surface` families now retain shared indexed patches while
Rust animates their control vertices, fill/stroke materials, and wire radii. Rust
reconstructs fill fans, normals, and wire geometry at render time instead of storing
expanded triangles for every sampled frame.
When shared-vertex connectivity changes, the compiler starts a new compact retained
lifetime at the exact sample instead of degrading the whole animation to expanded meshes.
Static non-fixed 3D cubic paths now remain in world coordinates and are projected by
Rust from the explicit camera state instead of being rewritten at every camera frame.
Manim `TracedPath` trails retain one shared cubic segment stream plus explicit-time
sliding windows instead of resending the full accumulated trail on every frame.
Compatible Manim morphs retain path topology once and animate compact numeric
`pathData` instead of repeating command names and coordinate keys at every sample.
Proven rotation/scale/translation paths stay completely static and use one composite
Rust `transform2d` timeline, including exact stroke compensation.
Moving `ShowPassingFlash`-style windows use one atomic Rust `drawRange` timeline
instead of independently interpolating and storing start/end tracks.
Compatible `MovingCameraScene` programs retain world-space paths and animate Rust's
camera directly instead of rewriting every object at every sampled camera frame.
Roll-free Manim 3D camera orbits retain one position timeline and derive the exact
orthogonal up-vector in Rust instead of interpolating redundant camera vectors.
Fixed-in-frame paths remain in 2D, while proven fixed-orientation families—including
single and self-animated objects—use native, animatable 3D billboard anchors projected
by Rust instead of baking camera projection into their geometry. Billboard children
may also change path topology while the camera moves; their full command morph remains
local to the live projected anchor.
Opaque meshes, textured meshes, and compact surfaces now carry projected depth
into a real 4x-MSAA WebGPU depth attachment; they no longer need CPU triangle sorting, and partial
near/far-plane crossings are clipped instead of dropping the entire triangle.
Compatible mixed-media `MovingCameraScene` programs keep vector paths, linear
gradient endpoints, image corners, and point positions in world space; one native camera timeline drives all
of them, while Cairo point sprites retain their fixed screen-space radius.
The current 20-scene corpus totals 2,717,974 bytes with zero diagnostics and renders
20/20 through isolated WebGPU; shared-keyframe output is pixel exact against its
unshared baseline across the full regression sweep.

## Agent animation studio

The local app now implements the complete prototype flow:

1. enter an OpenRouter key and optionally attach images, SVG, audio, or fonts,
2. describe an animation,
3. watch GPT-5.6 Terra stream real Manim Community Python,
4. let the sandboxed compiler execute Manim semantics and emit validated Rust IR,
5. run the compiled program in the Rust/Wasm/WebGPU renderer,
6. scrub any exact source frame with the live timeline, with pause/resume continuing
   from that frame,
7. when the Manim program has a monotonic user-authored `ValueTracker`, manipulate
   that value directly with a synchronized semantic slider,
8. inspect agent/compile/render/total timing, then play or download the WebM video.

The server uses the official `@openrouter/agent` package and the model
`openai/gpt-5.6-terra`. OpenRouter calls are server-side only. The supplied key is
held in server memory behind an HttpOnly localhost session cookie, never written to
disk or browser storage, and expires after eight hours.

Generated programs define one `GeneratedScene` using normal Manim Community Python.
They execute inside `sandbox-exec` with network denied, writes confined to one
temporary directory, and reads denied under the user's home and mounted volumes except
the isolated Manim environment, compiler, and fonts. The compatibility compiler
emits the versioned retained-graph contract; JavaScript and Rust validate that IR
again. Model-generated JavaScript and shaders are never evaluated.

Verified held-out requests include a Pareto-frontier explanation and a Pythagorean
visual proof. The Pareto scene used 31 nodes across eight node types with 32 animation
tracks; the proof used 21 nodes across six node types with 27 tracks. Both produced
playable five-second WebM output through the live local application.

## Run the web app

From the repository root:

```sh
./scripts/serve-web.sh
```

Then open <http://127.0.0.1:8917>. The script builds the release Wasm bundle before
starting the local agent server. Run `npm install` once before the first launch.

Pause freezes the retained timeline. Run code records the already compiled Manim
scene again. Direct retained-IR scenes can still declare live sliders backed by Rust
signal overrides and address individual nodes, groups, camera properties, and
signal-bound properties without changing the renderer.

To build without starting the server:

```sh
./scripts/build-web.sh
```

This requires the `wasm-bindgen` CLI at the same version as the crate:

```sh
brew install wasm-bindgen
```

## Browser package

`packages/manim-web` is a dependency-free ESM package containing the typed
JavaScript scene builder and the release Rust/Wasm runtime. Build and verify it with:

```sh
npm run build:package
npm run test:package
```

The public package is installable with `npm install realtime-manim`. The package
README and `example/index.html` show direct browser use with `Scene`, `Circle`,
`Text`, `Create`, `MoveTo`, playback, pausing, seeking, and live signals. It also
accepts the complete retained scene JSON produced by the regular-Manim compiler.

## Toolchain

The workspace pins Rust `1.88.0` and `wasm32-unknown-unknown` in `rust-toolchain.toml`.
On a Homebrew-based Mac, expose the keg-only Rustup shims:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
rustup show active-toolchain
```

CI and other Rustup installations read the same toolchain file automatically.

## Verification

Run from the repository root:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"

cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p realtime-manim-native-preview
cargo check -p realtime-manim-web-preview --target wasm32-unknown-unknown
cargo clippy -p realtime-manim-web-preview --target wasm32-unknown-unknown -- -D warnings
cargo run -p realtime-manim-env-capture
```

Expected smoke output:

```text
realtime-manim native preview scaffold
renderer: unselected
```

Browser proof evidence is recorded in `docs/receipts/P-12.md`.
The Manim authoring decision and compatibility evidence are recorded in
`docs/receipts/R-14.md`.

Environment capture prints JSON to stdout. It collects only benchmark-relevant, whitelisted facts—never serial numbers, UUIDs, hostnames, usernames, paths, or process lists. Set `REALTIME_MANIM_LOAD_PROFILE` to a deliberate label such as `chrome-slack-codex` when recording a real benchmark.

## Benchmark receipts

- Schema: `benchmarks/schema/benchmark-result.schema.json`
- Round-trip fixture: `benchmarks/samples/smoke.json`
- Current sanitized machine manifest: `benchmarks/environment/current.json`

Every later performance claim must include:

1. the exact commands,
2. target revision and dirty state,
3. sanitized environment/load profile,
4. workload and resolution,
5. raw metric samples and statistic names,
6. trace/diff artifact paths.

Do not publish an isolated best-case result as the user-facing number.
