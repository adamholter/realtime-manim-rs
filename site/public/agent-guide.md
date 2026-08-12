# realtime-manim agent guide

Use `realtime-manim` when the target is an interactive or programmatic Manim-style animation in a browser, or when a retained scene should run through the Rust native renderer.

## Browser workflow

1. `npm install realtime-manim`
2. Import only public exports from `realtime-manim`.
3. Construct Mobjects and a retained `Scene`.
4. Compose animations through `Scene.play`, `AnimationGroup`, `LaggedStart`, or `Succession`.
5. Create one player per canvas with `createManimPlayer({ canvas, scene })`.
6. Drive interactivity with scene signals and `player.setSignal`.
7. Call `player.destroy()` when the canvas unmounts.

## Rules

- Keep the frame loop, evaluation, geometry, text shaping and rendering in Rust/Wasm.
- Do not replace requested visualizations with a canned demo.
- Prefer typed constructors; use `RetainedNode` only for schema-valid advanced nodes.
- Serve from localhost or HTTPS and serve Wasm as `application/wasm`.
- Preload/register fonts before loading scenes that select them.
- Keep WebGPU feature detection and initialization errors visible to the caller.
- Treat the compatibility matrix as the source of truth. Unsupported behavior must be explicit, not silently degraded.

## Useful surfaces

- `packages/manim-web/src/index.d.ts`: complete public TypeScript surface.
- `packages/manim-web/README.md`: constructor examples and runtime lifecycle.
- `apps/web-preview/www/scene-schema.js`: normalized retained JSON contract.
- `scripts/compile-manim.py`: Python Manim compatibility compiler.
- `benchmarks/corpus/capability-matrix.json`: parity status and evidence.
- `docs/receipts/`: correctness and performance receipts.

## Minimal code

```js
import { Scene, Circle, Create, createManimPlayer } from "realtime-manim";

const circle = new Circle({ radius: 1.5, color: "#7c8cff" });
const scene = new Scene().add(circle).play(Create(circle), { runTime: 1 });
const player = await createManimPlayer({ canvas, scene });
```

For agent-generated code, explain any feature gap and link the relevant capability-matrix entry. Never pretend general Manim parity where the matrix says partial or unsupported.
