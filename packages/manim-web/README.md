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
  } from "https://cdn.jsdelivr.net/gh/adamholter/realtime-manim-rs@v0.6.1/packages/manim-web/src/index.js";

  const circle = new Circle().fill("#58c4ddcc");
  const scene = new Scene().add(circle).play(Create(circle));
  await createManimPlayer({ canvas: document.querySelector("#manim"), scene });
</script>
```

Do not open the HTML as `file://`; serve it locally (for example, `npx serve .`) so the browser can load Wasm. See [`example/index.html`](./example/index.html) for a complete play/pause/scrub demo. When running that example from this repository it imports `../src/index.js`; replace that line with the CDN URL to run it elsewhere.

## Scene API

Objects use chainable transforms and styles. The authored API includes:

- Geometry: `Circle`, `Dot`, `Ellipse`, `Rectangle`, `RoundedRectangle`, `Square`, `Line`, `Arrow`, `Polyline`, `Polygon`, `RegularPolygon`, and `Triangle`.
- Coordinate systems and graphs: `NumberLine`, `Axes`, `NumberPlane`, `FunctionGraph`, and `ParametricFunction`.
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

`MarkupText` accepts Pango markup directly. Parsing, Unicode bidi resolution,
font selection, shaping, and vector decoration geometry stay in Rust/Wasm:

```js
const label = new MarkupText(
  `<span foreground="cyan" background="#11223388" weight="bold"
     rise="2pt" letter_spacing="512" underline="double">
     سرعة &amp; speed
   </span>`,
);
```

Supported convenience elements are `b`, `i`, `u`, `s`, `big`, `small`, `sup`,
and `sub`. Supported `span` attributes are `foreground`/`color`, `background`,
`font_family`, `weight`, `style`, `size`, `rise`, `letter_spacing`, `underline`,
`underline_color`, `strikethrough`, and `strikethrough_color`. Invalid nesting,
unknown tags, unknown attributes, unsupported named colors, and unsupported
Pango features throw exact load-time errors; they are never flattened.

### Spatial layout

The builder has geometry-aware Manim-style layout methods. Bounds compose each
retained parent transform without flattening the hierarchy; arranging a group
moves only its direct child roots and leaves the group's own transform intact:

```js
import {
  DOWN, RIGHT, UL, Circle, Rectangle, VGroup,
} from "realtime-manim";

const cards = new VGroup(
  new Circle({ radius: 0.8 }),
  new Rectangle({ width: 3, height: 1.4 }),
  new Circle({ radius: 0.8 }),
)
  .arrange(RIGHT, { buff: 0.4, alignedEdge: DOWN })
  .toCorner(UL, 0.6);

const focus = cards.copy() // deep hierarchy copy with collision-safe fresh ids
  .nextTo(cards, DOWN, 0.5)
  .setX(0);

const bounds = cards.getBounds();
console.log(bounds.center, bounds.width, bounds.height);
```

`getBounds()`, `getCenter()`, `getWidth()`, `getHeight()`, `getLeft()`,
`getRight()`, `getTop()`, `getBottom()`, and `getCriticalPoint(direction)`
return scene-coordinate geometry. Positioning methods are chainable:
`center()`, `moveTo(target, alignedEdge)`, `setX()`, `setY()`, `alignTo()`,
`nextTo()`, `toEdge()`, and `toCorner()`. Like Manim, `moveTo` aligns the
object's center by default or aligns matching critical points when an edge such
as `UL` is supplied; it does not assign the raw retained transform origin.
Direction constants include `UP`, `DOWN`, `LEFT`, `RIGHT`, `UL`,
`UR`, `DL`, and `DR`; the default frame is `FRAME_WIDTH` by `FRAME_HEIGHT`
(16 by 9). A custom `{ width, height, center }` frame may be passed to edge and
corner placement. Readonly tuples such as `as const` are accepted by the
TypeScript API.

Bounds are exact for 2D circles, rectangles, lines/arrows, polylines, Bezier
paths, trace-path geometry, raster corners, point clouds with world-space
radii, and nested groups. Text shaping, parsed SVG viewport geometry,
camera-projected 3D nodes, billboards, screen-space point radii, path
references, and unknown retained node kinds deliberately throw instead of
guessing. Query a containing `Group` when a node uses an explicit retained
`parent` id so the parent transform is available.

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
])
  .stroke("#58c4ddff", 0.12)
  .strokeCap("round")
  .strokeJoin("round")
  .dash([0.45, 0.18], -0.1);
```

Stroke caps (`butt`, `square`, `round`), joins (`miter`, `miterClip`, `round`,
`bevel`), signed dash offsets, and odd-length SVG-style dash patterns are
tessellated natively in Rust on straight or curved paths.

Build reusable coordinate systems from retained primitives. Ranges are
`[minimum, maximum, tickStep]`; `c2p`/`p2c` and `n2p`/`p2n` remain reversible
after moving, rotating, or scaling the coordinate system:

```js
const axes = new Axes({
  xRange: [-6, 6, 1],
  yRange: [-3, 3, 1],
  xLength: 12,
  yLength: 6,
  includeNumbers: true,
  xLabel: "x",
  yLabel: "f(x)",
});

const graph = axes.plot((x) => Math.sin(x), [-Math.PI * 2, Math.PI * 2], {
  samples: 401,
  id: "sine",
});

const scene = new Scene()
  .add(axes, graph)
  .play(Create(axes), Create(graph));

const scenePoint = axes.c2p(Math.PI, 0);
const [x, y] = axes.p2c(scenePoint);
```

Plots are ordinary retained `Path` nodes, not special-case demo scenes.
Non-finite samples create separate subpaths; pass known discontinuities to
prevent interpolation across asymptotes. `ParametricFunction` and
`axes.plotParametric(...)` accept functions returning `[x, y]`:

```js
const reciprocal = axes.plot((x) => 1 / x, [-4, 4], {
  discontinuities: [0],
  samples: 501,
});

const circle = axes.plotParametric(
  (t) => [Math.cos(t), Math.sin(t)],
  [0, Math.PI * 2],
  { samples: 257 },
);

const plane = new NumberPlane({
  xRange: [-8, 8, 1],
  yRange: [-4, 4, 1],
  gridSubdivisions: 2,
});
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
If the selected face lacks a grapheme, the runtime resolves registered families
in deterministic registration order and shapes contiguous fallback runs without
splitting combining sequences. Missing vector coverage is an explicit load/render
error instead of tofu or disappearing glyphs.

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

Version 0.6 requires a WebGPU browser and a server that serves `.wasm` as `application/wasm`. Call `destroy()` when a player is no longer needed so its animation loop and GPU resources are released deterministically. Native UAX #9 bidi layout, Arabic joining, Hebrew, grapheme-safe registered-family fallback, and strict nested Pango-style markup are deterministic. Color emoji, exhaustive complex-script golden parity, unusual SVG filters, third-party Python renderer hooks, and custom-shader rendering are not yet equivalent to desktop Manim.
