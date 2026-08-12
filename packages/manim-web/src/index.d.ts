export type Point = [number, number];
export type Point3D = [number, number, number];
export type Easing = "linear" | "smooth" | "easeIn" | "easeOut" | "easeInOut" | "thereAndBack" | "bounce";
export type TextAlign = "left" | "center" | "right";
export type FontWeight = "normal" | "bold";
export type FontSlant = "normal" | "italic";
export type ImageResampling = "nearest" | "box" | "bilinear" | "hamming" | "bicubic" | "lanczos";
export type RgbaPixels = string | ArrayBuffer | Uint8Array | Uint8ClampedArray;
export type RgbaTexture =
  | { pixels: RgbaPixels; data?: never; width: number; height: number }
  | { data: RgbaPixels; pixels?: never; width: number; height: number };
export type Property =
  | "x" | "y" | "z" | "rotation" | "rotationX" | "rotationY"
  | "scaleX" | "scaleY" | "scaleZ" | "opacity" | "strokeWidth"
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
export interface TextSpan { text: string; color?: string; weight?: FontWeight; slant?: FontSlant }
export interface MarkupTextNode extends BaseNode { type: "markupText"; spans: TextSpan[]; fontSize: number; fontFamily: string; align: TextAlign }
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

export const ORIGIN: Readonly<Point>;
export const UP: Readonly<Point>;
export const DOWN: Readonly<Point>;
export const LEFT: Readonly<Point>;
export const RIGHT: Readonly<Point>;
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
  moveTo(point: Point): this;
  moveTo(x: number, y: number): this;
  shift(offset: Point): this;
  shift(dx: number, dy: number): this;
  scale(value: number): this;
  rotate(radians: number): this;
  rotateX(radians: number): this;
  rotateY(radians: number): this;
  fill(color: string | null): this;
  fillGradient(gradient: LinearGradient | null): this;
  stroke(color: string | null, width?: number): this;
  strokeGradient(gradient: LinearGradient | null, width?: number): this;
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
export class MarkupText extends Mobject<MarkupTextNode> { constructor(spans: TextSpan[], options?: MobjectOptions & { fontSize?: number; fontFamily?: string; align?: TextAlign }); }
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
  constructor(...items: Array<Mobject | Mobject[] | MobjectOptions>);
  readonly members: Mobject[];
  add(...items: Array<Mobject | Mobject[]>): this;
  fill(color: string | null): this;
  fillGradient(gradient: LinearGradient | null): this;
  stroke(color: string | null, width?: number): this;
  strokeGradient(gradient: LinearGradient | null, width?: number): this;
  opacity(value: number): this;
}
export class VGroup extends Group {}

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

export interface SceneOptions extends Partial<Omit<SceneData, "nodes" | "tracks" | "signals" | "bindings" | "controls" | "audio" | "captions">> {
  nodes?: SceneNode[];
  tracks?: Track[];
  signals?: SceneData["signals"];
  bindings?: SceneData["bindings"];
  controls?: SceneData["controls"];
  audio?: SceneData["audio"];
  captions?: SceneData["captions"];
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
  timeoutMs?: number;
  fonts?: BrowserFontFace[];
}

export function createManimPlayer(options: CreateManimPlayerOptions): Promise<ManimPlayer>;
