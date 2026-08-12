# `realtime-manim`

Build and scrub Manim-style animations directly in the browser. Scenes are described in JavaScript, then evaluated and rendered by the included Rust/Wasm + WebGPU runtime.

## Install

```sh
npm install realtime-manim
```

```html
<canvas id="manim" width="1280" height="720"></canvas>
<script type="module">
  import {
    Circle, Create, MoveTo, Scene, Text, createManimPlayer,
  } from "realtime-manim";

  const dot = new Circle({ radius: 1 })
    .fill("#58c4ddcc")
    .stroke("#ffffff", 0.06)
    .moveTo(-4, 0);

  const scene = new Scene({ background: "#080b12" })
    .add(dot, new Text("Hello, WebGPU").moveTo(0, 2.5))
    .play(
      Create(dot, { duration: 0.8 }),
      MoveTo(dot, [4, 0], { from: [-4, 0], duration: 3 }),
    );

  const player = await createManimPlayer({
    canvas: document.querySelector("#manim"),
    scene,
  });

  // The runtime is seekable, so normal DOM controls can drive it.
  player.pause();
  player.seek(1.5);
</script>
```

Your bundler copies the package's Wasm runtime automatically through the module-relative URL. Serve the app over `http://localhost` or HTTPS; WebGPU is unavailable from ordinary insecure origins.

## No-build browser demo

For a plain HTML file, import the module from a CDN. Pin the version in production:

```html
<canvas id="manim" width="1280" height="720"></canvas>
<script type="module">
  import {
    Circle, Create, Scene, createManimPlayer,
  } from "https://cdn.jsdelivr.net/npm/realtime-manim@0.3.0/src/index.js";

  const circle = new Circle().fill("#58c4ddcc");
  const scene = new Scene().add(circle).play(Create(circle));
  await createManimPlayer({ canvas: document.querySelector("#manim"), scene });
</script>
```

Do not open the HTML as `file://`; serve it locally (for example, `npx serve .`) so the browser can load Wasm. See [`example/index.html`](./example/index.html) for a complete play/pause/scrub demo. When running that example from this repository it imports `../src/index.js`; replace that line with the CDN URL to run it elsewhere.

## Scene API

Objects use chainable transforms and styles. The authored API includes:

- Geometry: `Circle`, `Dot`, `Ellipse`, `Rectangle`, `RoundedRectangle`, `Square`, `Line`, `Arrow`, `Polyline`, `Polygon`, `RegularPolygon`, and `Triangle`.
- Curves: `Path`, `Path3D`, `TracePath`, `Arc`, `QuadraticBezier`, and `CubicBezier`, with typed `pathCommand` and `pathCommand3D` builders.
- Composition and retained media: `Group`, `VGroup`, `Text`, `MarkupText`, `MathTex`, `SVG`, `Image`, `PointCloud`, `DotCloud`, `Mesh`, `Surface`, `Billboard`, and `PathReference`.
- Animation: `Create`, `FadeIn`, `FadeOut`, `MoveTo`, `Rotate`, `Transform`, `ReplacementTransform`, `MoveCamera`, `OrbitCamera`, and lower-level `animate(...)` tracks.
- Timing composition: `AnimationGroup`, `LaggedStart`, and `Succession`, including whole-play `runTime` scaling.

Groups serialize as native retained parent hierarchies, so their transforms stay in Rust instead of being recomputed in JavaScript:

```js
const triangle = new Triangle().fill("#58c4ddcc");
const curve = new CubicBezier([-2, 0], [-1, 2], [1, -2], [2, 0]);
const diagram = new Group([triangle, curve], { id: "diagram" }).moveTo([2, 1]);

const scene = new Scene()
  .add(diagram)
  .play(Create(diagram), MoveCamera({ zoom: 1.4 }, { duration: 2 }));
```

Animations passed to one `Scene.play(...)` start together. Compositions make the relationship explicit, and `runTime` rescales the complete schedule rather than changing only one track:

```js
const left = new Circle({ id: "left" }).moveTo([-3, 0]);
const right = new Circle({ id: "right" }).moveTo([3, 0]);

const scene = new Scene()
  .add(left, right)
  .play(LaggedStart(
    MoveTo(left, [0, 1], { from: [-3, 0] }),
    MoveTo(right, [0, -1], { from: [3, 0] }),
    { lagRatio: 0.25, runTime: 3 },
  ))
  .play(Succession(
    FadeOut(left),
    FadeOut(right),
  ));
```

`Transform(source, target)` interpolates transforms, styles, and compatible retained geometry such as circle radii, point arrays, and path commands. It rejects pairs that the runtime cannot interpolate faithfully instead of silently cross-fading or substituting a canned morph. `ReplacementTransform` uses the same interpolation, then schedules an exact retained-tree swap; only the source needs to be added first:

```js
const small = new Circle({ id: "small", radius: 0.5 }).fill("#58c4ddff");
const large = new Circle({ id: "large", radius: 2 }).fill("#fc6255ff");

const scene = new Scene()
  .add(small)
  .play(ReplacementTransform(small, large, { duration: 1.5 }));
```

Build arbitrary native paths without writing retained JSON by hand:

```js
const curve = new Path([
  pathCommand.moveTo([-3, 0]),
  pathCommand.quadTo([0, 3], [3, 0]),
]);
```

Raster and 3D constructors serialize directly to the Rust retained schema. `Image` accepts exact RGBA bytes (or canonical base64); `Image.fromSource(...)` decodes a URL, canvas, bitmap, image, or video through the browser. `Mesh` supports per-vertex color/normal data and optional RGBA textures, while `Surface` preserves arbitrary polygon patches:

```js
const pixels = new Uint8Array([255, 80, 70, 255]);
const image = new Image(pixels, 1, 1, { resampling: "nearest" });

const mesh = new Mesh(
  [[-1, -1, 0], [1, -1, 0], [0, 1, 0]],
  [[0, 1, 2]],
  { colors: ["#ff0000ff", "#00ff00ff", "#0000ffff"], doubleSided: true },
);
```

LaTeX is compiled once to vector SVG and then rendered entirely by Rust/WebGPU. Point `endpoint` at a compatible service that accepts `{ tex }` and returns `{ svg }`; this repository's local tool exposes `/api/typeset`:

```js
const equation = await MathTex.typeset(String.raw`e^{i\pi} + 1 = 0`, {
  fontSize: 1.4,
  endpoint: "/api/typeset",
});
scene.add(equation);
```

OpenType font files can be registered before the initial scene loads. The bytes
stay inside that player's Rust text engine and survive WebGPU device recovery:

```js
const fontData = await fetch("/fonts/ExampleSans-Regular.ttf").then((response) => response.arrayBuffer());
const label = new Text("Real vector outlines", { fontFamily: "Example Sans" });

const player = await createManimPlayer({
  canvas,
  scene: new Scene().add(label),
  fonts: [{ family: "Example Sans", data: fontData }],
});

// Register another face later, before loading text that selects it.
player.registerFont("Example Sans", boldFontData, { weight: "bold" });
```

Register regular, bold, italic, and bold-italic faces separately when available.
If a selected variant is missing, the runtime falls back to another face in the
same registered family; it does not silently switch to a different family.

`RetainedNode.from(node)` is the typed, mutation-isolated escape hatch for newer retained node kinds such as custom shaders. Its payload is still checked by the Rust scene validator when the player loads it; the higher-level constructors validate Image, Mesh, Surface, and TracePath topology immediately in JavaScript.

`createManimPlayer(...)` returns controls for `play()`, `pause()`, `seek(seconds)`, `time()`, `load(scene)`, `setSignal(id, value)`, sizing, validation, deterministic evaluation, and cleanup. The player accepts a `Scene`, retained scene-data object, or JSON string.

Live values use the same retained expression engine:

```js
const circle = new Circle();
const scene = new Scene()
  .add(circle)
  .signal("radius", [{ at: 0, value: 1 }])
  .bind(circle, "radius", { op: "signal", id: "radius" })
  .control("radius", { min: 0.25, max: 3, default: 1 });

const player = await createManimPlayer({ canvas, scene });
slider.oninput = () => player.setSignal("radius", Number(slider.value));
```

Colors use six- or eight-digit hex (`#rrggbb` or `#rrggbbaa`). Coordinates use a 16:9 scene by default, centered at `[0, 0]`.

Each player owns its canvas, WebGPU renderer, clock, signals, and animation-frame loop. Multiple players can run concurrently and destroying one does not interrupt the others:

```js
const [left, right] = await Promise.all([
  createManimPlayer({ canvas: leftCanvas, scene: leftScene }),
  createManimPlayer({ canvas: rightCanvas, scene: rightScene }),
]);
left.destroy(); // right keeps rendering
```

## Compatibility and current limits

This is a Manim-compatible browser runtime, not a drop-in execution environment for arbitrary Python Manim code. It accepts retained scene JSON emitted by the project's compatibility compiler and includes the lightweight JavaScript builder shown above. Playback, seeking, signals, text, SVG, raster media, 3D scene data, custom shaders, masks, patterns, audio metadata, and deterministic evaluation stay in the Rust runtime.

Version 0.3 requires a WebGPU browser and a server that serves `.wasm` as `application/wasm`. Call `destroy()` when a player is no longer needed so its animation loop and GPU resources are released deterministically. Automatic cross-family Unicode fallback, complex-script fallback chains, color emoji, unusual SVG filters, third-party Python renderer hooks, and some native 3D optimizations are not yet equivalent to desktop Manim.
