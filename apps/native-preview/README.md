# Native preview

This binary is a real retained-scene player for macOS. Rust loads and validates the same scene JSON as the browser package, evaluates it at explicit time, tessellates vector geometry and shaped OpenType text, and submits the frame through `wgpu`'s Metal backend.

```bash
# Non-interactive validation; safe for CI.
cargo run -p realtime-manim-native-preview -- check path/to/scene.json --time 1.5

# Real GPU render/readback check; safe for CI on a GPU host.
cargo run -p realtime-manim-native-preview -- smoke path/to/scene.json --time 1.5

# Render one explicit-time PNG without opening a window.
cargo run -p realtime-manim-native-preview -- render path/to/scene.json \
  --time 1.5 --output frame.png

# Strict proof covering RGBA images, lit/color meshes, textured meshes,
# surfaces, depth, culling, transparency, near-plane clipping, and Path3d.
cargo run -p realtime-manim-native-preview -- render \
  apps/native-preview/media-3d-proof.json --strict --time 2 \
  --output /tmp/realtime-manim-native-media-3d.png

# Real-time looping native window. Omitting the scene path uses the bundled demo.
cargo run -p realtime-manim-native-preview -- play path/to/scene.json
cargo run -p realtime-manim-native-preview -- play
```

The native renderer draws animated circles, rounded rectangles, lines, arrows, polylines, Bézier paths, `Path3d`, point clouds, `Text`, `MarkupText`, RGBA images, colored/lit meshes, light/dark textured meshes, and patch surfaces. The Metal path includes hierarchical 2D transforms, animated 3D node transforms, perspective cameras, near/far clipping, back-face culling, an actual depth buffer for opaque geometry, far-to-near transparent triangles, per-vertex colors/normals, gloss/shadow lighting, surface wire strokes, and nearest/box/bilinear/Hamming/bicubic/Lanczos texture sampling. Scene textures are retained by exact source signature and streaming geometry uses growable buffers, so warmed playback does not recreate scene buffers or textures per frame.

`--strict` turns any unsupported visible node into an exact error instead of a warning. SVG and custom shader meshes remain browser-only and are not silently claimed by the native renderer.

Interactive controls: Space pauses, Left/Right seek by 0.25 seconds, Home or R restarts, and Escape quits. Running the binary with no command prints help and exits, so an accidental CI invocation never opens a window or hangs.
