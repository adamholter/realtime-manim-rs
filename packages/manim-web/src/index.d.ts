export type Point = readonly [number, number];
export type Point3D = readonly [number, number, number];
export type Easing = "linear" | "smooth" | "manimSmooth" | "easeIn" | "easeOut" | "easeInOut" | "thereAndBack" | "bounce";
export type TextAlign = "left" | "center" | "right";
export type FontWeight = "normal" | "bold";
export type FontSlant = "normal" | "italic";
export type ImageResampling = "nearest" | "box" | "bilinear" | "hamming" | "bicubic" | "lanczos";
export type StrokeCap = "butt" | "square" | "round";
export type StrokeJoin = "miter" | "miterClip" | "round" | "bevel";
export interface MobjectBounds {
  readonly minX: number;
  readonly minY: number;
  readonly maxX: number;
  readonly maxY: number;
  readonly width: number;
  readonly height: number;
  readonly center: Point;
}
export interface LayoutFrame {
  readonly width?: number;
  readonly height?: number;
  readonly center?: Point;
}
export interface ArrangeOptions {
  readonly buff?: number;
  readonly alignedEdge?: Point;
  readonly center?: boolean;
}
export type RgbaPixels = string | ArrayBuffer | Uint8Array | Uint8ClampedArray;
export type RgbaTexture =
  | { pixels: RgbaPixels; data?: never; width: number; height: number }
  | { data: RgbaPixels; pixels?: never; width: number; height: number };
export type Property =
  | "x" | "y" | "z" | "rotation" | "rotationX" | "rotationY"
  | "scaleX" | "scaleY" | "scaleZ" | "opacity" | "strokeWidth" | "dashOffset"
  | "drawStart" | "drawProgress" | "drawRange" | "fill" | "fillGradient"
  | "stroke" | "strokeGradient" | "radius" | "points" | "vertices"
  | "normals" | "colors" | "surfaceColors" | "strokeRadii" | "lightPosition"
  | "commands" | "pathData" | "transform2d" | "affine2d" | "billboardAnchor"
  | "cameraX" | "cameraY" | "cameraZoom" | "cameraRotation" | "camera3dPosition"
  | "camera3dTarget" | "camera3dUp" | "camera3dFovY" | "camera3d"
  | "camera3dOrbit" | "shaderVertexData" | "shaderUniformValues";

export interface GradientStop { offset: number; color: string }
export interface LinearGradient {
  from: Point;
  to: Point;
  stops: GradientStop[];
  spread?: "pad" | "repeat" | "reflect";
  space?: "local" | "world";
}
export interface Transform {
  x: number;
  y: number;
  z: number;
  rotation: number;
  rotationX: number;
  rotationY: number;
  scaleX: number;
  scaleY: number;
  scaleZ: number;
}
export interface Style {
  fill: string | null;
  fillGradient: LinearGradient | null;
  stroke: string | null;
  strokeGradient: LinearGradient | null;
  strokeWidth: number;
  strokeCap: StrokeCap;
  strokeJoin: StrokeJoin;
  dashArray: number[];
  dashOffset: number;
  opacity: number;
  drawStart: number;
  drawProgress: number;
}

export type PathCommand =
  | { op: "moveTo" | "lineTo"; x: number; y: number }
  | { op: "quadTo"; cx: number; cy: number; x: number; y: number }
  | { op: "cubicTo"; c1x: number; c1y: number; c2x: number; c2y: number; x: number; y: number }
  | { op: "close" };
export type PathCommand3D =
  | { op: "moveTo" | "lineTo"; x: number; y: number; z: number }
  | { op: "quadTo"; cx: number; cy: number; cz: number; x: number; y: number; z: number }
  | { op: "cubicTo"; c1x: number; c1y: number; c1z: number; c2x: number; c2y: number; c2z: number; x: number; y: number; z: number }
  | { op: "close" };

export interface BaseNode {
  id: string;
  type: string;
  parent?: string;
  zIndex?: number;
  transform?: Partial<Transform>;
  style?: Partial<Style>;
  appearAt?: number;
  disappearAt?: number;
  [key: string]: unknown;
}
export interface GroupNode extends BaseNode { type: "group" }
export interface BillboardNode extends BaseNode { type: "billboard"; anchor: Point3D; base: Point }
export interface CircleNode extends BaseNode { type: "circle"; radius: number }
export interface RectangleNode extends BaseNode { type: "rect"; width: number; height: number; cornerRadius: number }
export interface LineNode extends BaseNode { type: "line"; from: Point; to: Point }
export interface ArrowNode extends BaseNode { type: "arrow"; from: Point; to: Point; tipSize: number }
export interface PolylineNode extends BaseNode { type: "polyline"; points: Point[]; closed: boolean }
export interface PathNode extends BaseNode { type: "path"; commands: PathCommand[] }
export interface Path3DNode extends BaseNode { type: "path3d"; commands: PathCommand3D[] }
export interface TraceSegment { start: Point; control1: Point; control2: Point; end: Point }
export interface TraceFrame { at: number; start: number; count: number; closed?: boolean }
export interface TracePathNode extends BaseNode { type: "tracePath"; segments: TraceSegment[]; frames: Array<Required<TraceFrame>> }
export interface PathReferenceNode extends BaseNode { type: "pathRef"; source: string }
export interface TextNode extends BaseNode {
  type: "text";
  text: string;
  fontSize: number;
  fontFamily: string;
  align: TextAlign;
  weight: FontWeight;
  slant: FontSlant;
}
export type TextUnderline = "none" | "single" | "double" | "low" | "error";
export interface TextSpan {
  text: string;
  color?: string;
  weight?: FontWeight;
  slant?: FontSlant;
  fontFamily?: string;
  fontScale?: number;
  rise?: number;
  letterSpacing?: number;
  background?: string;
  underline?: TextUnderline;
  underlineColor?: string;
  strikethrough?: boolean;
  strikethroughColor?: string;
}
export interface MarkupTextNode extends BaseNode {
  type: "markupText";
  spans: TextSpan[];
  markup?: string | null;
  fontSize: number;
  fontFamily: string;
  align: TextAlign;
}
export interface SVGNode extends BaseNode { type: "svg"; svg: string; height: number; preserveStyles: boolean }
export interface ImageNode extends BaseNode {
  type: "image";
  pixels: string;
  pixelWidth: number;
  pixelHeight: number;
  corners: Point[];
  resampling: ImageResampling;
}
export interface PointMark { x: number; y: number; color?: string; radius?: number }
export interface PointCloudNode extends BaseNode { type: "pointCloud"; points: PointMark[]; radius: number; screenSpaceRadius: boolean }
export interface MeshNode extends BaseNode {
  type: "mesh";
  vertices: Point3D[];
  triangles: Array<[number, number, number]>;
  colors: string[];
  normals: Point3D[];
  uvs: Point[];
  texturePixels: string;
  textureWidth: number;
  textureHeight: number;
  darkTexturePixels: string;
  darkTextureWidth: number;
  darkTextureHeight: number;
  textureResampling: ImageResampling;
  gloss: number;
  shadow: number;
  lightPosition: Point3D;
  unlit: boolean;
  doubleSided: boolean;
}
export interface SurfaceNode extends BaseNode {
  type: "surface";
  vertices: Point3D[];
  patches: number[][];
  colors: string[];
  strokeColors: string[];
  strokeRadii: number[];
  unlit: boolean;
  doubleSided: boolean;
}
export type SceneNode =
  | GroupNode | BillboardNode | CircleNode | RectangleNode | LineNode | ArrowNode
  | PolylineNode | PathNode | Path3DNode | TracePathNode | PathReferenceNode | TextNode
  | MarkupTextNode | SVGNode | ImageNode | PointCloudNode | MeshNode | SurfaceNode | BaseNode;

export type TrackValue = number | string | number[] | Point[] | Point3D[] | PathCommand[] | LinearGradient;
export type Keyframe = { at: number; value: TrackValue; easing?: Easing };
export type Track = { target: string; property: Property | string; keyframes: Keyframe[]; keyframesFrom?: string };
export type NumberKeyframe = { at: number; value: number; easing?: Easing };
export type Expression =
  | { op: "constant"; value: number }
  | { op: "time" }
  | { op: "signal"; id: string }
  | { op: "add" | "multiply" | "min" | "max"; args: Expression[] }
  | { op: "subtract" | "divide"; left: Expression; right: Expression }
  | { op: "sin" | "cos" | "abs"; value: Expression }
  | { op: "clamp"; value: Expression; min: number; max: number }
  | { op: "lerp"; from: Expression; to: Expression; amount: Expression };

export interface Camera2D {
  x?: number;
  y?: number;
  zoom?: number;
  rotation?: number;
}
export interface Camera3D {
  position?: Point3D;
  target?: Point3D;
  up?: Point3D;
  fovY?: number;
  near?: number;
  far?: number;
  ambient?: number;
  lightDirection?: Point3D;
}
export interface AudioClip { id: string; data: string; mimeType: string; startTime: number; gainDb?: number }
export interface Caption { text: string; start: number; end: number }
export interface Correspondence {
  id: string;
  kind: "transformMatchingTex" | "transformMatchingShapes";
  mode: "transform" | "keyMapped" | "transformMismatches" | "fadeTransformMismatches" | "fadeOut" | "fadeIn";
  keys: string[];
  targetKeys: string[];
  sourceNodes: string[];
  targetNodes: string[];
  start: number;
  end: number;
  pathArc: number;
}
export interface SceneData {
  version: number;
  title: string;
  width: number;
  height: number;
  pixelWidth?: number;
  pixelHeight?: number;
  duration: number;
  fps: number;
  background: string;
  camera?: Camera2D;
  camera3d?: Camera3D;
  nodes: SceneNode[];
  tracks: Track[];
  signals: Array<{ id: string; keyframes: NumberKeyframe[] }>;
  bindings: Array<{ target: string; property: Property | string; expression: Expression }>;
  controls: Array<{ id: string; label: string; signal: string; min: number; max: number; step: number; default: number; timeline?: boolean }>;
  audio: AudioClip[];
  captions: Caption[];
  correspondences?: Correspondence[];
}

export interface BaseMobjectOptions {
  id?: string;
  style?: Partial<Style>;
  transform?: Partial<Transform>;
  parent?: string;
  zIndex?: number;
  appearAt?: number;
  disappearAt?: number;
}
export interface MobjectOptions extends BaseMobjectOptions {
  [key: string]: unknown;
}

export interface ImageOptions extends BaseMobjectOptions {
  corners?: [Point, Point, Point, Point];
  resampling?: ImageResampling;
}
export interface ImageSourceOptions extends ImageOptions { crossOrigin?: string | null }
export interface MeshOptions extends BaseMobjectOptions {
  colors?: string[];
  normals?: Point3D[];
  uvs?: Point[];
  texture?: RgbaTexture;
  texturePixels?: RgbaPixels;
  textureWidth?: number;
  textureHeight?: number;
  darkTexture?: RgbaTexture;
  darkTexturePixels?: RgbaPixels;
  darkTextureWidth?: number;
  darkTextureHeight?: number;
  textureResampling?: ImageResampling;
  gloss?: number;
  shadow?: number;
  lightPosition?: Point3D;
  unlit?: boolean;
  doubleSided?: boolean;
}
export interface SurfaceOptions extends BaseMobjectOptions {
  colors: string[];
  strokeColors: string[];
  strokeRadii: number[];
  unlit?: boolean;
  doubleSided?: boolean;
}
export interface MathTexOptions extends BaseMobjectOptions { fontSize?: number; height?: number }
export interface MathTexTypesetOptions extends MathTexOptions {
  endpoint?: string | URL;
  fetch?: typeof globalThis.fetch;
  signal?: AbortSignal;
}

export type NumericRange = readonly [number, number] | readonly [number, number, number];
export interface NumberLabelOptions {
  fontSize?: number;
  fontFamily?: string;
  align?: TextAlign;
  weight?: FontWeight;
  slant?: FontSlant;
  style?: Partial<Style>;
  zIndex?: number;
}
export interface NumberLineOptions extends BaseMobjectOptions {
  xRange?: NumericRange;
  length?: number;
  direction?: Point;
  includeTicks?: boolean;
  tickSize?: number;
  includeTip?: boolean;
  tipSize?: number;
  includeNumbers?: boolean;
  numbersToInclude?: number[];
  numbersToExclude?: number[];
  numberLabelOptions?: NumberLabelOptions;
  labelDirection?: Point;
  labelBuff?: number;
  decimalPlaces?: number;
  numberFormatter?: (value: number) => string;
  axisStyle?: Partial<Style>;
  tickStyle?: Partial<Style>;
}
export type AxisConfig = Omit<NumberLineOptions, "id" | "parent" | "transform" | "xRange" | "length" | "direction">;
export interface AxesOptions extends BaseMobjectOptions {
  xRange?: NumericRange;
  yRange?: NumericRange;
  xLength?: number;
  yLength?: number;
  axisConfig?: AxisConfig;
  xAxisConfig?: AxisConfig;
  yAxisConfig?: AxisConfig;
  includeNumbers?: boolean;
  includeTips?: boolean;
  xLabel?: string | false | null;
  yLabel?: string | false | null;
  axisLabelOptions?: NumberLabelOptions;
  axisLabelBuff?: number;
}
export interface NumberPlaneOptions extends AxesOptions {
  backgroundLineStyle?: Partial<Style>;
  fadedLineStyle?: Partial<Style>;
  gridSubdivisions?: number;
}
export interface CoordinateSystem2D {
  coordsToPoint(point: Point): Point;
  coordsToPoint(x: number, y: number): Point;
  pointToCoords(point: Point): Point;
}
export interface ParametricFunctionOptions extends BaseMobjectOptions {
  tRange?: NumericRange;
  samples?: number;
  discontinuities?: number[];
  discontinuityThreshold?: number;
  coordinateSystem?: CoordinateSystem2D;
}
export interface FunctionGraphOptions extends Omit<ParametricFunctionOptions, "tRange"> {
  xRange?: NumericRange;
}
export type AxesPlotOptions = Omit<FunctionGraphOptions, "coordinateSystem">;
export type AxesParametricPlotOptions = Omit<ParametricFunctionOptions, "coordinateSystem">;

export const ORIGIN: Point;
export const UP: Point;
export const DOWN: Point;
export const LEFT: Point;
export const RIGHT: Point;
export const UL: Point;
export const UR: Point;
export const DL: Point;
export const DR: Point;
export const FRAME_WIDTH: number;
export const FRAME_HEIGHT: number;
export const DEFAULT_MOBJECT_TO_EDGE_BUFFER: number;
export const DEFAULT_MOBJECT_TO_MOBJECT_BUFFER: number;
export const pathCommand: {
  moveTo(point: Point): PathCommand;
  lineTo(point: Point): PathCommand;
  quadTo(control: Point, point: Point): PathCommand;
  cubicTo(control1: Point, control2: Point, point: Point): PathCommand;
  close(): PathCommand;
};
export const pathCommand3D: {
  moveTo(point: Point3D): PathCommand3D;
  lineTo(point: Point3D): PathCommand3D;
  quadTo(control: Point3D, point: Point3D): PathCommand3D;
  cubicTo(control1: Point3D, control2: Point3D, point: Point3D): PathCommand3D;
  close(): PathCommand3D;
};

export class Mobject<TNode extends BaseNode = BaseNode> {
  readonly id: string;
  readonly node: TNode;
  constructor(type: string, options?: MobjectOptions);
  copy(): this;
  getBounds(): MobjectBounds;
  getCenter(): Point;
  getWidth(): number;
  getHeight(): number;
  getLeft(): Point;
  getRight(): Point;
  getTop(): Point;
  getBottom(): Point;
  getCriticalPoint(direction: Point): Point;
  center(): this;
  moveTo(target: Mobject | Point, alignedEdge?: Point): this;
  moveTo(x: number, y: number): this;
  shift(offset: Point): this;
  shift(dx: number, dy: number): this;
  setX(value: number): this;
  setY(value: number): this;
  alignTo(target: Mobject | Point, direction?: Point): this;
  nextTo(target: Mobject | Point, direction?: Point, buff?: number, alignedEdge?: Point): this;
  toEdge(direction?: Point, buff?: number, frame?: LayoutFrame): this;
  toCorner(direction?: Point, buff?: number, frame?: LayoutFrame): this;
  scale(value: number): this;
  rotate(radians: number): this;
  rotateX(radians: number): this;
  rotateY(radians: number): this;
  fill(color: string | null): this;
  fillGradient(gradient: LinearGradient | null): this;
  stroke(color: string | null, width?: number): this;
  strokeGradient(gradient: LinearGradient | null, width?: number): this;
  strokeCap(value: StrokeCap): this;
  strokeJoin(value: StrokeJoin): this;
  dash(pattern?: readonly number[], offset?: number): this;
  opacity(value: number): this;
  zIndex(value: number): this;
  setParent(parent: Mobject | string | null): this;
  appearAt(value: number): this;
  disappearAt(value: number): this;
  toJSON(): TNode;
}
export class Circle extends Mobject<CircleNode> { constructor(options?: MobjectOptions & { radius?: number }); }
export class Dot extends Circle { constructor(options?: MobjectOptions & { radius?: number }); }
export class Ellipse extends Circle { constructor(options?: MobjectOptions & { width?: number; height?: number }); }
export class Rectangle extends Mobject<RectangleNode> { constructor(options?: MobjectOptions & { width?: number; height?: number; cornerRadius?: number }); }
export class Square extends Rectangle { constructor(options?: MobjectOptions & { size?: number }); }
export class RoundedRectangle extends Rectangle { constructor(options?: MobjectOptions & { width?: number; height?: number; cornerRadius?: number }); }
export class Line extends Mobject<LineNode> { constructor(from?: Point, to?: Point, options?: MobjectOptions); }
export class Arrow extends Mobject<ArrowNode> { constructor(from?: Point, to?: Point, options?: MobjectOptions & { tipSize?: number }); }
export class Polyline extends Mobject<PolylineNode> { constructor(points: Point[], options?: MobjectOptions & { closed?: boolean }); }
export class Polygon extends Polyline { constructor(points: Point[], options?: MobjectOptions); }
export class RegularPolygon extends Polygon { constructor(sides?: number, options?: MobjectOptions & { radius?: number; startAngle?: number }); }
export class Triangle extends RegularPolygon { constructor(options?: MobjectOptions & { radius?: number; startAngle?: number }); }
export class Path extends Mobject<PathNode> { constructor(commands: PathCommand[], options?: MobjectOptions); }
export class Path3D extends Mobject<Path3DNode> { constructor(commands: PathCommand3D[], options?: MobjectOptions); }
export class TracePath extends Mobject<TracePathNode> { constructor(segments: TraceSegment[], frames: TraceFrame[], options?: BaseMobjectOptions); }
export class QuadraticBezier extends Path { constructor(start: Point, control: Point, end: Point, options?: MobjectOptions); }
export class CubicBezier extends Path { constructor(start: Point, control1: Point, control2: Point, end: Point, options?: MobjectOptions); }
export class Arc extends Path { constructor(options?: MobjectOptions & { radius?: number; startAngle?: number; angle?: number; arcCenter?: Point }); }
export class Text extends Mobject<TextNode> { constructor(text: string, options?: MobjectOptions & { fontSize?: number; fontFamily?: string; align?: TextAlign; weight?: FontWeight; slant?: FontSlant }); }
export class MarkupText extends Mobject<MarkupTextNode> {
  constructor(markup: string, options?: MobjectOptions & { fontSize?: number; fontFamily?: string; align?: TextAlign });
  constructor(spans: TextSpan[], options?: MobjectOptions & { fontSize?: number; fontFamily?: string; align?: TextAlign });
}
export class SVG extends Mobject<SVGNode> { constructor(svg: string, options?: MobjectOptions & { height?: number; preserveStyles?: boolean }); }
export class MathTex extends SVG {
  constructor(compiledSvg: string, options?: MathTexOptions & { tex?: string });
  static typeset(tex: string, options?: MathTexTypesetOptions): Promise<MathTex>;
  readonly source?: string;
}
export class Image extends Mobject<ImageNode> {
  constructor(pixels: RgbaPixels, pixelWidth: number, pixelHeight: number, options?: ImageOptions);
  static fromImageData(imageData: Pick<ImageData, "data" | "width" | "height">, options?: ImageOptions): Image;
  static fromSource(source: string | URL | CanvasImageSource, options?: ImageSourceOptions): Promise<Image>;
}
export class PointCloud extends Mobject<PointCloudNode> { constructor(points: Array<Point | PointMark>, options?: MobjectOptions & { radius?: number; screenSpaceRadius?: boolean }); }
export class DotCloud extends PointCloud {}
export class Mesh extends Mobject<MeshNode> {
  constructor(vertices: Point3D[], triangles: Array<[number, number, number]>, options?: MeshOptions);
}
export class Surface extends Mobject<SurfaceNode> {
  constructor(vertices: Point3D[], patches: number[][], options: SurfaceOptions);
}
export class RetainedNode<TNode extends BaseNode = BaseNode> extends Mobject<TNode> {
  constructor(node: TNode);
  static from<TNode extends BaseNode>(node: TNode): RetainedNode<TNode>;
}
export class Billboard extends Mobject<BillboardNode> { constructor(anchor: Point3D, base?: Point, options?: MobjectOptions); }
export class PathReference extends Mobject<PathReferenceNode> { constructor(source: Path | string, options?: MobjectOptions); }
export class Group extends Mobject<GroupNode> {
  constructor(...items: Array<Mobject | readonly Mobject[] | MobjectOptions>);
  readonly members: readonly Mobject[];
  add(...items: Array<Mobject | readonly Mobject[]>): this;
  fill(color: string | null): this;
  fillGradient(gradient: LinearGradient | null): this;
  stroke(color: string | null, width?: number): this;
  strokeGradient(gradient: LinearGradient | null, width?: number): this;
  strokeCap(value: StrokeCap): this;
  strokeJoin(value: StrokeJoin): this;
  dash(pattern?: readonly number[], offset?: number): this;
  opacity(value: number): this;
  arrange(direction?: Point, options?: ArrangeOptions): this;
}
export class VGroup extends Group {}

export class NumberLine extends VGroup {
  readonly axis: Line | Arrow;
  readonly ticks: VGroup;
  readonly numbers: VGroup;
  readonly xRange: Readonly<[number, number, number]>;
  readonly length: number;
  readonly unitSize: number;
  readonly direction: Readonly<Point>;
  constructor(options?: NumberLineOptions);
  getTickValues(): number[];
  numberToPoint(value: number): Point;
  n2p(value: number): Point;
  pointToNumber(point: Point): number;
  p2n(point: Point): number;
}

export class ParametricFunction extends Path {
  readonly function: (parameter: number) => Point;
  readonly tRange: Readonly<NumericRange>;
  readonly sampleCount: number;
  readonly segments: ReadonlyArray<ReadonlyArray<Readonly<Point>>>;
  readonly discontinuities: ReadonlyArray<number>;
  readonly coordinateSystem?: CoordinateSystem2D;
  constructor(fn: (parameter: number) => Point, options?: ParametricFunctionOptions);
  constructor(fn: (parameter: number) => Point, tRange: NumericRange, options?: Omit<ParametricFunctionOptions, "tRange">);
  pointAt(parameter: number): Point;
}

export class FunctionGraph extends ParametricFunction {
  readonly underlyingFunction: (x: number) => number;
  readonly xRange: Readonly<NumericRange>;
  constructor(fn: (x: number) => number, options?: FunctionGraphOptions);
  constructor(fn: (x: number) => number, xRange: NumericRange, options?: Omit<FunctionGraphOptions, "xRange">);
  valueAt(x: number): number;
}

export class Axes extends VGroup implements CoordinateSystem2D {
  readonly xAxis: NumberLine;
  readonly yAxis: NumberLine;
  readonly axisLabels: VGroup;
  readonly xRange: Readonly<[number, number, number]>;
  readonly yRange: Readonly<[number, number, number]>;
  readonly xLength: number;
  readonly yLength: number;
  constructor(options?: AxesOptions);
  coordsToPoint(point: Point): Point;
  coordsToPoint(x: number, y: number): Point;
  c2p(point: Point): Point;
  c2p(x: number, y: number): Point;
  pointToCoords(point: Point): Point;
  p2c(point: Point): Point;
  getOrigin(): Point;
  plot(fn: (x: number) => number, options?: AxesPlotOptions): FunctionGraph;
  plot(fn: (x: number) => number, xRange: NumericRange, options?: Omit<AxesPlotOptions, "xRange">): FunctionGraph;
  plotParametric(fn: (parameter: number) => Point, options?: AxesParametricPlotOptions): ParametricFunction;
  plotParametric(fn: (parameter: number) => Point, tRange: NumericRange, options?: Omit<AxesParametricPlotOptions, "tRange">): ParametricFunction;
}

export class NumberPlane extends Axes {
  readonly gridLines: VGroup;
  constructor(options?: NumberPlaneOptions);
}

export interface AnimationOptions { start?: number; duration?: number; easing?: Easing }
export interface CameraAnimationOptions extends AnimationOptions { from?: Camera2D }
export interface OrbitCameraOptions extends AnimationOptions { from?: Point3D }
export type Animation = Track | Track[];
export interface AnimationGroupOptions {
  start?: number;
  duration?: number;
  runTime?: number;
  lagRatio?: number;
}
export interface ScenePlayOptions {
  duration?: number;
  runTime?: number;
  lagRatio?: number;
}
export function animate(target: Group, property: Property | string, from: TrackValue, to: TrackValue, options?: AnimationOptions): Track | Track[];
export function animate(target: Mobject | string, property: Property | string, from: TrackValue, to: TrackValue, options?: AnimationOptions): Track;
export function FadeIn(target: Group, options?: AnimationOptions): Track[];
export function FadeIn(target: Mobject | string, options?: AnimationOptions): Track;
export function FadeOut(target: Group, options?: AnimationOptions): Track[];
export function FadeOut(target: Mobject | string, options?: AnimationOptions): Track;
export function Create(target: Group, options?: AnimationOptions): Track[];
export function Create(target: Mobject | string, options?: AnimationOptions): Track;
export function Rotate(target: Mobject | string, radians?: number, options?: AnimationOptions & { from?: number }): Track;
export function MoveTo(target: Mobject | string, point: Point, options?: AnimationOptions & { from?: Point }): Track[];
export function MoveCamera(to: Camera2D, options?: CameraAnimationOptions): Track[];
export function OrbitCamera(position: Point3D, options?: OrbitCameraOptions): Track;
export function Transform(source: Mobject, target: Mobject, options?: AnimationOptions): Track[];
export function ReplacementTransform(source: Mobject, target: Mobject, options?: AnimationOptions): Track[];
export function AnimationGroup(...animations: Animation[]): Track[];
export function AnimationGroup(...animationsAndOptions: [...Animation[], AnimationGroupOptions]): Track[];
export function LaggedStart(...animations: Animation[]): Track[];
export function LaggedStart(...animationsAndOptions: [...Animation[], AnimationGroupOptions]): Track[];
export function Succession(...animations: Animation[]): Track[];
export function Succession(...animationsAndOptions: [...Animation[], AnimationGroupOptions]): Track[];

export interface SceneOptions extends Partial<Omit<SceneData, "nodes" | "tracks" | "signals" | "bindings" | "controls" | "audio" | "captions" | "correspondences">> {
  nodes?: SceneNode[];
  tracks?: Track[];
  signals?: SceneData["signals"];
  bindings?: SceneData["bindings"];
  controls?: SceneData["controls"];
  audio?: SceneData["audio"];
  captions?: SceneData["captions"];
  correspondences?: SceneData["correspondences"];
}

type SceneMember = Mobject | SceneNode | Array<Mobject | SceneNode>;
export class Scene {
  readonly cursor: number;
  constructor(options?: SceneOptions);
  add(...objects: SceneMember[]): this;
  play(...animations: Animation[]): this;
  play(...animationsAndOptions: [...Animation[], ScenePlayOptions]): this;
  wait(seconds?: number): this;
  track(target: Mobject | string, property: Property | string, keyframes: Keyframe[]): this;
  signal(id: string, keyframes: NumberKeyframe[]): this;
  bind(target: Mobject | string, property: Property | string, expression: Expression): this;
  control(signal: string, options?: { id?: string; label?: string; min?: number; max?: number; step?: number; default?: number; timeline?: boolean }): this;
  setCamera(camera: Camera2D): this;
  setCamera3D(camera: Camera3D): this;
  toJSON(): SceneData;
  toString(): string;
}

export interface ManimPlayer {
  readonly canvas: HTMLCanvasElement;
  load(scene: Scene | SceneData | string): ManimPlayer;
  validate(scene: Scene | SceneData | string): SceneData;
  evaluate(scene: Scene | SceneData | string, time: number): unknown;
  play(): void;
  pause(): void;
  seek(time: number): void;
  time(): number;
  setSignal(id: string, value: number): void;
  registerFont(family: string, data: ArrayBuffer | Uint8Array, options?: { weight?: FontWeight; slant?: FontSlant }): ManimPlayer;
  setSize(width: number, height: number): void;
  clearSize(): void;
  reset(): void;
  diagnostics(): { recoveryCount: number; registeredFontFaces: number; webgpu: true };
  destroy(): void;
}

export interface BrowserFontFace {
  family: string;
  data: ArrayBuffer | Uint8Array;
  weight?: FontWeight;
  slant?: FontSlant;
}

export interface CreateManimPlayerOptions {
  canvas: HTMLCanvasElement;
  scene?: Scene | SceneData | string;
  autoplay?: boolean;
  wasmUrl?: URL | string;
  fonts?: BrowserFontFace[];
}

export function createManimPlayer(options: CreateManimPlayerOptions): Promise<ManimPlayer>;
