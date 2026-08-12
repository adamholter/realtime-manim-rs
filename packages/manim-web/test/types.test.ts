import {
  AnimationGroup,
  Arc,
  Axes,
  Billboard,
  Circle,
  Create,
  DotCloud,
  FunctionGraph,
  Group,
  Image,
  MarkupText,
  LaggedStart,
  Line,
  MathTex,
  Mesh,
  MoveCamera,
  MoveTo,
  NumberLine,
  NumberPlane,
  ORIGIN,
  OrbitCamera,
  Path,
  Path3D,
  ParametricFunction,
  Polygon,
  RegularPolygon,
  ReplacementTransform,
  RIGHT,
  RetainedNode,
  Scene,
  Surface,
  Succession,
  TracePath,
  Transform,
  pathCommand,
  pathCommand3D,
  createManimPlayer,
  type Camera3D,
  type BrowserFontFace,
  type Expression,
  type ImageNode,
  type MeshNode,
  type NumericRange,
  type Point,
  type NumberLineOptions,
  type SurfaceNode,
  type SceneData,
} from "../src/index.js";

const circle = new Circle({ id: "typed", radius: 1 }).fill("#58c4ddff");
circle.strokeCap("round").strokeJoin("bevel").dash([0.4, 0.2], -0.1);
const customText = new MarkupText([{ text: "Typed" }], { fontFamily: "Uploaded Sans" });
const signal: Expression = { op: "signal", id: "radius" };
const scene = new Scene({ title: "Typed scene" })
  .add(circle)
  .play(Create(circle), MoveTo(circle, [2, 0]))
  .signal("radius", [{ at: 0, value: 1 }])
  .bind(circle, "radius", signal)
  .control("radius", { min: 0.25, max: 3, default: 1 });
const data: SceneData = scene.toJSON();

const path = new Path([
  pathCommand.moveTo([-2, 0]),
  pathCommand.cubicTo([-1, 2], [1, -2], [2, 0]),
]);
const path3d = new Path3D([
  pathCommand3D.moveTo([0, 0, 0]),
  pathCommand3D.lineTo([1, 1, 1]),
]);
const group = new Group([
  new Polygon([[-1, -1], [1, -1], [0, 1]]),
  new RegularPolygon(6, { radius: 1.5 }),
  new Arc({ radius: 2, angle: Math.PI }),
  path,
]);
const camera: Camera3D = { position: [0, -8, 5], target: [0, 0, 0] };
new Scene()
  .add(
    group,
    path3d,
    new MarkupText([{ text: "Rust", weight: "bold" }]),
    new DotCloud([[0, 0], { x: 1, y: 1, color: "#ffffffff" }]),
    new Billboard([0, 0, 2]),
  )
  .setCamera({ zoom: 1.5 })
  .setCamera3D(camera)
  .play(Create(group), MoveCamera({ x: 2 }), OrbitCamera([4, -6, 4]));

const transformTarget = new Circle({ id: "typed-target", radius: 2 }).moveTo([2, 0]);
const composed = AnimationGroup(
  Transform(circle, transformTarget),
  LaggedStart(Create(path), Create(path3d), { lagRatio: 0.2, runTime: 2 }),
);
new Scene()
  .add(circle, path, path3d)
  .play(composed, { runTime: 4 })
  .play(Succession(
    ReplacementTransform(circle, transformTarget),
    Create(group),
  ));

const image = new Image(new Uint8Array([255, 0, 0, 255]), 1, 1, {
  corners: [[-1, 1], [1, 1], [-1, -1], [1, -1]],
  resampling: "bicubic",
});
const imageNode: ImageNode = image.toJSON();
const browserImage = await Image.fromSource("/texture.png", { crossOrigin: "anonymous" });
const mesh = new Mesh(
  [[-1, -1, 0], [1, -1, 0], [0, 1, 0]],
  [[0, 1, 2]],
  { colors: ["#ff0000ff", "#00ff00ff", "#0000ffff"], doubleSided: true },
);
const meshNode: MeshNode = mesh.toJSON();
const surface = new Surface(
  [[-1, -1, 0], [1, -1, 0], [0, 1, 0]],
  [[0, 1, 2]],
  { colors: ["#ff0000ff", "#00ff00ff", "#0000ffff"], strokeColors: ["#ffffffff"], strokeRadii: [0.02] },
);
const surfaceNode: SurfaceNode = surface.toJSON();
const trace = new TracePath(
  [{ start: [0, 0], control1: [0, 1], control2: [1, 1], end: [1, 0] }],
  [{ at: 0, start: 0, count: 1 }],
);
const retained = RetainedNode.from({ id: "retained", type: "circle" as const, radius: 1 });
const equation = await MathTex.typeset(String.raw`e^{i\pi}+1=0`, { fontSize: 1.5 });
new Scene().add(image, browserImage, mesh, surface, trace, retained, equation, customText);
void imageNode;
void meshNode;
void surfaceNode;

const tickRange: NumericRange = [-3, 3, 0.5];
const numberLineOptions: NumberLineOptions = {
  xRange: tickRange,
  includeNumbers: true,
  numberFormatter: (value) => value.toFixed(1),
};
const numberLine = new NumberLine(numberLineOptions);
const numberPoint: Point = numberLine.n2p(1.5);
const numberValue: number = numberLine.p2n(numberPoint);
const axes = new Axes({
  xRange: [-4, 4, 1],
  yRange: [-2, 2, 0.5],
  includeNumbers: true,
  xLabel: "x",
  yLabel: "y",
});
const graph: FunctionGraph = axes.plot((x) => Math.sin(x), [-Math.PI, Math.PI], {
  samples: 257,
  discontinuities: [],
});
const parametric: ParametricFunction = axes.plotParametric(
  (t) => [Math.cos(t), Math.sin(t)],
  { tRange: [0, Math.PI * 2], samples: 257 },
);
const plane = new NumberPlane({ gridSubdivisions: 2 });
const coordinate: Point = plane.c2p([1, 2]);
new NumberLine({ direction: RIGHT, xRange: numberLine.xRange });
new Line(ORIGIN, RIGHT);
axes.c2p(ORIGIN);
axes.plot((x) => x, axes.xRange);
new Scene().add(numberLine, axes, graph, parametric, plane).play(Create(graph));
// @ts-expect-error graphing constructors reject unknown options
new Axes({ unknownGraphOption: true });
// @ts-expect-error coordinates are always a two-number point
plane.c2p([1, 2, 3]);
void numberValue;
void coordinate;

declare const canvas: HTMLCanvasElement;
const uploadedFont: BrowserFontFace = {
  family: "Uploaded Sans",
  data: new Uint8Array([0, 1, 2]),
  weight: "normal",
};
const player = await createManimPlayer({ canvas, scene: data, autoplay: false, fonts: [uploadedFont] });
const secondPlayer = await createManimPlayer({ canvas: document.createElement("canvas"), scene: data });
player.seek(0.5);
player.setSignal("radius", 2);
player.registerFont("Uploaded Sans", uploadedFont.data, { slant: "italic" });
player.destroy();
secondPlayer.destroy();
