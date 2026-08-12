# Full public API compatibility sweep

Updated: 2026-08-10

Pinned reference: Manim Community 0.20.1.

## Public mobjects

- 186 public Cairo/OpenGL mobject classes audited
- 183 constructed with deterministic meaningful fixtures
- 180 visual classes compiled into retained Rust scene IR in 16 batches
- all 180 rendered through isolated headless Chrome/WebGPU
- zero compiler diagnostics, scene validation errors, JavaScript errors, shader
  warnings, WebGPU validation errors, or blank sweep batches
- three remaining constructors fail in Manim itself:
  `ArrowTip` (intentional base), `OpenGLElbow` (constructor color bug), and
  `OpenGLRoundedRectangle` (NumPy generator bug)
- `VectorizedPoint`, `OpenGLVectorizedPoint`, and `OpenGLPoint` are intentionally
  invisible positioning helpers; their no-output semantics are retained

Machine-readable evidence:

- `benchmarks/corpus/manim-constructor-audit.json`
- `docs/receipts/constructor-sweep.json`
- `benchmarks/compat/constructor-sweep/`

## Public animations

- 74 public Animation classes audited
- 70 construct, begin, interpolate, and finish in Manim 0.20.1
- all 70 execute through real `Scene.play`, compile without diagnostics in nine
  retained batches, and render at 70 class-specific WebGPU sample times
- four remaining classes fail in Manim itself:
  `ShowPartial` (intentional subclass base),
  `ApplyPointwiseFunctionToCenter` (missing super argument),
  `SmoothedVectorizedHomotopy` (missing runtime import), and
  `TransformAnimations` (family alignment failure)

Machine-readable evidence:

- `benchmarks/corpus/manim-animation-audit.json`
- `docs/receipts/animation-sweep.json`
- `benchmarks/compat/animation-sweep/`

## Public Scene subclasses

All seven public Scene families compile and render with behavioral fixtures:

- `Scene`
- `MovingCameraScene`
- `ThreeDScene`
- `SpecialThreeDScene`
- `VectorScene`
- `LinearTransformationScene`
- `ZoomedScene`

The pinned release's `SpecialThreeDScene.__init__` reads `self.renderer` before
`Scene.__init__`; its behavioral fixture normalizes that upstream ordering bug
and exercises the class's 3D camera helpers.

Machine-readable evidence:

- `docs/receipts/scene-subclasses.json`
- `benchmarks/compat/scene-subclasses/`

## Boundary

The remaining public exports are renderer/camera/config/color/value
infrastructure, enums, templates, or exception types rather than authorable
scene objects. Their visible behavior is exercised through the scene,
constructor, animation, camera, text/TeX, color, export, and renderer suites.

Plugin-supplied ShaderWrapper vertex, geometry, and fragment semantics are now
translated and executed as WebGPU programs, including textures, typed arrays,
matrix attributes, programmable point size, and dynamic program/topology
replacement. Third-party renderers that bypass Manim's ShaderWrapper contract
remain the explicit adapter boundary.
