import assert from "node:assert/strict";
import test from "node:test";
import {
  AnimationGroup,
  Arc,
  Arrow,
  Axes,
  Billboard,
  Circle,
  Create,
  CubicBezier,
  DL,
  DR,
  DotCloud,
  FadeIn,
  FadeOut,
  FRAME_HEIGHT,
  FRAME_WIDTH,
  FunctionGraph,
  Group,
  Image,
  LEFT,
  Line,
  MarkupText,
  LaggedStart,
  MathTex,
  Mesh,
  MoveCamera,
  MoveTo,
  NumberLine,
  NumberPlane,
  OrbitCamera,
  Path,
  Path3D,
  PathReference,
  ParametricFunction,
  Polygon,
  Rectangle,
  RegularPolygon,
  ReplacementTransform,
  RetainedNode,
  RIGHT,
  Scene,
  Square,
  SVG,
  Text,
  Transform,
  Triangle,
  Surface,
  TracePath,
  UL,
  UP,
  VGroup,
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

test("authors native stroke caps, joins, and dash patterns", () => {
  const line = new Path([
    pathCommand.moveTo([-2, 0]),
    pathCommand.lineTo([0, 1]),
    pathCommand.lineTo([2, 0]),
  ], { id: "styled-stroke" })
    .stroke("#58c4ddff", 0.2)
    .strokeCap("round")
    .strokeJoin("bevel")
    .dash([0.5, 0.2, 0.1], -0.125);
  assert.deepEqual(line.toJSON().style, {
    stroke: "#58c4ddff",
    strokeWidth: 0.2,
    strokeCap: "round",
    strokeJoin: "bevel",
    dashArray: [0.5, 0.2, 0.1],
    dashOffset: -0.125,
  });
  assert.throws(() => line.dash([0]), /between 0.00001 and 100000/);
  assert.throws(() => line.strokeCap("triangle"), /butt, square, or round/);
  assert.throws(
    () => Transform(line, new Path(line.toJSON().commands).strokeCap("square")),
    /strokeCap is discrete/,
  );
  assert.doesNotThrow(() => ReplacementTransform(
    line,
    new Path(line.toJSON().commands, { id: "replacement-stroke" }).strokeCap("square"),
  ));
  const offsetTarget = new Path(line.toJSON().commands, { id: "offset-stroke" })
    .strokeCap("round")
    .strokeJoin("bevel")
    .dash([0.5, 0.2, 0.1], 0.75);
  const offsetTracks = Transform(line, offsetTarget);
  assert.equal(offsetTracks.find(({ property }) => property === "dashOffset")?.keyframes.at(-1).value, 0.75);
  const scene = new Scene().add(line).play(animate(line, "dashOffset", -0.125, 0.75));
  assert.equal(scene.toJSON().tracks[0].property, "dashOffset");
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

test("computes exact 2D geometry bounds through retained nested transforms", () => {
  const rectangle = new Rectangle({ id: "bounded-rectangle", width: 4, height: 2 }).moveTo([1, 0]);
  const nested = new Group(rectangle, { id: "bounded-nested" }).rotate(Math.PI / 2).shift([3, -1]);
  const bounds = nested.getBounds();
  assert.ok(Math.abs(bounds.minX - 2) < 1e-12);
  assert.ok(Math.abs(bounds.maxX - 4) < 1e-12);
  assert.ok(Math.abs(bounds.minY + 2) < 1e-12);
  assert.ok(Math.abs(bounds.maxY - 2) < 1e-12);
  assert.deepEqual(bounds.center, [3, 0]);
  assert.equal(bounds.width, 2);
  assert.equal(bounds.height, 4);
  assert.equal(Object.isFrozen(bounds), true);
  assert.equal(Object.isFrozen(bounds.center), true);
  assert.ok(Math.abs(nested.getLeft()[0] - 2) < 1e-12);
  assert.ok(Math.abs(nested.getRight()[0] - 4) < 1e-12);
  assert.ok(Math.abs(nested.getTop()[1] - 2) < 1e-12);
  assert.ok(Math.abs(nested.getBottom()[1] + 2) < 1e-12);

  const quadratic = new Path([
    pathCommand.moveTo([-1, 0]),
    pathCommand.quadTo([0, 2], [1, 0]),
  ], { id: "bounded-quadratic" });
  assert.deepEqual(quadratic.getBounds(), {
    minX: -1,
    minY: 0,
    maxX: 1,
    maxY: 1,
    width: 2,
    height: 1,
    center: [0, 0.5],
  });

  const arrow = new Arrow([0, 0], [2, 0], { id: "bounded-arrow", tipSize: 1 });
  assert.deepEqual(arrow.getBounds(), {
    minX: 0,
    minY: -0.55,
    maxX: 2,
    maxY: 0.55,
    width: 2,
    height: 1.1,
    center: [1, 0],
  });
});

test("moveTo aligns asymmetric geometry centers exactly and shift preserves the resulting translation", () => {
  const line = new Line([1, -2], [5, 4], { id: "move-line" }).moveTo([10, -3]);
  assert.deepEqual(line.getCenter(), [10, -3]);
  assert.deepEqual(line.node.transform, { x: 7, y: -4 });
  line.shift([2, 1]);
  assert.deepEqual(line.getCenter(), [12, -2]);
  assert.deepEqual(line.node.transform, { x: 9, y: -3 });

  const path = new Path([
    pathCommand.moveTo([-4, -1]),
    pathCommand.lineTo([2, 5]),
  ], { id: "move-path" }).moveTo([-2, 7]);
  assert.deepEqual(path.getCenter(), [-2, 7]);
  assert.deepEqual(path.node.transform, { x: -1, y: 5 });

  const image = new Image(new Uint8Array([255, 255, 255, 255]), 1, 1, {
    id: "move-image",
    corners: [[0, 4], [6, 3], [1, -2], [7, -1]],
  }).moveTo([4, -5]);
  assert.deepEqual(image.getCenter(), [4, -5]);
  assert.deepEqual(image.node.transform, { x: 0.5, y: -6 });
});

test("moveTo aligns matching critical points on Mobject targets", () => {
  const target = new Rectangle({ id: "move-target", width: 4, height: 2 }).moveTo([5, 1]);
  const line = new Line([0, -2], [3, 1], { id: "move-aligned-line" }).moveTo(target, UL);
  assert.deepEqual(line.getCriticalPoint(UL), target.getCriticalPoint(UL));
  assert.deepEqual(line.getCriticalPoint(UL), [3, 2]);

  const pointAligned = new Rectangle({ id: "move-aligned-point", width: 2, height: 4 })
    .moveTo([7, -3], DR);
  assert.deepEqual(pointAligned.getCriticalPoint(DR), [7, -3]);
});

test("moveTo keeps centered renderer geometry compatible and rejects unavailable edge metrics", () => {
  const text = new Text("centered", { id: "move-text" }).moveTo([3, -2]);
  const markup = new MarkupText([{ text: "centered" }], { id: "move-markup" }).moveTo(-4, 1);
  const svg = new SVG("<svg viewBox='0 0 10 10'/>", { id: "move-svg" }).moveTo([2, 3]);
  const math = new MathTex("<svg viewBox='0 0 10 10'/>", { id: "move-math" }).moveTo([-1, -3]);
  assert.deepEqual(text.node.transform, { x: 3, y: -2 });
  assert.deepEqual(markup.node.transform, { x: -4, y: 1 });
  assert.deepEqual(svg.node.transform, { x: 2, y: 3 });
  assert.deepEqual(math.node.transform, { x: -1, y: -3 });

  const anchor = new Circle({ id: "move-renderer-anchor" }).moveTo([6, 2]);
  text.moveTo(anchor);
  assert.deepEqual(text.node.transform, { x: 6, y: 2 });
  anchor.moveTo(svg);
  assert.deepEqual(anchor.getCenter(), [2, 3]);

  assert.throws(() => text.moveTo([0, 0], RIGHT), /font shaping metrics/);
  assert.throws(
    () => new Text("left", { id: "move-left-text", align: "left" }).moveTo([0, 0]),
    /font shaping metrics/,
  );
  assert.throws(
    () => new MarkupText([{ text: "right" }], { id: "move-right-markup", align: "right" }).moveTo([0, 0]),
    /font shaping metrics/,
  );
  assert.throws(() => svg.moveTo(anchor, UP), /parsed SVG viewport/);
  assert.throws(
    () => new Text("nested", { id: "move-parented-text", parent: "parent" }).moveTo([0, 0]),
    /Cannot resolve parent/,
  );
});

test("positions retained objects with Manim-style center, alignment, edge, and corner helpers", () => {
  const anchor = new Square({ id: "layout-anchor", size: 2 });
  const placed = new Rectangle({ id: "layout-placed", width: 4, height: 2 })
    .setX(10)
    .setY(-3)
    .nextTo(anchor, RIGHT, 0.5, UP);
  assert.deepEqual(placed.getCenter(), [3.5, 0]);
  assert.deepEqual(placed.getLeft(), [1.5, 0]);
  assert.deepEqual(placed.getTop(), [3.5, 1]);

  placed.alignTo(anchor, UL);
  assert.equal(placed.getLeft()[0], anchor.getLeft()[0]);
  assert.equal(placed.getTop()[1], anchor.getTop()[1]);
  placed.nextTo(Object.freeze([5, -2]), LEFT, 0.25);
  assert.deepEqual(placed.getRight(), [4.75, -2]);

  const diagonal = new Square({ id: "layout-diagonal", size: 2 }).nextTo(anchor, DR, 0.5);
  assert.deepEqual(diagonal.getCenter(), [2.5, -2.5]);

  const edge = new Square({ id: "layout-edge", size: 2 }).toEdge(RIGHT);
  assert.equal(FRAME_WIDTH, 16);
  assert.equal(FRAME_HEIGHT, 9);
  assert.deepEqual(edge.getRight(), [7.5, 0]);
  edge.toCorner(DR, 1, Object.freeze({ width: 20, height: 10, center: Object.freeze([10, 5]) }));
  assert.deepEqual(edge.getRight(), [19, 2]);
  assert.deepEqual(edge.getBottom(), [18, 1]);
  edge.center();
  assert.deepEqual(edge.getCenter(), [0, 0]);

  const weightedEdge = new Square({ id: "layout-weighted-edge", size: 2 }).toEdge([2, 1], 0.5);
  assert.deepEqual(weightedEdge.getRight(), [7, 3]);
  assert.deepEqual(weightedEdge.getTop(), [6, 4]);

  const defaultCorner = new Square({ id: "layout-default-corner", size: 2 }).toCorner();
  assert.deepEqual(defaultCorner.getCriticalPoint(DL), [-7.5, -4]);
  const cornerAlias = new Square({ id: "layout-corner-as-edge", size: 2 }).toCorner(UP);
  assert.deepEqual(cornerAlias.getTop(), [0, 4]);
  assert.deepEqual(new Square({ id: "layout-zero-direction" }).toEdge([0, 0]).getCenter(), [0, 0]);
});

test("arranges direct Group members locally while preserving retained hierarchy transforms", () => {
  const first = new Square({ id: "arrange-first", size: 2 });
  const second = new Rectangle({ id: "arrange-second", width: 2, height: 4 });
  const nestedLeaf = new Square({ id: "arrange-nested-leaf", size: 2 });
  const third = new Group(nestedLeaf, { id: "arrange-third" });
  const group = new VGroup(first, second, third, { id: "arranged", transform: { x: 8, y: -4, rotation: 0.2 } })
    .arrange(RIGHT, { buff: 1, alignedEdge: UP });

  assert.deepEqual(group.toJSON().transform, { x: 8, y: -4, rotation: 0.2 });
  assert.deepEqual(first.getCenter(), [-3, 1]);
  assert.deepEqual(second.getCenter(), [0, 0]);
  assert.deepEqual(third.getCenter(), [3, 1]);
  assert.equal(first.getTop()[1], second.getTop()[1]);
  assert.equal(second.getTop()[1], third.getTop()[1]);
  assert.equal(second.getLeft()[0] - first.getRight()[0], 1);
  assert.equal(third.getLeft()[0] - second.getRight()[0], 1);

  const nodes = new Scene().add(group).toJSON().nodes;
  assert.deepEqual(nodes.map(({ id }) => id), [
    "arranged", "arrange-first", "arrange-second", "arrange-third", "arrange-nested-leaf",
  ]);
  assert.equal(nodes.find(({ id }) => id === "arrange-nested-leaf").parent, "arrange-third");
});

test("copies complete retained hierarchies with fresh ids and remapped public aliases", () => {
  const axes = new Axes({ id: "copy-axes", xRange: [-2, 2, 1], yRange: [-1, 1, 1] }).shift([2, -1]);
  const copy = axes.copy();
  assert.ok(copy instanceof Axes);
  assert.notEqual(copy.id, axes.id);
  assert.notEqual(copy.xAxis.id, axes.xAxis.id);
  assert.equal(copy.xAxis, copy.members[0]);
  assert.equal(copy.yAxis, copy.members[1]);
  assert.deepEqual(copy.getBounds(), axes.getBounds());
  copy.xAxis.shift([5, 0]);
  assert.notDeepEqual(copy.xAxis.toJSON().transform, axes.xAxis.toJSON().transform);

  const nodes = new Scene().add(axes, copy).toJSON().nodes;
  assert.equal(new Set(nodes.map(({ id }) => id)).size, nodes.length);
  assert.ok(nodes.every(({ parent }) => parent === undefined || nodes.some(({ id }) => id === parent)));

  const probe = new Circle();
  const nextSuffix = Number(probe.id.slice(probe.id.lastIndexOf("-") + 1)) + 1;
  const occupied = new Circle({ id: `circle-${nextSuffix}` });
  const collisionSafeCopy = probe.copy();
  assert.notEqual(collisionSafeCopy.id, probe.id);
  assert.notEqual(collisionSafeCopy.id, occupied.id);
  assert.doesNotThrow(() => new Scene().add(probe, occupied, collisionSafeCopy).toJSON());

  const sourcePath = new Path([
    pathCommand.moveTo([0, 0]),
    pathCommand.lineTo([1, 0]),
  ], { id: "copy-reference-source" });
  const referenceBundle = new Group(
    sourcePath,
    new PathReference(sourcePath, { id: "copy-reference" }),
    { id: "copy-reference-bundle" },
  ).copy();
  assert.equal(referenceBundle.members[1].node.source, referenceBundle.members[0].id);
});

test("Group ignores repeated object identities like Manim", () => {
  const circle = new Circle({ id: "deduplicated-member" });
  const group = new Group(circle, circle).add(circle);
  assert.equal(group.members.length, 1);
  assert.doesNotThrow(() => group.arrange());
  assert.equal(new Scene().add(group).toJSON().nodes.filter(({ id }) => id === circle.id).length, 1);
});

test("layout fails explicitly when exact 2D bounds are unavailable", () => {
  assert.throws(() => new Text("metrics", { id: "layout-text" }).getBounds(), /font shaping metrics/);
  assert.throws(() => new SVG("<svg/>", { id: "layout-svg" }).getCenter(), /parsed SVG viewport/);
  assert.throws(() => new Path3D([
    pathCommand3D.moveTo([0, 0, 0]), pathCommand3D.lineTo([1, 1, 1]),
  ], { id: "layout-3d" }).getBounds(), /camera projection/);
  assert.throws(() => RetainedNode.from({ id: "layout-unknown", type: "futureNode" }).getBounds(), /unknown to the public layout API/);
  assert.throws(() => new Group({ id: "layout-empty" }).getBounds(), /empty or non-drawing/);
  assert.throws(() => new Circle({ id: "layout-parented", parent: "missing" }).getBounds(), /Cannot resolve parent/);
  assert.throws(() => new Circle({ id: "layout-rotated-3d", transform: { rotationX: 0.2 } }).getBounds(), /3D rotation/);
  assert.throws(() => new Group(new Circle()).arrange(RIGHT, { mystery: true }), /Unknown arrange option/);
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

test("accepts raw nested Pango markup without flattening it in JavaScript", () => {
  const markup = new MarkupText(
    "<span foreground='red'><b>fast &amp; <i>faithful</i></b></span>",
    { id: "raw-pango", fontSize: 1.2 },
  );
  assert.equal(markup.node.markup, "<span foreground='red'><b>fast &amp; <i>faithful</i></b></span>");
  assert.deepEqual(markup.node.spans, []);
  assert.equal(markup.node.fontSize, 1.2);
});

test("preserves semantic correspondence receipts and accepts Manim smooth easing", () => {
  const correspondences = [{
    id: "match-1",
    kind: "transformMatchingTex",
    mode: "transform",
    keys: ["x"],
    targetKeys: ["x"],
    sourceNodes: ["source-x"],
    targetNodes: ["target-x"],
    start: 0,
    end: 1,
    pathArc: 0,
  }];
  const circle = new Circle({ id: "manim-easing-circle" });
  const data = new Scene({ correspondences })
    .add(circle)
    .track(circle, "x", [
      { at: 0, value: 0 },
      { at: 1, value: 1, easing: "manimSmooth" },
    ])
    .toJSON();
  assert.deepEqual(data.correspondences, correspondences);
  assert.equal(data.tracks[0].keyframes[1].easing, "manimSmooth");
});

test("retains explicit Pango span metrics, paints, and decorations", () => {
  const markup = new MarkupText([{
    text: "styled\ntext",
    color: "#ff0000ff",
    background: "#11223344",
    weight: "bold",
    slant: "italic",
    fontFamily: "Noto Sans",
    fontScale: 1.5,
    rise: 0.25,
    letterSpacing: 0.05,
    underline: "double",
    underlineColor: "#00ff00ff",
    strikethrough: true,
    strikethroughColor: "#0000ffff",
  }], { id: "styled-pango" });
  assert.deepEqual(markup.node.spans[0], {
    text: "styled\ntext",
    color: "#ff0000ff",
    background: "#11223344",
    weight: "bold",
    slant: "italic",
    fontFamily: "Noto Sans",
    fontScale: 1.5,
    rise: 0.25,
    letterSpacing: 0.05,
    underline: "double",
    underlineColor: "#00ff00ff",
    strikethrough: true,
    strikethroughColor: "#0000ffff",
  });
  assert.throws(
    () => new MarkupText([{ text: "x", gravity: "east" }]),
    /Unknown spans\[0\] property: gravity/,
  );
});

test("authors retained NumberLine ticks and labels with reversible coordinates", () => {
  const line = new NumberLine({
    id: "number-line",
    xRange: [-2, 2, 0.5],
    length: 8,
    includeNumbers: true,
    numbersToExclude: [0],
    decimalPlaces: 1,
    direction: [1, 1],
  }).shift([1, -1]);
  assert.equal(line.getTickValues().length, 9);
  assert.equal(line.ticks.members.length, 9);
  assert.equal(line.numbers.members.length, 8);
  assert.deepEqual(line.n2p(0), [1, -1]);
  assert.ok(Math.abs(line.p2n(line.n2p(1.5)) - 1.5) < 1e-12);
  const nodes = new Scene().add(line).play(Create(line)).toJSON();
  assert.equal(nodes.nodes.find(({ id }) => id === "number-line.axis").type, "line");
  assert.equal(nodes.tracks.length, 18);
  assert.deepEqual(
    new NumberLine({ xRange: [-5, 5, 2.5], includeNumbers: true }).numbers.members.map(({ node }) => node.text),
    ["-5", "-2.5", "0", "2.5", "5"],
  );
  const positive = new NumberLine({ xRange: [0, 10, 1], length: 10 });
  assert.deepEqual(positive.n2p(0), [-5, 0]);
  assert.deepEqual(positive.n2p(10), [5, 0]);
  assert.equal(positive.p2n([-5, 0]), 0);
  assert.throws(() => new NumberLine({ xRange: [2, -2, 1] }), /greater than its minimum/);
  assert.throws(() => new NumberLine({ direction: [0, 0] }), /zero vector/);
  assert.throws(() => new NumberLine({ mystery: true }), /Unknown NumberLine option/);
});

test("Axes converts coordinates and produces animation-compatible retained graphs", () => {
  const axes = new Axes({
    id: "plot-axes",
    xRange: [-2, 4, 1],
    yRange: [-1, 3, 1],
    xLength: 6,
    yLength: 4,
    includeNumbers: true,
    xLabel: "x",
    yLabel: "f(x)",
  }).shift([1, 2]);
  const point = axes.c2p(2, 1);
  assert.deepEqual(point, [2, 2]);
  assert.deepEqual(axes.p2c(point), [2, 1]);
  assert.deepEqual(axes.getOrigin(), [0, 1]);
  assert.equal(axes.xAxis.numbers.members.length, 7);
  assert.equal(axes.yAxis.numbers.members.length, 4);
  assert.equal(axes.axisLabels.members.length, 2);

  const reciprocal = axes.plot((x) => 1 / x, [-2, 2], {
    id: "reciprocal",
    samples: 101,
    discontinuities: [0],
  });
  assert.ok(reciprocal instanceof FunctionGraph);
  assert.equal(reciprocal.segments.length, 2);
  assert.equal(reciprocal.node.commands.filter(({ op }) => op === "moveTo").length, 2);
  assert.ok(reciprocal.node.commands.every((command) => Object.values(command).every((value) => typeof value !== "number" || Number.isFinite(value))));
  assert.equal(reciprocal.valueAt(2), 0.5);

  const shifted = axes.plot((x) => 1 / x + 1, [-2, 2], {
    id: "shifted-reciprocal",
    samples: 101,
    discontinuities: [0],
  });
  const morph = Transform(reciprocal, shifted);
  assert.deepEqual(morph.map(({ property }) => property), ["commands"]);
  const scene = new Scene().add(axes, reciprocal).play(Create(axes), Create(reciprocal));
  assert.ok(scene.toJSON().tracks.some(({ target }) => target === "reciprocal"));

  const positive = new Axes({ xRange: [0, 10, 1], yRange: [0, 5, 1], xLength: 10, yLength: 6 });
  assert.deepEqual(positive.c2p(0, 0), [-5, -3]);
  assert.deepEqual(positive.c2p(10, 5), [5, 3]);
  assert.deepEqual(positive.p2c([-5, -3]), [0, 0]);
  assert.deepEqual(positive.xAxis.node.transform, { x: 0, y: -3 });
  assert.deepEqual(positive.yAxis.node.transform, { x: -5, y: 0 });
});

test("ParametricFunction and NumberPlane lower to general retained paths and lines", () => {
  const curve = new ParametricFunction(
    (t) => [Math.cos(t), Math.sin(t)],
    [0, Math.PI * 2],
    { id: "unit-circle", samples: 65 },
  );
  assert.equal(curve.sampleCount, 65);
  assert.equal(curve.node.type, "path");
  assert.ok(Math.abs(curve.pointAt(Math.PI / 2)[1] - 1) < 1e-12);

  const plane = new NumberPlane({
    id: "plane",
    xRange: [-2, 2, 1],
    yRange: [-1, 1, 1],
    xLength: 8,
    yLength: 4,
    gridSubdivisions: 2,
  });
  assert.equal(plane.gridLines.members.length, 12);
  const parametric = plane.plotParametric((t) => [t, t * t], [-1, 1], { samples: 41, id: "parabola" });
  assert.ok(parametric instanceof ParametricFunction);
  assert.deepEqual(parametric.pointAt(1), [2, 2]);
  const data = new Scene().add(plane, parametric).toJSON();
  assert.equal(data.nodes.find(({ id }) => id === "plane.grid-x-0").type, "line");
  const positivePlane = new NumberPlane({
    id: "positive-plane", xRange: [0, 2, 1], yRange: [0, 2, 1], xLength: 4, yLength: 4,
  });
  const firstVertical = positivePlane.gridLines.members.find(({ id }) => id === "positive-plane.grid-x-1");
  assert.deepEqual(firstVertical.node.from, [0, -2]);
  assert.deepEqual(firstVertical.node.to, [0, 2]);
  assert.throws(() => new ParametricFunction(() => [Number.NaN, 0]), /did not produce a finite curve/);
  assert.throws(() => new FunctionGraph(() => "wrong"), /must return a number/);
  assert.throws(() => new NumberPlane({ gridSubdivisions: 0 }), /at least 1/);
});

test("automatic ids remain unique beyond JavaScript's safe-integer boundary", () => {
  new Circle({ id: "id-boundary-9007199254740990" });
  const ids = [new Circle().id, new Circle().id, new Circle().id];
  assert.deepEqual(ids, [
    "circle-9007199254740991",
    "circle-9007199254740992",
    "circle-9007199254740993",
  ]);
  assert.equal(new Set(ids).size, ids.length);

  new Circle({ id: `circle-${"9".repeat(73)}` });
  assert.match(new Circle().id, /^circle-\d+$/);
});
