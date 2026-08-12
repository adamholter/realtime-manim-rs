import assert from "node:assert/strict";
import test from "node:test";
import {
  AnimationGroup,
  Arc,
  Billboard,
  Circle,
  Create,
  CubicBezier,
  DotCloud,
  FadeIn,
  FadeOut,
  Group,
  Image,
  MarkupText,
  LaggedStart,
  MathTex,
  Mesh,
  MoveCamera,
  MoveTo,
  OrbitCamera,
  Path,
  Path3D,
  Polygon,
  RegularPolygon,
  ReplacementTransform,
  RetainedNode,
  Scene,
  Square,
  Text,
  Transform,
  Triangle,
  Surface,
  TracePath,
  Succession,
  animate,
  pathCommand,
  pathCommand3D,
} from "../src/index.js";

test("builds a deterministic browser scene", () => {
  const circle = new Circle({ id: "circle", radius: 1.2 }).fill("#3366ffff").moveTo(-2, 0);
  const label = new Text("Rust + WebGPU", { id: "label" }).moveTo(0, -2);
  const scene = new Scene({ title: "Package test", duration: 0.001 })
    .add(circle, label, new Square({ id: "square", size: 1.5 }))
    .play(Create(circle, { duration: 0.5 }), MoveTo(circle, [2, 0], { from: [-2, 0], duration: 1 }))
    .play(FadeOut(label, { duration: 0.25 }));
  const data = scene.toJSON();
  assert.equal(data.nodes.length, 3);
  assert.equal(data.tracks.length, 4);
  assert.equal(data.tracks[3].keyframes[0].at, 1);
  assert.equal(data.duration, 1.25);
  assert.match(scene.toString(), /Rust \+ WebGPU/);
});

test("returns copies instead of mutable internals", () => {
  const circle = new Circle({ id: "safe" });
  const scene = new Scene().add(circle);
  const first = scene.toJSON();
  first.nodes[0].id = "changed";
  assert.equal(scene.toJSON().nodes[0].id, "safe");
});

test("preserves imported retained scenes", () => {
  const source = {
    version: 2,
    title: "Imported",
    width: 16,
    height: 9,
    duration: 2,
    fps: 60,
    background: "#000000",
    nodes: [{ id: "node", type: "circle", radius: 1 }],
    tracks: [{ target: "node", property: "x", keyframes: [{ at: 0, value: 0 }, { at: 2, value: 1 }] }],
    signals: [], bindings: [], controls: [], audio: [], captions: [],
  };
  const scene = new Scene(source);
  source.nodes[0].id = "mutated";
  assert.equal(scene.toJSON().nodes[0].id, "node");
  assert.equal(scene.cursor, 2);
  assert.equal(scene.toJSON().tracks.length, 1);
});

test("builds valid signal controls and bindings", () => {
  const circle = new Circle({ id: "controlled" });
  const scene = new Scene()
    .add(circle)
    .signal("radius", [{ at: 0, value: 1 }])
    .bind(circle, "radius", { op: "signal", id: "radius" })
    .control("radius", { label: "Radius", min: 0.25, max: 3, default: 1 });
  const data = scene.toJSON();
  assert.deepEqual(data.bindings[0], {
    target: "controlled",
    property: "radius",
    expression: { op: "signal", id: "radius" },
  });
  assert.deepEqual(data.controls[0], {
    id: "radius-control",
    label: "Radius",
    signal: "radius",
    min: 0.25,
    max: 3,
    step: 0.01,
    default: 1,
    timeline: false,
  });
});

test("rejects invalid animation timing and controls", () => {
  const circle = new Circle({ id: "invalid" });
  assert.throws(() => Create(circle, { duration: 0 }), /greater than zero/);
  assert.throws(() => new Scene().wait(-1), /must not be negative/);
  assert.throws(() => new Scene().control("x", { min: 2, max: 1 }), /greater than min/);
});

test("authors retained geometry and nested transform groups", () => {
  const polygon = new Polygon([[-1, -1], [1, -1], [0, 1]], { id: "polygon" });
  const curve = new CubicBezier([-2, 0], [-1, 2], [1, -2], [2, 0], { id: "curve" });
  const nested = new Group(new Triangle({ id: "triangle" }), { id: "nested" }).shift([1, 0]);
  const group = new Group([polygon, curve, nested], { id: "geometry" }).rotate(Math.PI / 4);
  const data = new Scene({ title: "Geometry" }).add(group).play(Create(group)).toJSON();

  assert.deepEqual(data.nodes.map(({ id }) => id), ["geometry", "polygon", "curve", "nested", "triangle"]);
  assert.equal(data.nodes.find(({ id }) => id === "polygon").parent, "geometry");
  assert.equal(data.nodes.find(({ id }) => id === "triangle").parent, "nested");
  assert.equal(data.nodes.find(({ id }) => id === "curve").type, "path");
  assert.deepEqual(data.tracks.map(({ target }) => target), ["polygon", "curve", "triangle"]);
});

test("builds path, 3D, camera, rich-text, and point-cloud schema exactly", () => {
  const path = new Path([
    pathCommand.moveTo([-2, 0]),
    pathCommand.quadTo([0, 2], [2, 0]),
    pathCommand.close(),
  ], { id: "path" });
  const path3d = new Path3D([
    pathCommand3D.moveTo([0, 0, 0]),
    pathCommand3D.lineTo([1, 1, 1]),
  ], { id: "path-3d" });
  const scene = new Scene()
    .add(
      path,
      path3d,
      new Arc({ id: "arc", radius: 2, angle: Math.PI }),
      new RegularPolygon(7, { id: "heptagon" }),
      new MarkupText([{ text: "fast", weight: "bold", color: "#58c4ddff" }], { id: "rich" }),
      new DotCloud([[0, 0], { x: 1, y: 1, color: "#ffffffff", radius: 0.1 }], { id: "dots" }),
      new Billboard([0, 0, 2], [0, 0], { id: "billboard" }),
    )
    .setCamera({ x: 1, zoom: 1.25 })
    .setCamera3D({ position: [0, -8, 5], target: [0, 0, 0], ambient: 0.4 })
    .play(MoveCamera({ x: 2, zoom: 2 }, { duration: 1 }))
    .play(OrbitCamera([4, -6, 4], { from: [0, -8, 5], duration: 1 }));
  const data = scene.toJSON();

  assert.deepEqual(data.camera, { x: 1, zoom: 1.25 });
  assert.deepEqual(data.camera3d.position, [0, -8, 5]);
  assert.deepEqual(data.nodes.find(({ id }) => id === "dots").points[0], { x: 0, y: 0 });
  assert.equal(data.nodes.find(({ id }) => id === "arc").commands.length, 3);
  assert.deepEqual(data.tracks.map(({ property }) => property), ["cameraX", "cameraZoom", "camera3dOrbit"]);
  assert.equal(data.duration, 2);
});

test("validates geometry before it reaches Wasm", () => {
  assert.throws(() => new Circle({ radius: 0 }), /greater than zero/);
  assert.throws(() => new Polygon([[0, 0], [1, 1]]), /3–100,000/);
  assert.throws(() => new Path([{ op: "cubicTo", c1x: 0 }]), /must be finite/);
  assert.throws(() => new RegularPolygon(2), /at least 3/);
  assert.throws(() => new Text(""), /nonempty/);
  assert.throws(() => new Circle().opacity(2), /between 0 and 1/);
  assert.throws(() => new Scene().setCamera3D({ near: 10, far: 1 }), /greater than/);
  const group = new Group();
  assert.throws(() => group.add(group), /cannot contain itself/);
});

test("group style helpers affect descendants without flattening transforms", () => {
  const first = new Circle({ id: "first" });
  const second = new Square({ id: "second" });
  const group = new Group(first, second, { id: "pair" }).fill("#ff0000ff").opacity(0.5).moveTo([2, 1]);
  const tracks = FadeIn(group);
  assert.deepEqual(tracks.map(({ target }) => target), ["first", "second"]);
  assert.equal(first.toJSON().style.fill, "#ff0000ff");
  assert.deepEqual(group.toJSON().transform, { x: 2, y: 1 });
});

test("composes parallel, lagged, and successive animation timing", () => {
  const first = new Circle({ id: "timing-first" });
  const second = new Circle({ id: "timing-second" });
  const parallel = AnimationGroup(
    MoveTo(first, [2, 0], { duration: 1 }),
    FadeIn(second, { duration: 2 }),
  );
  assert.deepEqual(parallel.map((track) => track.keyframes.at(-1).at), [1, 1, 2]);

  const lagged = LaggedStart(
    FadeIn(first, { duration: 1 }),
    FadeIn(second, { duration: 2 }),
    { lagRatio: 0.5, runTime: 5 },
  );
  assert.deepEqual(lagged[0].keyframes.map(({ at }) => at), [0, 2]);
  assert.deepEqual(lagged[1].keyframes.map(({ at }) => at), [1, 5]);

  const succession = Succession(
    MoveTo(first, [1, 0], { from: [0, 0], duration: 1 }),
    MoveTo(first, [3, 0], { from: [1, 0], duration: 2 }),
  );
  const scene = new Scene().add(first).play(succession);
  const tracks = scene.toJSON().tracks;
  assert.equal(tracks.length, 2);
  assert.deepEqual(tracks.find(({ property }) => property === "x").keyframes.map(({ at, value }) => [at, value]), [
    [0, 0], [1, 1], [3, 3],
  ]);
  assert.equal(scene.cursor, 3);
});

test("Scene.play scales the whole play and advances by its actual maximum runtime", () => {
  const first = new Circle({ id: "play-first" });
  const second = new Circle({ id: "play-second" });
  const scene = new Scene()
    .add(first, second)
    .play(
      FadeIn(first, { duration: 1 }),
      FadeIn(second, { duration: 2 }),
      { runTime: 4 },
    )
    .wait(0.5)
    .play(FadeOut(first, { duration: 1 }));
  const data = scene.toJSON();
  assert.equal(scene.cursor, 5.5);
  assert.equal(data.duration, 5.5);
  assert.deepEqual(
    data.tracks.find(({ target }) => target === "play-first").keyframes.map(({ at, value }) => [at, value]),
    [[0, 0], [2, 1], [4.5, 1], [5.5, 0]],
  );
  assert.deepEqual(
    data.tracks.find(({ target }) => target === "play-second").keyframes.map(({ at }) => at),
    [0, 4],
  );
});

test("transforms compatible retained properties and rejects lossy morphs", () => {
  const source = new Circle({
    id: "transform-source",
    radius: 1,
    transform: { x: -2 },
    style: { fill: "#ff0000ff" },
  });
  const target = new Circle({
    id: "transform-target",
    radius: 2,
    transform: { x: 3, scaleY: 0.5 },
    style: { fill: "#0000ffff", opacity: 0.6 },
  });
  const tracks = Transform(source, target, { duration: 2 });
  assert.deepEqual(new Set(tracks.map(({ property }) => property)), new Set(["x", "scaleY", "fill", "opacity", "radius"]));
  assert.deepEqual(tracks.find(({ property }) => property === "radius").keyframes, [
    { at: 0, value: 1 },
    { at: 2, value: 2, easing: "smooth" },
  ]);
  assert.throws(() => Transform(source, new Square({ id: "lossy-target" })), /cannot interpolate circle into rect/);
  assert.throws(
    () => new Scene().add(source).play(
      animate(source, "x", 0, 1),
      animate(source, "x", 0, 2),
    ),
    /overlapping animations/,
  );
});

test("ReplacementTransform schedules exact retained hierarchy swaps through compositions", () => {
  const first = new Circle({ id: "replace-first", radius: 1 });
  const second = new Circle({ id: "replace-second", radius: 2 });
  const third = new Circle({ id: "replace-third", radius: 3 });
  const scene = new Scene()
    .add(first)
    .play(Succession(
      ReplacementTransform(first, second, { duration: 1 }),
      ReplacementTransform(second, third, { duration: 2 }),
      { runTime: 6 },
    ));
  const data = scene.toJSON();
  assert.equal(scene.cursor, 6);
  assert.deepEqual(data.nodes.map(({ id }) => id), ["replace-first", "replace-second", "replace-third"]);
  assert.equal(data.nodes.find(({ id }) => id === "replace-first").disappearAt, 2);
  assert.equal(data.nodes.find(({ id }) => id === "replace-second").appearAt, 2);
  assert.equal(data.nodes.find(({ id }) => id === "replace-second").disappearAt, 6);
  assert.equal(data.nodes.find(({ id }) => id === "replace-third").appearAt, 6);
  assert.deepEqual(data.tracks.find(({ target }) => target === "replace-first").keyframes.map(({ at }) => at), [0, 2]);
  assert.deepEqual(data.tracks.find(({ target }) => target === "replace-second").keyframes.map(({ at }) => at), [2, 6]);
});

test("authors byte-exact raster images without a browser decoder", () => {
  const pixels = new Uint8ClampedArray([255, 0, 0, 255, 0, 255, 0, 128]);
  const image = new Image(pixels, 2, 1, {
    id: "pixels",
    corners: [[-2, 1], [2, 1], [-2, -1], [2, -1]],
    resampling: "lanczos",
  });
  const node = image.toJSON();
  assert.equal(node.pixels, Buffer.from(pixels).toString("base64"));
  assert.deepEqual(node.corners, [[-2, 1], [2, 1], [-2, -1], [2, -1]]);
  assert.equal(node.resampling, "lanczos");
  assert.equal(Image.fromImageData({ data: pixels, width: 2, height: 1 }).toJSON().pixels, node.pixels);
  assert.throws(() => new Image(new Uint8Array(3), 1, 1), /expected 4/);
  assert.throws(() => new Image("////", 2, 1), /matching its dimensions/);
  assert.throws(() => new Image(pixels, 2, 1, { resampling: "pixelated" }), /not supported/);
  assert.throws(() => new Image(pixels, 2, 1, { surprise: true }), /Unknown Image option/);
});

test("authors native Mesh and Surface topology with strict renderer limits", () => {
  const vertices = [[-1, -1, 0], [1, -1, 0], [0, 1, 0]];
  const texture = new Uint8Array([255, 255, 255, 255]);
  const mesh = new Mesh(vertices, [[0, 1, 2]], {
    id: "mesh",
    colors: ["#ff0000ff", "#00ff00ff", "#0000ffff"],
    normals: [[0, 0, 1], [0, 0, 1], [0, 0, 1]],
    uvs: [[0, 0], [1, 0], [0.5, 1]],
    texture: { data: texture, width: 1, height: 1 },
    gloss: 0.4,
    shadow: 0.2,
    doubleSided: true,
  });
  const surface = new Surface(vertices, [[0, 1, 2]], {
    id: "surface",
    colors: ["#ff0000ff", "#00ff00ff", "#0000ffff"],
    strokeColors: ["#ffffffff"],
    strokeRadii: [0.02],
    unlit: true,
  });
  const data = new Scene().add(mesh, surface).toJSON();
  assert.equal(data.nodes[0].texturePixels, Buffer.from(texture).toString("base64"));
  assert.deepEqual(data.nodes[0].triangles, [[0, 1, 2]]);
  assert.equal(data.nodes[1].colors.length, 3);
  assert.equal(data.nodes[1].unlit, true);
  assert.throws(() => new Mesh(vertices, [[0, 1, 3]]), /must not exceed 2/);
  assert.throws(() => new Mesh(vertices, [[0, 1, 2]], { colors: ["#ffffff"] }), /match the vertex count/);
  assert.throws(() => new Mesh(vertices, [[0, 1, 2]], { uvs: [[0, 0], [1, 0], [0, 1]] }), /require texture/);
  assert.throws(() => new Surface(vertices, [[0, 1, 2]], {
    colors: ["#ffffffff"], strokeColors: ["#ffffffff"], strokeRadii: [0.02],
  }), /flattened patch corners/);
});

test("authors deterministic TracePath frames and retained escape-hatch nodes", () => {
  const segments = [
    { start: [0, 0], control1: [0.3, 0], control2: [0.7, 1], end: [1, 1] },
    { start: [1, 1], control1: [1.3, 1], control2: [1.7, 0], end: [2, 0] },
  ];
  const trace = new TracePath(segments, [
    { at: 0, start: 0, count: 1 },
    { at: 1, start: 0, count: 2, closed: false },
  ], { id: "trace" });
  assert.equal(trace.toJSON().frames[0].closed, false);
  assert.equal(new Scene().add(trace).toJSON().duration, 1);
  assert.throws(() => new TracePath(segments, [{ at: 0, start: 1, count: 2 }]), /exceeds the segment list/);
  assert.throws(() => new TracePath(segments, [
    { at: 1, start: 0, count: 1 }, { at: 0, start: 0, count: 1 },
  ]), /ordered by time/);

  const raw = { id: "shader", type: "customShaderMesh", vertexData: [0, 1, 2] };
  const retained = RetainedNode.from(raw);
  raw.vertexData[0] = 99;
  assert.deepEqual(retained.toJSON(), { id: "shader", type: "customShaderMesh", transform: {}, style: {}, vertexData: [0, 1, 2] });
  assert.throws(() => new RetainedNode({ type: "circle", radius: 1 }), /retained node id/);
});

test("MathTex compiles through a configurable endpoint into Rust-native SVG", async () => {
  let request;
  const equation = await MathTex.typeset(String.raw`x^2 + y^2 = 1`, {
    id: "equation",
    fontSize: 1.5,
    endpoint: "/latex",
    fetch: async (url, init) => {
      request = { url, init };
      return { ok: true, status: 200, json: async () => ({ svg: '<svg viewBox="0 0 2 1"><path d="M0 0L2 1"/></svg>' }) };
    },
  });
  assert.equal(request.url, "/latex");
  assert.deepEqual(JSON.parse(request.init.body), { tex: "x^2 + y^2 = 1" });
  assert.deepEqual(equation.toJSON(), {
    id: "equation",
    type: "svg",
    transform: {},
    style: {},
    svg: '<svg viewBox="0 0 2 1"><path d="M0 0L2 1"/></svg>',
    height: 1.5,
    preserveStyles: false,
  });
  assert.equal(equation.source, "x^2 + y^2 = 1");
  await assert.rejects(() => MathTex.typeset("x", {
    fetch: async () => ({ ok: false, status: 422, json: async () => ({ error: "bad TeX" }) }),
  }), /bad TeX/);
});

test("retains explicit portable font families for plain and markup text", () => {
  const plain = new Text("Custom", {
    id: "custom-plain",
    fontFamily: "Uploaded Sans",
    weight: "bold",
  });
  const markup = new MarkupText([{ text: "Custom", slant: "italic" }], {
    id: "custom-markup",
    fontFamily: "Uploaded Sans",
  });
  const data = new Scene().add(plain, markup).toJSON();
  assert.equal(data.nodes[0].fontFamily, "Uploaded Sans");
  assert.equal(data.nodes[1].fontFamily, "Uploaded Sans");
  assert.throws(() => new Text("Bad", { fontFamily: "   " }), /visible characters/);
  assert.throws(() => new Text("Bad", { unknownTextOption: true }), /Unknown Text option/);
});
