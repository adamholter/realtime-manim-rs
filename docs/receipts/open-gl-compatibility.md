# OpenGL compatibility receipt

Updated: 2026-08-10

The regular-Manim compiler and Rust/WebGPU runtime now retain the core OpenGL
families instead of rasterizing them or substituting unrelated geometry:

- OpenGL VMobjects: cubic paths, paint, transforms, and animated camera projection
- OpenGL PMobjects: per-point color, point radius, perspective scaling, and animation
- OpenGL surfaces: triangle topology, animated vertices, smooth normals, gloss,
  shadow, camera-relative point lighting, and painter-sorted depth
- OpenGL image/textured surfaces: embedded RGBA pixels, UVs, perspective deformation,
  nearest/bilinear/bicubic sampling, dual light/dark texture mixing, gloss, and
  shadow
- plugin-defined indexed OpenGL geometry: structured shader positions/colors,
  triangle lists/strips/fans, transforms, animated vertices, and vertex colors
- arbitrary ShaderWrapper programs: translated vertex/fragment semantics,
  transform-feedback geometry stages, typed uniforms/attributes, textures,
  programmable point sizes, and dynamic program/topology replacement
- dynamic retained lifetimes for changing surface topology/material/UVs,
  point count, image pixels/resampling, and remove/re-add cycles
- six GPU reconstruction filters for images and textured meshes: nearest, box,
  bilinear, Hamming, bicubic, and Lanczos

Differential final-frame results at 854×480 against Manim Community 0.20.1:

| Scene | Diagnostics | RMSE |
|---|---:|---:|
| `OpenGLCompatibility` | 0 | 0.014546 |
| `OpenGLPointsCompatibility` | 0 | 0.006485 |
| `OpenGLSurfaceCompatibility` | 0 | 0.018022 |
| `OpenGLImageCompatibility` | 0 | 0.009858 |
| `OpenGLDarkTextureCompatibility` | 0 | 0.007644 |
| `OpenGLPluginCompatibility` | 0 | 0.007791 |
| `DynamicOpenGLColorCompatibility` | 0 | 0.008350 |
| `DynamicOpenGLPointColorCompatibility` | 0 | 0.001291 |

All Rust frames were rendered through isolated headless Chrome/WebGPU with no
page or console errors. References came from Manim's real `--renderer=opengl`
output. Artifacts live under `benchmarks/compat/`.

This closes the core OpenGL object families and Manim's normal ShaderWrapper
contract, including user-supplied fragment logic. Renderer plugins that bypass
ShaderWrapper remain the explicit adapter boundary.
