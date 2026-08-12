import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_SCENE,
  formatSceneCode,
  parseSceneCode,
} from "../apps/web-preview/www/scene-schema.js";

test("default scene code round-trips", () => {
  const parsed = parseSceneCode(formatSceneCode(DEFAULT_SCENE));
  assert.equal(parsed.version, 2);
  assert.equal(parsed.title, DEFAULT_SCENE.title);
  assert.equal(parsed.pixelWidth, 1280);
  assert.equal(parsed.pixelHeight, 720);
  assert.ok(parsed.nodes.length >= 6);
  assert.deepEqual(
    new Set(parsed.nodes.map((node) => node.type)),
    new Set(["text", "circle", "rect", "polyline", "line"]),
  );
});

test("scene coordinates and pixel export dimensions are independent", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.width = 16;
  scene.height = 9;
  scene.pixelWidth = 854;
  scene.pixelHeight = 480;
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.deepEqual(
    [parsed.width, parsed.height, parsed.pixelWidth, parsed.pixelHeight],
    [16, 9, 854, 480],
  );
});

test("arbitrary JavaScript is rejected", () => {
  assert.throws(
    () => parseSceneCode("fetch('https://example.com')"),
    /one scene/,
  );
});

test("out-of-range scene values are rejected", () => {
  const invalid = structuredClone(DEFAULT_SCENE);
  invalid.nodes[1].radius = 100_001;
  assert.throws(() => parseSceneCode(`scene(${JSON.stringify(invalid)});`), /radius/);
});

test("tracks can animate topology-independent properties and colors", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.tracks.push({
    target: "triangle",
    property: "fill",
    keyframes: [
      { at: 0, value: "#22c55e55" },
      { at: 6, value: "#a855f7aa", easing: "smooth" },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-1).property, "fill");
});

test("point clouds can retain a screen-space radius under camera zoom", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "screen-points",
    type: "pointCloud",
    radius: 0.12,
    screenSpaceRadius: true,
    points: [{ x: -1, y: 0, color: "#22c55eff" }, { x: 1, y: 0 }],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).screenSpaceRadius, true);
});

test("tracks can reuse concrete keyframes for the same property", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.tracks.push({
    target: "square",
    property: "opacity",
    keyframesFrom: "ring",
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-1).keyframesFrom, "ring");
  scene.tracks.at(-1).keyframesFrom = "triangle";
  assert.throws(
    () => parseSceneCode(formatSceneCode(scene)),
    /missing or ambiguous opacity keyframes/,
  );
});

test("real LaTeX nodes survive code validation before vector materialization", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "equation",
    type: "mathTex",
    tex: String.raw`\int_0^\infty e^{-x^2}\,dx=\frac{\sqrt{\pi}}{2}`,
    fontSize: 1.2,
    style: { fill: "#ffffff", stroke: null },
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).type, "mathTex");
  assert.match(parsed.nodes.at(-1).tex, /\\int/);
});

test("missing track targets fail honestly", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.tracks.push({
    target: "not-a-node",
    property: "x",
    keyframes: [{ at: 0, value: 0 }],
  });
  assert.throws(
    () => parseSceneCode(`scene(${JSON.stringify(scene)});`),
    /target does not exist/,
  );
});

test("declared controls round-trip with their signal contract", () => {
  const parsed = parseSceneCode(formatSceneCode(DEFAULT_SCENE));
  assert.deepEqual(parsed.controls, [
    {
      id: "ring-size",
      label: "Ring radius",
      signal: "ring-size",
      min: 0.4,
      max: 2.2,
      step: 0.05,
      default: 1.2,
      timeline: false,
    },
  ]);
});

test("monotonic signals can drive semantic timeline controls", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.signals.push({
    id: "tracker-value",
    keyframes: [
      { at: 0, value: -2 },
      { at: 3, value: 1 },
      { at: 6, value: 4 },
    ],
  });
  scene.controls = [{
    id: "tracker-control",
    label: "Value tracker",
    signal: "tracker-value",
    min: -2,
    max: 4,
    step: 0.05,
    default: -2,
    timeline: true,
  }];
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.controls[0].timeline, true);
});

test("controls cannot target undeclared signals", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.controls[0].signal = "missing";
  assert.throws(
    () => parseSceneCode(`scene(${JSON.stringify(scene)});`),
    /references missing signal/,
  );
});

test("3D meshes and rotation tracks validate", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "tetrahedron",
    type: "mesh",
    vertices: [[1, 1, 1], [-1, -1, 1], [-1, 1, -1], [1, -1, -1]],
    triangles: [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    colors: ["#38bdf8", "#c084fc", "#f472b6", "#facc15"],
    normals: [[1, 1, 1], [-1, -1, 1], [-1, 1, -1], [1, -1, -1]],
    uvs: [[0, 0], [1, 0], [0, 1], [1, 1]],
    texturePixels: "/wAA/w==",
    textureWidth: 1,
    textureHeight: 1,
    darkTexturePixels: "AAD//w==",
    darkTextureWidth: 1,
    darkTextureHeight: 1,
    textureResampling: "nearest",
    gloss: 0.25,
    shadow: 0.35,
    lightPosition: [-10, 10, 10],
    style: { fill: "#38bdf8", stroke: null },
  });
  scene.tracks.push({
    target: "tetrahedron",
    property: "rotationY",
    keyframes: [{ at: 0, value: 0 }, { at: 6, value: Math.PI * 2 }],
  });
  scene.tracks.push({
    target: "tetrahedron",
    property: "vertices",
    keyframes: [
      { at: 0, value: scene.nodes.at(-1).vertices },
      {
        at: 6,
        value: scene.nodes.at(-1).vertices.map(([x, y, z]) => [x, y, z + 0.5]),
      },
    ],
  });
  scene.tracks.push({
    target: "tetrahedron",
    property: "lightPosition",
    keyframes: [
      { at: 0, value: [[-10, 10, 10]] },
      { at: 6, value: [[10, 10, 10]] },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).type, "mesh");
  assert.equal(parsed.nodes.at(-1).colors.length, 4);
  assert.equal(parsed.nodes.at(-1).normals.length, 4);
  assert.equal(parsed.nodes.at(-1).textureWidth, 1);
  assert.equal(parsed.nodes.at(-1).darkTextureWidth, 1);
  assert.equal(parsed.nodes.at(-1).gloss, 0.25);
  assert.equal(parsed.tracks.at(-1).property, "lightPosition");
});

test("camera-independent cubic 3D paths validate", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "orbit-path",
    type: "path3d",
    commands: [
      { op: "moveTo", x: -1, y: 0, z: 0 },
      { op: "cubicTo", c1x: -0.5, c1y: 1, c1z: 0.5, c2x: 0.5, c2y: 1, c2z: -0.5, x: 1, y: 0, z: 0 },
    ],
    style: { fill: null, stroke: "#38bdf8", strokeWidth: 0.05 },
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).type, "path3d");
  assert.equal(parsed.nodes.at(-1).commands[1].c2z, -0.5);
});

test("sliding cubic trace paths validate", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "trail",
    type: "tracePath",
    segments: [
      { start: [0, 0], control1: [0.3, 0], control2: [0.7, 0], end: [1, 0] },
      { start: [1, 0], control1: [1.3, 0.3], control2: [1.7, 0.7], end: [2, 1] },
    ],
    frames: [
      { at: 0, start: 0, count: 1 },
      { at: 1, start: 0, count: 2 },
      { at: 2, start: 1, count: 1 },
    ],
    style: { fill: null, stroke: "#38bdf8" },
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).segments.length, 2);
  assert.equal(parsed.nodes.at(-1).frames[2].start, 1);
});

test("compact Manim surface patches validate without expanded mesh geometry", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "surface",
    type: "surface",
    vertices: [[-1, -1, 0], [1, -1, 0], [1, 1, 0], [-1, 1, 0]],
    patches: [[0, 1, 2, 3]],
    colors: ["#2563ebcc", "#2563ebcc", "#1d4ed8cc", "#1d4ed8cc"],
    strokeColors: ["#ffffff"],
    strokeRadii: [0.004],
    unlit: true,
    doubleSided: true,
    style: { fill: "#2563eb", stroke: null, opacity: 0.8 },
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).type, "surface");
  assert.deepEqual(parsed.nodes.at(-1).patches, [[0, 1, 2, 3]]);
  assert.deepEqual(parsed.nodes.at(-1).strokeRadii, [0.004]);
});

test("3D camera vector and focal tracks validate", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.camera3d = {
    position: [0, -8, 4],
    target: [0, 0, 0],
    up: [0, 0, 1],
    fovY: 0.8,
    near: 0.1,
    far: 100,
    ambient: 0.28,
    lightDirection: [-0.4, 0.7, 1],
  };
  scene.tracks.push(
    {
      target: "__camera__",
      property: "camera3dPosition",
      keyframes: [
        { at: 0, value: [0, -8, 4] },
        { at: 6, value: [6, -4, 5], easing: "smooth" },
      ],
    },
    {
      target: "__camera__",
      property: "camera3dUp",
      keyframes: [
        { at: 0, value: [0, 0, 1] },
        { at: 6, value: [0, 1, 0] },
      ],
    },
    {
      target: "__camera__",
      property: "camera3dFovY",
      keyframes: [{ at: 0, value: 0.8 }, { at: 6, value: 1.1 }],
    },
  );
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.deepEqual(parsed.camera3d.position, [0, -8, 4]);
  assert.equal(parsed.tracks.at(-3).property, "camera3dPosition");
  assert.equal(parsed.tracks.at(-1).property, "camera3dFovY");
});

test("atomic 3D camera state validates", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.tracks.push({
    target: "__camera__",
    property: "camera3d",
    keyframes: [
      { at: 0, value: [15, 0, -8, 4, 0, 0, 0, 0, 0, 1, 0.8] },
      { at: 6, value: [15, 4, -4, 8, 2, 0, 1, 0, 1, 0, 1.2] },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-1).property, "camera3d");
  assert.equal(parsed.tracks.at(-1).keyframes[1].value.length, 11);
});

test("roll-free 3D orbit tracks validate", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.tracks.push({
    target: "__camera__",
    property: "camera3dOrbit",
    keyframes: [
      { at: 0, value: [4, -8, 4] },
      { at: 6, value: [0, -4, 8] },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-1).property, "camera3dOrbit");
});

test("native billboard groups validate a 3D anchor and 2D base", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({ id: "billboard", type: "billboard", anchor: [2, 0, 1], base: [1.5, 0.25] });
  scene.nodes[1].parent = "billboard";
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.deepEqual(parsed.nodes.at(-1).anchor, [2, 0, 1]);
  assert.deepEqual(parsed.nodes.at(-1).base, [1.5, 0.25]);
});

test("native billboard anchors animate in world space", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({ id: "billboard", type: "billboard", anchor: [2, 0, 1], base: [1.5, 0.25] });
  scene.nodes[1].parent = "billboard";
  scene.tracks.push({
    target: "billboard",
    property: "billboardAnchor",
    keyframes: [
      { at: 0, value: [2, 0, 1] },
      { at: 6, value: [2, 1, 2] },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.deepEqual(parsed.tracks.at(-1).keyframes[1].value, [2, 1, 2]);
});

test("compact surfaces animate topology-stable vertices and materials", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "surface",
    type: "surface",
    vertices: [[-1, -1, 0], [1, -1, 0], [1, 1, 0], [-1, 1, 0]],
    patches: [[0, 1, 2, 3]],
    colors: ["#2563ebcc", "#2563ebcc", "#1d4ed8cc", "#1d4ed8cc"],
    strokeColors: ["#ffffffff"],
    strokeRadii: [0.004],
    unlit: true,
    doubleSided: true,
    style: { fill: "#2563eb", stroke: null, opacity: 0.8 },
  });
  scene.tracks.push(
    {
      target: "surface",
      property: "vertices",
      keyframes: [
        { at: 0, value: [[-1, -1, 0], [1, -1, 0], [1, 1, 0], [-1, 1, 0]] },
        { at: 6, value: [[-1, -1, 0], [1, -1, 0], [1, 1, 1], [-1, 1, 0]] },
      ],
    },
    {
      target: "surface",
      property: "surfaceColors",
      keyframes: [
        { at: 0, value: ["#2563ebcc", "#2563ebcc", "#1d4ed8cc", "#1d4ed8cc", "#ffffffff"] },
        { at: 6, value: ["#ef4444cc", "#ef4444cc", "#dc2626cc", "#dc2626cc", "#facc15ff"] },
      ],
    },
    {
      target: "surface",
      property: "strokeRadii",
      keyframes: [{ at: 0, value: [0.004] }, { at: 6, value: [0.02] }],
    },
  );
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-2).property, "surfaceColors");
  assert.equal(parsed.tracks.at(-1).keyframes[1].value[0], 0.02);

  scene.tracks.at(-2).keyframes[1].value = ["#ef4444cc"];
  assert.throws(
    () => parseSceneCode(formatSceneCode(scene)),
    /surface topology/,
  );
});

test("text selects real bold and italic font faces", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "emphasis",
    type: "text",
    text: "Bold italic",
    fontSize: 0.8,
    weight: "bold",
    slant: "italic",
    style: { fill: "#ffffff", stroke: null },
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).weight, "bold");
  assert.equal(parsed.nodes.at(-1).slant, "italic");
});

test("markup text validates independently styled OpenType spans", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "markup",
    type: "markupText",
    spans: [
      { text: "velocity ", color: "#e2e8f0" },
      { text: "v", color: "#38bdf8", weight: "bold", slant: "italic" },
      { text: " = 12 m/s", color: "#f8fafc" },
    ],
    fontSize: 0.8,
    align: "center",
    style: { fill: "#ffffff", stroke: null },
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).type, "markupText");
  assert.equal(parsed.nodes.at(-1).spans[1].weight, "bold");
  assert.equal(parsed.nodes.at(-1).spans[1].slant, "italic");
});

test("cubic path commands can be animated for Manim-compatible morphs", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "morph",
    type: "path",
    commands: [
      { op: "moveTo", x: 0, y: 0 },
      { op: "cubicTo", c1x: 1, c1y: 0, c2x: 1, c2y: 1, x: 2, y: 1 },
    ],
    style: { fill: null, stroke: "#38bdf8" },
  });
  scene.tracks.push({
    target: "morph",
    property: "commands",
    keyframes: [
      {
        at: 0,
        value: [
          { op: "moveTo", x: 0, y: 0 },
          { op: "cubicTo", c1x: 1, c1y: 0, c2x: 1, c2y: 1, x: 2, y: 1 },
        ],
      },
      {
        at: 6,
        value: [
          { op: "moveTo", x: 0, y: 1 },
          { op: "cubicTo", c1x: 1, c1y: 1, c2x: 1, c2y: 2, x: 2, y: 2 },
        ],
      },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-1).property, "commands");
  assert.equal(parsed.tracks.at(-1).keyframes[1].value[1].op, "cubicTo");
});

test("compact path data reuses retained path topology", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "compact-morph",
    type: "path",
    commands: [
      { op: "moveTo", x: 0, y: 0 },
      { op: "cubicTo", c1x: 1, c1y: 0, c2x: 1, c2y: 1, x: 2, y: 1 },
      { op: "close" },
    ],
  });
  scene.tracks.push({
    target: "compact-morph",
    property: "pathData",
    keyframes: [
      { at: 0, value: [0, 0, 1, 0, 1, 1, 2, 1] },
      { at: 6, value: [0, 2, 1, 2, 1, 3, 2, 3] },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-1).property, "pathData");
  assert.equal(parsed.tracks.at(-1).keyframes[1].value.length, 8);
  scene.tracks.at(-1).keyframes[1].value.pop();
  assert.throws(() => parseSceneCode(formatSceneCode(scene)), /topology/);
});

test("composite 2D transform tracks validate as one retained operation", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.tracks.push({
    target: "ring",
    property: "transform2d",
    keyframes: [
      { at: 0, value: [0, 0, 0, 1, 1] },
      { at: 6, value: [2, -1, Math.PI, 1.5, 1.5] },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-1).property, "transform2d");
  assert.equal(parsed.tracks.at(-1).keyframes[1].value[2], Math.PI);
});

test("full affine 2D tracks retain shear without path snapshots", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.tracks.push({
    target: "ring",
    property: "affine2d",
    keyframes: [
      { at: 0, value: [1, 0, 0, 1, 0, 0] },
      { at: 6, value: [1, 0.4, 0.25, 1.2, 2, -1] },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.tracks.at(-1).property, "affine2d");
  assert.deepEqual(parsed.tracks.at(-1).keyframes[1].value, [1, 0.4, 0.25, 1.2, 2, -1]);
});

test("cubic paths expose an independently animated visible draw range", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes[1].style.drawStart = 0.2;
  scene.nodes[1].style.drawProgress = 0.5;
  scene.tracks.push({
    target: "ring",
    property: "drawStart",
    keyframes: [
      { at: 0, value: 0 },
      { at: 6, value: 0.7 },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes[1].style.drawStart, 0.2);
  assert.equal(parsed.tracks.at(-1).property, "drawStart");
});

test("native stroke cap join dash and animated offset validate", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes[1].style.strokeCap = "round";
  scene.nodes[1].style.strokeJoin = "bevel";
  scene.nodes[1].style.dashArray = [0.5, 0.2, 0.1];
  scene.nodes[1].style.dashOffset = -0.25;
  scene.tracks.push({
    target: "ring",
    property: "dashOffset",
    keyframes: [
      { at: 0, value: -0.25 },
      { at: 6, value: 1.5 },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes[1].style.strokeCap, "round");
  assert.equal(parsed.nodes[1].style.strokeJoin, "bevel");
  assert.deepEqual(parsed.nodes[1].style.dashArray, [0.5, 0.2, 0.1]);
  assert.equal(parsed.tracks.at(-1).property, "dashOffset");
  assert.throws(() => {
    const invalid = structuredClone(scene);
    invalid.nodes[1].style.dashArray = [0];
    parseSceneCode(formatSceneCode(invalid));
  }, /dashArray/);
});

test("cubic paths accept one atomic draw-window track", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.tracks.push({
    target: "ring",
    property: "drawRange",
    keyframes: [
      { at: 0, value: [0, 0.25] },
      { at: 6, value: [0.75, 1] },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.deepEqual(parsed.tracks.at(-1).keyframes[1].value, [0.75, 1]);
});

test("retained path references reuse concrete geometry", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "shared-path",
    type: "path",
    commands: [
      { op: "moveTo", x: 0, y: 0 },
      { op: "cubicTo", c1x: 1, c1y: 0, c2x: 1, c2y: 1, x: 2, y: 1 },
    ],
  });
  scene.nodes.push({ id: "shared-copy", type: "pathRef", source: "shared-path" });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).source, "shared-path");
  scene.nodes.at(-1).source = "missing";
  assert.throws(() => parseSceneCode(formatSceneCode(scene)), /concrete path/);
});

test("linear fill gradients validate and animate", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  const gradient = {
    from: [-1, 0],
    to: [1, 0],
    spread: "reflect",
    stops: [
      { offset: 0, color: "#38bdf8cc" },
      { offset: 1, color: "#c084fccc" },
    ],
  };
  scene.nodes[1].style.fillGradient = gradient;
  scene.tracks.push({
    target: "ring",
    property: "fillGradient",
    keyframes: [
      { at: 0, value: gradient },
      {
        at: 6,
        value: {
          ...gradient,
          from: [0, -1],
          to: [0, 1],
        },
      },
    ],
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes[1].style.fillGradient.stops.length, 2);
  assert.equal(parsed.nodes[1].style.fillGradient.spread, "reflect");
  assert.equal(parsed.tracks.at(-1).property, "fillGradient");
});

test("raw RGBA image nodes validate dimensions, corners, and resampling", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "image",
    type: "image",
    pixels: Buffer.from([
      255, 0, 0, 255,
      0, 0, 255, 128,
    ]).toString("base64"),
    pixelWidth: 2,
    pixelHeight: 1,
    corners: [[-1, 0.5], [1, 0.5], [-1, -0.5], [1, -0.5]],
    resampling: "bicubic",
    style: { fill: null, stroke: null },
  });
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).type, "image");
  assert.equal(parsed.nodes.at(-1).pixelWidth, 2);
  assert.equal(parsed.nodes.at(-1).resampling, "bicubic");
});

test("all Pillow and Manim image reconstruction filters validate", () => {
  const filters = ["nearest", "box", "bilinear", "hamming", "bicubic", "lanczos"];
  for (const resampling of filters) {
    const scene = structuredClone(DEFAULT_SCENE);
    scene.nodes.push({
      id: `filter-${resampling}`,
      type: "image",
      pixels: "/wAA/w==",
      pixelWidth: 1,
      pixelHeight: 1,
      corners: [[-1, 1], [-1, -1], [1, 1], [1, -1]],
      resampling,
      style: { fill: null, stroke: null },
    });
    assert.equal(
      parseSceneCode(formatSceneCode(scene)).nodes.at(-1).resampling,
      resampling,
    );
  }
});

test("audio clips and Manim subcaptions survive validation", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.audio = [
    {
      id: "tone",
      data: Buffer.from("RIFF").toString("base64"),
      mimeType: "audio/wav",
      startTime: 0.25,
      gainDb: -3,
    },
  ];
  scene.captions = [{ text: "A real subtitle", start: 0.5, end: 2 }];
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.audio[0].gainDb, -3);
  assert.equal(parsed.captions[0].text, "A real subtitle");
});

test("translated custom shaders, textures, and dynamic buffers validate", () => {
  const scene = structuredClone(DEFAULT_SCENE);
  scene.nodes.push({
    id: "plugin-shader",
    type: "customShaderMesh",
    vertexWgsl: `
      struct Out { @builtin(position) position: vec4<f32> }
      @vertex fn main(@location(0) point: vec2<f32>) -> Out {
        return Out(vec4<f32>(point, 0.0, 1.0));
      }
    `,
    fragmentWgsl: `
      @fragment fn main() -> @location(0) vec4<f32> {
        return vec4<f32>(1.0, 0.0, 1.0, 1.0);
      }
    `,
    attributes: [
      { name: "point", location: 0, offset: 0, format: "float32x2" },
    ],
    vertexStride: 8,
    vertexData: [-0.5, -0.5, 0.5, -0.5, 0, 0.5],
    indices: [0, 1, 2],
    primitive: "triangle-list",
    uniforms: [
      { name: "phase", binding: 0, type: "float", values: [0] },
      {
        name: "weights",
        binding: 1,
        type: "vec2",
        arrayLength: 2,
        values: [0.25, 0.75, 0.5, 0.5],
      },
      {
        name: "image",
        binding: 2,
        samplerBinding: 3,
        type: "sampler2D",
        values: [],
        texturePixels: "/wAA/w==",
        textureWidth: 1,
        textureHeight: 1,
      },
    ],
    depthTest: true,
    style: { fill: null, stroke: null },
  });
  scene.tracks.push(
    {
      target: "plugin-shader",
      property: "shaderVertexData",
      keyframes: [
        { at: 0, value: [-0.5, -0.5, 0.5, -0.5, 0, 0.5] },
        { at: 6, value: [-0.4, -0.4, 0.6, -0.4, 0.1, 0.6] },
      ],
    },
    {
      target: "plugin-shader",
      property: "shaderUniformValues",
      keyframes: [
        { at: 0, value: [0, 0.25, 0.75, 0.5, 0.5] },
        { at: 6, value: [Math.PI * 2, 0.5, 0.5, 0.75, 0.25] },
      ],
    },
  );
  const parsed = parseSceneCode(formatSceneCode(scene));
  assert.equal(parsed.nodes.at(-1).type, "customShaderMesh");
  assert.equal(parsed.nodes.at(-1).uniforms[1].arrayLength, 2);
  assert.equal(parsed.nodes.at(-1).uniforms[2].type, "sampler2D");
  assert.equal(parsed.tracks.at(-1).property, "shaderUniformValues");
});
