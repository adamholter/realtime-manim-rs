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

# Real-time looping native window. Omitting the scene path uses the bundled demo.
cargo run -p realtime-manim-native-preview -- play path/to/scene.json
cargo run -p realtime-manim-native-preview -- play
```

The native renderer currently draws animated circles, rounded rectangles, lines, arrows, polylines, Bézier paths, point clouds, `Text`, and `MarkupText`, including hierarchical transforms, camera transforms, solid paints, linear gradients, fill/stroke opacity, and partial stroke ranges. `--strict` turns any unsupported visible node into an error instead of a warning. SVG, images, 3D geometry, and custom shaders remain implemented by the browser renderer but are not silently claimed by this native subset.

Interactive controls: Space pauses, Left/Right seek by 0.25 seconds, Home or R restarts, and Escape quits. Running the binary with no command prints help and exits, so an accidental CI invocation never opens a window or hangs.
