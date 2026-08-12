# OpenGL custom-shader compatibility

Updated: 2026-08-10

## Result

Plugin-defined Manim OpenGL programs now remain executable shader programs in
the Rust/WebGPU runtime. They are not replaced with a canned primitive, bitmap,
or video:

- GLSL 3.30 vertex and fragment stages are normalized to explicit Vulkan-style
  locations and bindings, translated to WGSL by Naga, and compiled by WebGPU.
- sampler2D uniforms become WebGPU texture/sampler pairs.
- scalar, vector, matrix, signed/unsigned integer, boolean, and fixed-size
  array uniforms retain their typed std140 GPU representation and may change
  over the animation.
- float, signed-integer, unsigned-integer, and matrix-column vertex attributes
  retain their declared layouts.
- flat/smooth/noperspective interpolation qualifiers are retained.
- depth-tested and 4x-MSAA custom pipelines share the runtime depth target.
- desktop-only geometry stages are executed through headless OpenGL transform
  feedback at compile time; their point, line-strip, or triangle-strip output
  is retained as animated typed vertex buffers for WebGPU.
- vertex or geometry stages that emit programmable-size points are lowered to
  pixel-accurate triangle quads, with `gl_PointCoord` fragment behavior intact.
- shader-program, topology, attribute-layout, uniform-layout, and buffer-size
  changes split into exact retained lifetimes rather than silently reusing an
  incompatible GPU node.
- custom shader folders attached to ordinary OpenGLMobjects, OpenGLSurfaces,
  textured surfaces, and plugin-defined OpenGLVMobjects all enter this path.

The build-time translator is
`tools/glsl-to-wgsl`; the Manim annotation/lowering pass is
`scripts/translate-manim-shader.py`.

## Differential evidence

All images are 854x480 midframes. Normalized ImageMagick RMSE is measured
against Manim Community 0.20.1's OpenGL renderer.

| Trial | Capability | Normalized RMSE | Browser errors |
|---|---|---:|---:|
| `opengl-custom-shader` | arbitrary vertex/fragment, animated int/bool uniforms | 0.030196 | 0 |
| `opengl-dark-texture-shader` | exact translated Manim textured-surface program | 0.006482 | 0 |
| `opengl-geometry-shader` | transform-feedback geometry stage, flat int varying | 0.008701 | 0 |
| `opengl-vmobject-custom-shader` | custom geometry program on OpenGLVMobject | 0.002476 | 0 |
| `opengl-integer-attribute-shader` | typed signed-int vertex attribute | 0.009529 | 0 |

The remaining pixel differences are rasterization and edge-antialiasing
differences; every expected object, color region, texture, and shader-generated
surface is present.

Machine-readable/source evidence:

- `benchmarks/compat/opengl-custom-shader.json`
- `benchmarks/compat/opengl-geometry-shader.json`
- `benchmarks/compat/opengl-vmobject-custom-shader.json`
- `benchmarks/compat/opengl-integer-attribute-shader.json`
- `benchmarks/compat/opengl-dark-texture-shader.json`
- `benchmarks/compat/custom_shader/`
- `benchmarks/compat/opengl-programmable-point-size.json`
- `benchmarks/compat/opengl-geometry-point-size.json`
- `benchmarks/compat/opengl-uniform-array-shader.json`
- `benchmarks/compat/opengl-matrix-attribute-shader.json`
- `benchmarks/compat/opengl-dynamic-geometry-shader-topology.json`

## Tests

- Rust scene validation and typed-buffer interpolation: 9 passing tests.
- JavaScript schema validation: 18 passing tests.
- isolated Chrome/WebGPU rendering: zero page, console, shader, or WebGPU
  validation errors across 22 OpenGL/custom-shader fixtures.

## Remaining extension frontier

This closes Manim's normal ShaderWrapper vertex/fragment/geometry, typed-buffer,
uniform, and texture contract, including dynamic replacement. Third-party
renderers that bypass ShaderWrapper still need a renderer-specific adapter;
sampler arrays are not part of Manim's normal wrapper mapping and remain an
extension case.
