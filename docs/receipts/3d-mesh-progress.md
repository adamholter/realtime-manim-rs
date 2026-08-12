# 3D mesh runtime progress

Updated: 2026-07-30

## Delivered

- arbitrary indexed triangle-mesh node in scene IR `2`
- 3D translation, X/Y/Z rotation, and nonuniform scale tracks
- perspective camera with position, target, up, FOV, near/far planes
- configurable directional light and ambient term
- back-face culling or double-sided shading
- per-frame triangle depth sorting
- mixed 3D mesh and fixed 2D vector/text layers
- JavaScript and Rust structural validation, including triangle bounds

## Live verification

An eight-vertex, twelve-triangle cube was animated through X/Y rotation in the
Rust/Wasm renderer.

- Chrome WebGPU: `74 fps`
- page errors: `0`
- console errors: `0`
- overflow: `0`
- visual receipt: `docs/receipts/rust-3d-mesh.png`

## Honest boundary

This is foundational 3D, not the Manim 3D tier. It uses CPU perspective projection
and painter-sorted triangles; it does not yet have a GPU depth buffer, near-plane
triangle clipping, mesh-group transform composition, surface/parametric helpers,
normals supplied by authors, wireframes, transparent-order guarantees, or Manim's
`ThreeDScene`, fixed-orientation, and fixed-in-frame APIs.
