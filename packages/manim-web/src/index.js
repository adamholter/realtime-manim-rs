import initRuntime, * as wasmRuntime from "../runtime/realtime_manim_web_preview.js";

const EMPTY_SCENE = {
  version: 2,
  title: "Empty scene",
  width: 16,
  height: 9,
  duration: 0.25,
  fps: 60,
  background: "#000000",
  nodes: [{ id: "empty-root", type: "group" }],
};

let runtimeInitializationPromise = null;
let runtimeSource;
let nextObjectId = 1;

export const ORIGIN = Object.freeze([0, 0]);
export const UP = Object.freeze([0, 1]);
export const DOWN = Object.freeze([0, -1]);
export const LEFT = Object.freeze([-1, 0]);
export const RIGHT = Object.freeze([1, 0]);

const EASINGS = new Set(["linear", "smooth", "easeIn", "easeOut", "easeInOut", "thereAndBack", "bounce"]);
const TEXT_ALIGNS = new Set(["left", "center", "right"]);
const FONT_WEIGHTS = new Set(["normal", "bold"]);
const FONT_SLANTS = new Set(["normal", "italic"]);
const GRADIENT_SPREADS = new Set(["pad", "repeat", "reflect"]);
const GRADIENT_SPACES = new Set(["local", "world"]);
const IMAGE_RESAMPLING = new Set(["nearest", "box", "bilinear", "hamming", "bicubic", "lanczos"]);
const BASE_NODE_OPTION_KEYS = new Set([
  "id", "style", "transform", "parent", "zIndex", "appearAt", "disappearAt",
]);
const GROUP_STYLE_PROPERTIES = new Set([
  "opacity", "strokeWidth", "drawStart", "drawProgress", "fill", "fillGradient", "stroke", "strokeGradient",
]);
const ANIMATION_EVENTS = Symbol("realtime-manim.animation-events");
const TRANSFORM_DEFAULTS = Object.freeze({
  x: 0,
  y: 0,
  z: 0,
  rotation: 0,
  rotationX: 0,
  rotationY: 0,
  scaleX: 1,
  scaleY: 1,
  scaleZ: 1,
});
const STYLE_DEFAULTS = Object.freeze({
  fill: null,
  fillGradient: null,
  stroke: "#f8fafc",
  strokeGradient: null,
  strokeWidth: 0.04,
  opacity: 1,
  drawStart: 0,
  drawProgress: 1,
});
const DIRECT_TRACK_PROPERTIES = new Map([
  ["radius", "radius"],
  ["points", "points"],
  ["vertices", "vertices"],
  ["normals", "normals"],
  ["colors", "colors"],
  ["surfaceColors", "surfaceColors"],
  ["strokeRadii", "strokeRadii"],
  ["lightPosition", "lightPosition"],
  ["commands", "commands"],
  ["pathData", "pathData"],
  ["transform2d", "transform2d"],
  ["affine2d", "affine2d"],
  ["drawRange", "drawRange"],
  ["shaderVertexData", "shaderVertexData"],
  ["shaderUniformValues", "shaderUniformValues"],
  ["anchor", "billboardAnchor"],
]);

function assertFinite(value, label) {
  if (!Number.isFinite(value)) throw new TypeError(`${label} must be finite.`);
  return value;
}

function assertNonNegative(value, label) {
  assertFinite(value, label);
  if (value < 0) throw new RangeError(`${label} must not be negative.`);
  return value;
}

function assertPositive(value, label) {
  assertFinite(value, label);
  if (value <= 0) throw new RangeError(`${label} must be greater than zero.`);
  return value;
}

function assertInteger(value, label, minimum = Number.MIN_SAFE_INTEGER) {
  if (!Number.isInteger(value) || value < minimum) throw new RangeError(`${label} must be an integer of at least ${minimum}.`);
  return value;
}

function assertIntegerRange(value, label, minimum, maximum) {
  assertInteger(value, label, minimum);
  if (value > maximum) throw new RangeError(`${label} must not exceed ${maximum}.`);
  return value;
}

function assertBoolean(value, label) {
  if (typeof value !== "boolean") throw new TypeError(`${label} must be boolean.`);
  return value;
}

function assertObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new TypeError(`${label} must be an object.`);
  return value;
}

function assertString(value, label, { allowEmpty = false, max = Infinity } = {}) {
  if (typeof value !== "string" || (!allowEmpty && value.length === 0) || value.length > max) {
    throw new TypeError(`${label} must be ${allowEmpty ? "a" : "a nonempty"} string${Number.isFinite(max) ? ` of at most ${max} characters` : ""}.`);
  }
  return value;
}

function assertId(value, label = "id") {
  assertString(value, label, { max: 80 });
  if (!/^[A-Za-z0-9_.-]+$/.test(value)) throw new TypeError(`${label} may contain only letters, numbers, _, -, and .`);
  return value;
}

function assertFontFamily(value, label = "font family") {
  const family = assertString(value, label, { max: 120 }).trim();
  if (family.length === 0) throw new TypeError(`${label} must contain visible characters.`);
  return family;
}

function fontBytes(value, label = "font data") {
  let bytes;
  if (value instanceof ArrayBuffer) bytes = new Uint8Array(value);
  else if (value instanceof Uint8Array) bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  else throw new TypeError(`${label} must be an ArrayBuffer or Uint8Array.`);
  if (bytes.byteLength === 0 || bytes.byteLength > 32 * 1024 * 1024) {
    throw new RangeError(`${label} must contain 1 byte–32 MiB.`);
  }
  return bytes;
}

function normalizeFontFace(face, label = "font") {
  assertObject(face, label);
  const allowed = new Set(["family", "data", "weight", "slant"]);
  for (const key of Object.keys(face)) if (!allowed.has(key)) throw new TypeError(`Unknown ${label} property: ${key}.`);
  return {
    family: assertFontFamily(face.family, `${label}.family`),
    data: fontBytes(face.data, `${label}.data`),
    weight: assertEnum(face.weight ?? "normal", FONT_WEIGHTS, `${label}.weight`),
    slant: assertEnum(face.slant ?? "normal", FONT_SLANTS, `${label}.slant`),
  };
}

function assertColor(value, label = "color") {
  if (typeof value !== "string" || !/^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/i.test(value)) {
    throw new TypeError(`${label} must be #rrggbb or #rrggbbaa.`);
  }
  return value;
}

function assertGradient(value, label = "gradient") {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new TypeError(`${label} must be an object.`);
  const from = assertPoint(value.from, `${label}.from`);
  const to = assertPoint(value.to, `${label}.to`);
  if ((from[0] - to[0]) ** 2 + (from[1] - to[1]) ** 2 <= Number.EPSILON) throw new RangeError(`${label} endpoints must be distinct.`);
  if (!Array.isArray(value.stops) || value.stops.length < 2 || value.stops.length > 64) throw new RangeError(`${label} must contain 2–64 stops.`);
  let previous = -1;
  const stops = value.stops.map((stop, index) => {
    if (!stop || typeof stop !== "object" || Array.isArray(stop)) throw new TypeError(`${label}.stops[${index}] must be an object.`);
    const offset = assertUnitInterval(stop.offset, `${label}.stops[${index}].offset`);
    if (offset < previous) throw new RangeError(`${label} stop offsets must be nondecreasing.`);
    previous = offset;
    return { offset, color: assertColor(stop.color, `${label}.stops[${index}].color`) };
  });
  return {
    from,
    to,
    stops,
    spread: assertEnum(value.spread ?? "pad", GRADIENT_SPREADS, `${label}.spread`),
    space: assertEnum(value.space ?? "local", GRADIENT_SPACES, `${label}.space`),
  };
}

function assertUnitInterval(value, label) {
  assertFinite(value, label);
  if (value < 0 || value > 1) throw new RangeError(`${label} must be between 0 and 1.`);
  return value;
}

function assertEnum(value, values, label) {
  if (!values.has(value)) throw new RangeError(`${label} is not supported.`);
  return value;
}

function assertPoint(value, label = "point") {
  if (!Array.isArray(value) || value.length !== 2) throw new TypeError(`${label} must be a [x, y] pair.`);
  return [assertFinite(value[0], `${label} x`), assertFinite(value[1], `${label} y`)];
}

function assertPoint3D(value, label = "3D point") {
  if (!Array.isArray(value) || value.length !== 3) throw new TypeError(`${label} must be a [x, y, z] triple.`);
  return [
    assertFinite(value[0], `${label} x`),
    assertFinite(value[1], `${label} y`),
    assertFinite(value[2], `${label} z`),
  ];
}

function assertPoints(points, minimum, label = "points") {
  if (!Array.isArray(points) || points.length < minimum || points.length > 100_000) {
    throw new RangeError(`${label} must contain ${minimum}–100,000 points.`);
  }
  return points.map((point, index) => assertPoint(point, `${label}[${index}]`));
}

function assertPoints3D(points, minimum, maximum, label = "3D points") {
  if (!Array.isArray(points) || points.length < minimum || points.length > maximum) {
    throw new RangeError(`${label} must contain ${minimum}–${maximum.toLocaleString("en-US")} points.`);
  }
  return points.map((point, index) => assertPoint3D(point, `${label}[${index}]`));
}

function partitionNodeOptions(options, kindKeys, label) {
  assertObject(options, `${label} options`);
  const nodeOptions = {};
  const kindOptions = {};
  for (const [key, value] of Object.entries(options)) {
    if (BASE_NODE_OPTION_KEYS.has(key)) nodeOptions[key] = value;
    else if (kindKeys.has(key)) kindOptions[key] = value;
    else throw new TypeError(`Unknown ${label} option: ${key}.`);
  }
  if (nodeOptions.id !== undefined) nodeOptions.id = assertId(nodeOptions.id);
  if (nodeOptions.parent !== undefined) nodeOptions.parent = assertId(nodeOptions.parent, "parent id");
  if (nodeOptions.zIndex !== undefined) {
    nodeOptions.zIndex = assertIntegerRange(nodeOptions.zIndex, "zIndex", -2_147_483_648, 2_147_483_647);
  }
  if (nodeOptions.appearAt !== undefined) nodeOptions.appearAt = assertNonNegative(nodeOptions.appearAt, "appearAt");
  if (nodeOptions.disappearAt !== undefined) nodeOptions.disappearAt = assertPositive(nodeOptions.disappearAt, "disappearAt");
  if (nodeOptions.disappearAt !== undefined && nodeOptions.disappearAt <= (nodeOptions.appearAt ?? 0)) {
    throw new RangeError("disappearAt must be after appearAt.");
  }
  return { nodeOptions, kindOptions };
}

const BASE64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const CANONICAL_BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

function rgbaBytes(value, label) {
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value) && (value instanceof Uint8Array || value instanceof Uint8ClampedArray)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  throw new TypeError(`${label} must be an ArrayBuffer, Uint8Array, or Uint8ClampedArray.`);
}

function bytesToBase64(bytes) {
  const chunks = [];
  let chunk = "";
  let index = 0;
  for (; index + 2 < bytes.length; index += 3) {
    const value = (bytes[index] << 16) | (bytes[index + 1] << 8) | bytes[index + 2];
    chunk += BASE64_ALPHABET[(value >>> 18) & 63]
      + BASE64_ALPHABET[(value >>> 12) & 63]
      + BASE64_ALPHABET[(value >>> 6) & 63]
      + BASE64_ALPHABET[value & 63];
    if (chunk.length >= 16_384) {
      chunks.push(chunk);
      chunk = "";
    }
  }
  if (index < bytes.length) {
    const first = bytes[index];
    const second = index + 1 < bytes.length ? bytes[index + 1] : 0;
    const value = (first << 16) | (second << 8);
    chunk += BASE64_ALPHABET[(value >>> 18) & 63] + BASE64_ALPHABET[(value >>> 12) & 63];
    chunk += index + 1 < bytes.length ? `${BASE64_ALPHABET[(value >>> 6) & 63]}=` : "==";
  }
  chunks.push(chunk);
  return chunks.join("");
}

function normalizeRgba(pixels, pixelWidth, pixelHeight, label) {
  const width = assertIntegerRange(pixelWidth, `${label} width`, 1, 8_192);
  const height = assertIntegerRange(pixelHeight, `${label} height`, 1, 8_192);
  if (width * height > 16_777_216) throw new RangeError(`${label} exceeds 16,777,216 pixels.`);
  const expectedBytes = width * height * 4;
  let encoded;
  if (typeof pixels === "string") {
    encoded = pixels;
    const expectedCharacters = Math.ceil(expectedBytes / 3) * 4;
    if (encoded.length !== expectedCharacters || !CANONICAL_BASE64.test(encoded)) {
      throw new TypeError(`${label} must be canonical base64 RGBA data matching its dimensions.`);
    }
  } else {
    const bytes = rgbaBytes(pixels, label);
    if (bytes.byteLength !== expectedBytes) {
      throw new RangeError(`${label} contains ${bytes.byteLength} bytes; expected ${expectedBytes}.`);
    }
    encoded = bytesToBase64(bytes);
  }
  return { pixels: encoded, width, height };
}

function normalizeOptionalTexture(texture, flatPixels, flatWidth, flatHeight, label) {
  if (texture !== undefined) {
    assertObject(texture, label);
    const allowed = new Set(["pixels", "data", "width", "height"]);
    for (const key of Object.keys(texture)) if (!allowed.has(key)) throw new TypeError(`Unknown ${label} property: ${key}.`);
    if (flatPixels !== undefined || flatWidth !== undefined || flatHeight !== undefined) {
      throw new TypeError(`${label} cannot be combined with flat pixel or dimension options.`);
    }
    return normalizeRgba(texture.pixels ?? texture.data, texture.width, texture.height, label);
  }
  if (flatPixels === undefined || flatPixels === "") {
    if ((flatWidth ?? 0) !== 0 || (flatHeight ?? 0) !== 0) throw new RangeError(`${label} dimensions require pixels.`);
    return { pixels: "", width: 0, height: 0 };
  }
  return normalizeRgba(flatPixels, flatWidth, flatHeight, label);
}

function assertTrackValue(value, label) {
  if (typeof value === "number") return assertFinite(value, label);
  if (typeof value === "string") return assertString(value, label);
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) assertTrackValue(value[index], `${label}[${index}]`);
    return value;
  }
  if (value && typeof value === "object") {
    for (const [key, member] of Object.entries(value)) assertTrackValue(member, `${label}.${key}`);
    return value;
  }
  throw new TypeError(`${label} is not a supported retained-scene value.`);
}

function assertPathCommand(command, dimension = 2, index = 0) {
  if (!command || typeof command !== "object" || Array.isArray(command)) {
    throw new TypeError(`commands[${index}] must be a path command.`);
  }
  const op = command.op;
  const fields = dimension === 2
    ? { moveTo: ["x", "y"], lineTo: ["x", "y"], quadTo: ["cx", "cy", "x", "y"], cubicTo: ["c1x", "c1y", "c2x", "c2y", "x", "y"], close: [] }
    : { moveTo: ["x", "y", "z"], lineTo: ["x", "y", "z"], quadTo: ["cx", "cy", "cz", "x", "y", "z"], cubicTo: ["c1x", "c1y", "c1z", "c2x", "c2y", "c2z", "x", "y", "z"], close: [] };
  if (!(op in fields)) throw new RangeError(`commands[${index}] has an unsupported operation.`);
  const allowed = new Set(["op", ...fields[op]]);
  for (const key of Object.keys(command)) {
    if (!allowed.has(key)) throw new TypeError(`commands[${index}].${key} is not valid for ${op}.`);
  }
  const result = { op };
  for (const key of fields[op]) result[key] = assertFinite(command[key], `commands[${index}].${key}`);
  return result;
}

function assertPathCommands(commands, dimension = 2) {
  if (!Array.isArray(commands) || commands.length === 0 || commands.length > 50_000) {
    throw new RangeError(`${dimension === 3 ? "3D p" : "P"}ath commands must contain 1–50,000 entries.`);
  }
  return commands.map((command, index) => assertPathCommand(command, dimension, index));
}

export const pathCommand = Object.freeze({
  moveTo: (point) => { const [x, y] = assertPoint(point); return { op: "moveTo", x, y }; },
  lineTo: (point) => { const [x, y] = assertPoint(point); return { op: "lineTo", x, y }; },
  quadTo: (control, point) => { const [cx, cy] = assertPoint(control, "control"); const [x, y] = assertPoint(point); return { op: "quadTo", cx, cy, x, y }; },
  cubicTo: (control1, control2, point) => {
    const [c1x, c1y] = assertPoint(control1, "control1");
    const [c2x, c2y] = assertPoint(control2, "control2");
    const [x, y] = assertPoint(point);
    return { op: "cubicTo", c1x, c1y, c2x, c2y, x, y };
  },
  close: () => ({ op: "close" }),
});

export const pathCommand3D = Object.freeze({
  moveTo: (point) => { const [x, y, z] = assertPoint3D(point); return { op: "moveTo", x, y, z }; },
  lineTo: (point) => { const [x, y, z] = assertPoint3D(point); return { op: "lineTo", x, y, z }; },
  quadTo: (control, point) => {
    const [cx, cy, cz] = assertPoint3D(control, "control");
    const [x, y, z] = assertPoint3D(point);
    return { op: "quadTo", cx, cy, cz, x, y, z };
  },
  cubicTo: (control1, control2, point) => {
    const [c1x, c1y, c1z] = assertPoint3D(control1, "control1");
    const [c2x, c2y, c2z] = assertPoint3D(control2, "control2");
    const [x, y, z] = assertPoint3D(point);
    return { op: "cubicTo", c1x, c1y, c1z, c2x, c2y, c2z, x, y, z };
  },
  close: () => ({ op: "close" }),
});

function idOf(target) {
  if (typeof target === "string" && target) return assertId(target, "target id");
  if (target instanceof Mobject) return target.id;
  throw new TypeError("Animation target must be a Mobject or node id.");
}

function sceneObject(scene) {
  if (scene instanceof Scene) return scene.toJSON();
  if (typeof scene === "string") return JSON.parse(scene);
  if (scene && typeof scene === "object") return structuredClone(scene);
  throw new TypeError("Scene must be a Scene, object, or JSON string.");
}

export class Mobject {
  constructor(type, options = {}) {
    const { id = `${type}-${nextObjectId++}`, style = {}, transform = {}, ...kind } = options;
    assertId(id);
    if (!style || typeof style !== "object" || Array.isArray(style)) throw new TypeError("style must be an object.");
    if (!transform || typeof transform !== "object" || Array.isArray(transform)) throw new TypeError("transform must be an object.");
    const normalizedTransform = {};
    const transformKeys = new Set(["x", "y", "z", "rotation", "rotationX", "rotationY", "scaleX", "scaleY", "scaleZ"]);
    for (const [key, value] of Object.entries(transform)) {
      if (!transformKeys.has(key)) throw new TypeError(`Unknown transform property: ${key}.`);
      const normalized = assertFinite(value, `transform.${key}`);
      if (key.startsWith("scale") && Math.abs(normalized) > 1_000) throw new RangeError(`${key} is outside the supported range.`);
      normalizedTransform[key] = normalized;
    }
    const styleKeys = new Set(["fill", "fillGradient", "stroke", "strokeGradient", "strokeWidth", "opacity", "drawStart", "drawProgress"]);
    for (const key of Object.keys(style)) if (!styleKeys.has(key)) throw new TypeError(`Unknown style property: ${key}.`);
    const normalizedStyle = { ...style };
    for (const key of ["fill", "stroke"]) {
      if (key in normalizedStyle && normalizedStyle[key] !== null) normalizedStyle[key] = assertColor(normalizedStyle[key], `style.${key}`);
    }
    for (const key of ["fillGradient", "strokeGradient"]) {
      if (key in normalizedStyle && normalizedStyle[key] !== null) normalizedStyle[key] = assertGradient(normalizedStyle[key], `style.${key}`);
    }
    if ("strokeWidth" in normalizedStyle) {
      const width = assertNonNegative(normalizedStyle.strokeWidth, "style.strokeWidth");
      if (width > 10) throw new RangeError("style.strokeWidth must not exceed 10.");
    }
    for (const key of ["opacity", "drawStart", "drawProgress"]) {
      if (key in normalizedStyle) assertUnitInterval(normalizedStyle[key], `style.${key}`);
    }
    this.node = {
      id,
      type,
      transform: normalizedTransform,
      style: normalizedStyle,
      ...kind,
    };
  }

  get id() { return this.node.id; }
  moveTo(x, y) {
    const point = Array.isArray(x) ? assertPoint(x) : [assertFinite(x, "x"), assertFinite(y, "y")];
    this.node.transform.x = point[0];
    this.node.transform.y = point[1];
    return this;
  }
  shift(dx, dy) {
    const offset = Array.isArray(dx) ? assertPoint(dx, "offset") : [assertFinite(dx, "dx"), assertFinite(dy, "dy")];
    return this.moveTo((this.node.transform.x ?? 0) + offset[0], (this.node.transform.y ?? 0) + offset[1]);
  }
  scale(value) { assertFinite(value, "scale"); this.node.transform.scaleX = value; this.node.transform.scaleY = value; return this; }
  rotate(radians) { this.node.transform.rotation = assertFinite(radians, "rotation"); return this; }
  rotateX(radians) { this.node.transform.rotationX = assertFinite(radians, "rotationX"); return this; }
  rotateY(radians) { this.node.transform.rotationY = assertFinite(radians, "rotationY"); return this; }
  fill(color) { if (color !== null) assertColor(color, "fill"); this.node.style.fill = color; return this; }
  fillGradient(gradient) { this.node.style.fillGradient = gradient === null ? null : assertGradient(gradient, "fill gradient"); return this; }
  stroke(color, width = this.node.style.strokeWidth ?? 0.04) {
    if (color !== null) assertColor(color, "stroke");
    const normalizedWidth = assertNonNegative(width, "stroke width");
    if (normalizedWidth > 10) throw new RangeError("stroke width must not exceed 10.");
    this.node.style.stroke = color;
    this.node.style.strokeWidth = normalizedWidth;
    return this;
  }
  strokeGradient(gradient, width = this.node.style.strokeWidth ?? 0.04) {
    const normalizedWidth = assertNonNegative(width, "stroke width");
    if (normalizedWidth > 10) throw new RangeError("stroke width must not exceed 10.");
    this.node.style.strokeGradient = gradient === null ? null : assertGradient(gradient, "stroke gradient");
    this.node.style.strokeWidth = normalizedWidth;
    return this;
  }
  opacity(value) { this.node.style.opacity = assertUnitInterval(value, "opacity"); return this; }
  zIndex(value) { this.node.zIndex = assertInteger(value, "zIndex"); return this; }
  setParent(parent) { this.node.parent = parent === null ? undefined : assertId(idOf(parent), "parent id"); return this; }
  appearAt(value) { this.node.appearAt = assertNonNegative(value, "appearAt"); return this; }
  disappearAt(value) { this.node.disappearAt = assertPositive(value, "disappearAt"); return this; }
  toJSON() { return structuredClone(this.node); }
}

export class Circle extends Mobject {
  constructor(options = {}) {
    const radius = assertPositive(options.radius ?? 1, "circle radius");
    super("circle", { ...options, radius });
  }
}

export class Dot extends Circle {
  constructor(options = {}) { super({ radius: 0.08, ...options }); }
}

export class Ellipse extends Circle {
  constructor(options = {}) {
    const { width = 2, height = 1, transform = {}, ...rest } = options;
    const normalizedWidth = assertPositive(width, "ellipse width");
    const normalizedHeight = assertPositive(height, "ellipse height");
    super({ ...rest, radius: 1, transform: { ...transform, scaleX: normalizedWidth / 2, scaleY: normalizedHeight / 2 } });
  }
  scale(value) {
    assertFinite(value, "scale");
    this.node.transform.scaleX *= value;
    this.node.transform.scaleY *= value;
    return this;
  }
}

export class Rectangle extends Mobject {
  constructor(options = {}) {
    const width = assertPositive(options.width ?? 2, "rectangle width");
    const height = assertPositive(options.height ?? 1, "rectangle height");
    const cornerRadius = assertNonNegative(options.cornerRadius ?? 0, "rectangle corner radius");
    super("rect", { ...options, width, height, cornerRadius });
  }
}

export class Square extends Rectangle {
  constructor(options = {}) {
    const size = assertPositive(options.size ?? 2, "square size");
    super({ ...options, width: size, height: size });
    delete this.node.size;
  }
}

export class RoundedRectangle extends Rectangle {
  constructor(options = {}) { super({ cornerRadius: 0.2, ...options }); }
}

export class Line extends Mobject {
  constructor(from = [-1, 0], to = [1, 0], options = {}) {
    super("line", { ...options, from: assertPoint(from, "line start"), to: assertPoint(to, "line end") });
  }
}

export class Arrow extends Mobject {
  constructor(from = [-1, 0], to = [1, 0], options = {}) {
    const tipSize = assertPositive(options.tipSize ?? 0.24, "arrow tip size");
    super("arrow", { ...options, from: assertPoint(from, "arrow start"), to: assertPoint(to, "arrow end"), tipSize });
  }
}

export class Polyline extends Mobject {
  constructor(points, options = {}) { super("polyline", { ...options, points: assertPoints(points, 2), closed: options.closed ?? false }); }
}

export class Polygon extends Polyline {
  constructor(points, options = {}) { super(assertPoints(points, 3, "polygon points"), { ...options, closed: true }); }
}

export class RegularPolygon extends Polygon {
  constructor(sides = 6, options = {}) {
    assertInteger(sides, "polygon sides", 3);
    const { radius = 1, startAngle = Math.PI / 2, ...rest } = options;
    assertPositive(radius, "polygon radius");
    assertFinite(startAngle, "polygon start angle");
    const points = Array.from({ length: sides }, (_, index) => {
      const theta = startAngle + (index * Math.PI * 2) / sides;
      return [Math.cos(theta) * radius, Math.sin(theta) * radius];
    });
    super(points, rest);
  }
}

export class Triangle extends RegularPolygon {
  constructor(options = {}) { super(3, options); }
}

export class Path extends Mobject {
  constructor(commands, options = {}) { super("path", { ...options, commands: assertPathCommands(commands) }); }
}

export class Path3D extends Mobject {
  constructor(commands, options = {}) { super("path3d", { ...options, commands: assertPathCommands(commands, 3) }); }
}

export class TracePath extends Mobject {
  constructor(segments, frames, options = {}) {
    if (!Array.isArray(segments) || segments.length === 0 || segments.length > 100_000) {
      throw new RangeError("TracePath requires 1–100,000 cubic segments.");
    }
    const normalizedSegments = segments.map((segment, index) => {
      assertObject(segment, `segments[${index}]`);
      const allowed = new Set(["start", "control1", "control2", "end"]);
      for (const key of Object.keys(segment)) {
        if (!allowed.has(key)) throw new TypeError(`Unknown segments[${index}] property: ${key}.`);
      }
      return {
        start: assertPoint(segment.start, `segments[${index}].start`),
        control1: assertPoint(segment.control1, `segments[${index}].control1`),
        control2: assertPoint(segment.control2, `segments[${index}].control2`),
        end: assertPoint(segment.end, `segments[${index}].end`),
      };
    });
    if (!Array.isArray(frames) || frames.length === 0 || frames.length > 10_000) {
      throw new RangeError("TracePath requires 1–10,000 frames.");
    }
    let previous = -1;
    const normalizedFrames = frames.map((frame, index) => {
      assertObject(frame, `frames[${index}]`);
      const allowed = new Set(["at", "start", "count", "closed"]);
      for (const key of Object.keys(frame)) {
        if (!allowed.has(key)) throw new TypeError(`Unknown frames[${index}] property: ${key}.`);
      }
      const at = assertNonNegative(frame.at, `frames[${index}].at`);
      if (at < previous) throw new RangeError("TracePath frames must be ordered by time.");
      previous = at;
      const start = assertIntegerRange(frame.start, `frames[${index}].start`, 0, normalizedSegments.length - 1);
      const count = assertIntegerRange(frame.count, `frames[${index}].count`, 1, normalizedSegments.length);
      if (start + count > normalizedSegments.length) throw new RangeError(`frames[${index}] exceeds the segment list.`);
      return {
        at,
        start,
        count,
        closed: assertBoolean(frame.closed ?? false, `frames[${index}].closed`),
      };
    });
    const { nodeOptions } = partitionNodeOptions(options, new Set(), "TracePath");
    super("tracePath", { ...nodeOptions, segments: normalizedSegments, frames: normalizedFrames });
  }
}

export class QuadraticBezier extends Path {
  constructor(start, control, end, options = {}) {
    super([pathCommand.moveTo(start), pathCommand.quadTo(control, end)], options);
  }
}

export class CubicBezier extends Path {
  constructor(start, control1, control2, end, options = {}) {
    super([pathCommand.moveTo(start), pathCommand.cubicTo(control1, control2, end)], options);
  }
}

export class Arc extends Path {
  constructor(options = {}) {
    const { radius = 1, startAngle = 0, angle = Math.PI / 2, arcCenter = ORIGIN, ...rest } = options;
    assertPositive(radius, "arc radius");
    assertFinite(startAngle, "arc start angle");
    assertFinite(angle, "arc angle");
    const center = assertPoint(arcCenter, "arc center");
    const count = Math.max(1, Math.ceil(Math.abs(angle) / (Math.PI / 2)));
    const step = angle / count;
    const pointAt = (theta) => [center[0] + Math.cos(theta) * radius, center[1] + Math.sin(theta) * radius];
    const commands = [pathCommand.moveTo(pointAt(startAngle))];
    for (let index = 0; index < count; index += 1) {
      const fromAngle = startAngle + step * index;
      const toAngle = fromAngle + step;
      const start = pointAt(fromAngle);
      const end = pointAt(toAngle);
      const handle = (4 / 3) * Math.tan(step / 4) * radius;
      commands.push(pathCommand.cubicTo(
        [start[0] - Math.sin(fromAngle) * handle, start[1] + Math.cos(fromAngle) * handle],
        [end[0] + Math.sin(toAngle) * handle, end[1] - Math.cos(toAngle) * handle],
        end,
      ));
    }
    super(commands, rest);
  }
}

export class Text extends Mobject {
  constructor(text, options = {}) {
    const { nodeOptions, kindOptions } = partitionNodeOptions(
      options,
      new Set(["fontSize", "fontFamily", "align", "weight", "slant"]),
      "Text",
    );
    const fontSize = assertPositive(kindOptions.fontSize ?? 0.6, "font size");
    const fontFamily = assertFontFamily(kindOptions.fontFamily ?? "Noto Sans");
    const align = assertEnum(kindOptions.align ?? "center", TEXT_ALIGNS, "text alignment");
    const weight = assertEnum(kindOptions.weight ?? "normal", FONT_WEIGHTS, "font weight");
    const slant = assertEnum(kindOptions.slant ?? "normal", FONT_SLANTS, "font slant");
    super("text", { ...nodeOptions, text: assertString(text, "text", { max: 4_000 }), fontSize, fontFamily, align, weight, slant });
  }
}

export class MarkupText extends Mobject {
  constructor(spans, options = {}) {
    const { nodeOptions, kindOptions } = partitionNodeOptions(
      options,
      new Set(["fontSize", "fontFamily", "align"]),
      "MarkupText",
    );
    if (!Array.isArray(spans) || spans.length === 0 || spans.length > 1_000) throw new RangeError("MarkupText requires 1–1,000 spans.");
    const normalizedSpans = spans.map((span, index) => {
      if (!span || typeof span !== "object" || Array.isArray(span)) throw new TypeError(`spans[${index}] must be an object.`);
      const text = assertString(span.text, `spans[${index}].text`, { max: 4_000 });
      if (text.includes("\n")) throw new TypeError(`spans[${index}].text must be single-line.`);
      const normalized = {
        text,
        weight: assertEnum(span.weight ?? "normal", FONT_WEIGHTS, `spans[${index}].weight`),
        slant: assertEnum(span.slant ?? "normal", FONT_SLANTS, `spans[${index}].slant`),
      };
      if (span.color !== undefined) normalized.color = assertColor(span.color, `spans[${index}].color`);
      return normalized;
    });
    if (normalizedSpans.reduce((total, span) => total + span.text.length, 0) > 4_000) throw new RangeError("MarkupText is limited to 4,000 characters.");
    const fontSize = assertPositive(kindOptions.fontSize ?? 0.6, "font size");
    const fontFamily = assertFontFamily(kindOptions.fontFamily ?? "Noto Sans");
    const align = assertEnum(kindOptions.align ?? "center", TEXT_ALIGNS, "text alignment");
    super("markupText", { ...nodeOptions, spans: normalizedSpans, fontSize, fontFamily, align });
  }
}

export class SVG extends Mobject {
  constructor(svg, options = {}) {
    const height = assertPositive(options.height ?? 2, "SVG height");
    super("svg", { ...options, svg: assertString(svg, "SVG", { max: 4_000_000 }), height, preserveStyles: options.preserveStyles ?? true });
  }
}

export class MathTex extends SVG {
  constructor(svg, options = {}) {
    assertObject(options, "MathTex options");
    const { tex, fontSize, height, ...rest } = options;
    if (tex !== undefined) assertString(tex, "LaTeX source", { max: 4_000 });
    if (fontSize !== undefined && height !== undefined && fontSize !== height) {
      throw new RangeError("MathTex fontSize and height cannot disagree.");
    }
    const { nodeOptions } = partitionNodeOptions(rest, new Set(), "MathTex");
    super(svg, { ...nodeOptions, height: assertPositive(fontSize ?? height ?? 1.2, "MathTex font size"), preserveStyles: false });
    if (tex !== undefined) this.tex = tex;
  }

  static async typeset(tex, options = {}) {
    const source = assertString(tex, "LaTeX source", { max: 4_000 });
    const {
      endpoint = "/api/typeset",
      fetch: fetchImplementation = globalThis.fetch,
      signal,
      ...nodeOptions
    } = options;
    if (typeof fetchImplementation !== "function") throw new TypeError("MathTex.typeset needs a Fetch-compatible function.");
    const response = await fetchImplementation(String(endpoint), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ tex: source }),
      ...(signal === undefined ? {} : { signal }),
    });
    if (!response || typeof response.ok !== "boolean" || typeof response.json !== "function") {
      throw new TypeError("MathTex.typeset received an invalid Fetch response.");
    }
    let payload;
    try {
      payload = await response.json();
    } catch (error) {
      throw new Error("LaTeX endpoint returned invalid JSON.", { cause: error });
    }
    if (!response.ok) throw new Error(payload?.error || `LaTeX typesetting failed with ${response.status}.`);
    const svg = assertString(payload?.svg, "LaTeX endpoint SVG", { max: 4_000_000 });
    if (!/<svg(?:\s|>)/i.test(svg)) throw new TypeError("LaTeX endpoint did not return an SVG document.");
    return new MathTex(svg, { ...nodeOptions, tex: source });
  }

  get source() { return this.tex; }
}

export class Image extends Mobject {
  constructor(pixels, pixelWidth, pixelHeight, options = {}) {
    const { nodeOptions, kindOptions } = partitionNodeOptions(
      options,
      new Set(["corners", "resampling"]),
      "Image",
    );
    const rgba = normalizeRgba(pixels, pixelWidth, pixelHeight, "image pixels");
    const corners = (kindOptions.corners ?? [[-1, 1], [1, 1], [-1, -1], [1, -1]])
      .map((point, index) => assertPoint(point, `image corners[${index}]`));
    if (corners.length !== 4) throw new RangeError("Image corners must contain exactly four points.");
    const resampling = assertEnum(kindOptions.resampling ?? "bicubic", IMAGE_RESAMPLING, "image resampling");
    super("image", {
      ...nodeOptions,
      pixels: rgba.pixels,
      pixelWidth: rgba.width,
      pixelHeight: rgba.height,
      corners,
      resampling,
    });
  }

  static fromImageData(imageData, options = {}) {
    if (!imageData || typeof imageData !== "object") throw new TypeError("Image.fromImageData expects ImageData-like data.");
    return new Image(imageData.data, imageData.width, imageData.height, options);
  }

  static async fromSource(source, options = {}) {
    const { crossOrigin, ...nodeOptions } = options;
    const image = typeof source === "string" || source instanceof URL
      ? await Image.loadSource(source, crossOrigin)
      : source;
    if (!image || typeof image !== "object") throw new TypeError("Image.fromSource expects a URL or CanvasImageSource.");
    const width = image.naturalWidth ?? image.videoWidth ?? image.width;
    const height = image.naturalHeight ?? image.videoHeight ?? image.height;
    assertIntegerRange(width, "image source width", 1, 8_192);
    assertIntegerRange(height, "image source height", 1, 8_192);
    if (typeof globalThis.OffscreenCanvas === "undefined" && typeof globalThis.document === "undefined") {
      throw new Error("Image.fromSource needs OffscreenCanvas or a browser document.");
    }
    const canvas = typeof globalThis.OffscreenCanvas === "function"
      ? new globalThis.OffscreenCanvas(width, height)
      : Object.assign(globalThis.document.createElement("canvas"), { width, height });
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) throw new Error("Image.fromSource could not create a 2D canvas context.");
    context.drawImage(image, 0, 0, width, height);
    let imageData;
    try {
      imageData = context.getImageData(0, 0, width, height);
    } catch (error) {
      throw new Error("Image.fromSource could not read pixels; use a CORS-enabled source.", { cause: error });
    }
    return Image.fromImageData(imageData, nodeOptions);
  }

  static loadSource(source, crossOrigin) {
    if (typeof globalThis.Image !== "function") throw new Error("Loading image URLs requires a browser Image implementation.");
    return new Promise((resolve, reject) => {
      const image = new globalThis.Image();
      if (crossOrigin !== undefined) image.crossOrigin = crossOrigin;
      image.onload = () => resolve(image);
      image.onerror = () => reject(new Error(`Could not load image source ${String(source)}.`));
      image.src = String(source);
    });
  }
}

function normalizeTriangles(triangles, vertexCount) {
  if (!Array.isArray(triangles) || triangles.length === 0 || triangles.length > 2_000_000) {
    throw new RangeError("Mesh triangles must contain 1–2,000,000 faces.");
  }
  return triangles.map((triangle, faceIndex) => {
    if (!Array.isArray(triangle) || triangle.length !== 3) throw new TypeError(`triangles[${faceIndex}] must contain three indices.`);
    return triangle.map((index, cornerIndex) => assertIntegerRange(
      index,
      `triangles[${faceIndex}][${cornerIndex}]`,
      0,
      vertexCount - 1,
    ));
  });
}

export class Mesh extends Mobject {
  constructor(vertices, triangles, options = {}) {
    const { nodeOptions, kindOptions } = partitionNodeOptions(options, new Set([
      "colors", "normals", "uvs", "texture", "texturePixels", "textureWidth", "textureHeight",
      "darkTexture", "darkTexturePixels", "darkTextureWidth", "darkTextureHeight", "textureResampling",
      "gloss", "shadow", "lightPosition", "unlit", "doubleSided",
    ]), "Mesh");
    const normalizedVertices = assertPoints3D(vertices, 3, 1_000_000, "mesh vertices");
    const normalizedTriangles = normalizeTriangles(triangles, normalizedVertices.length);
    const colors = kindOptions.colors ?? [];
    if (!Array.isArray(colors) || (colors.length !== 0 && colors.length !== normalizedVertices.length)) {
      throw new RangeError("Mesh colors must be empty or match the vertex count.");
    }
    const normalizedColors = colors.map((color, index) => assertColor(color, `mesh colors[${index}]`));
    const normals = kindOptions.normals ?? [];
    if (!Array.isArray(normals) || (normals.length !== 0 && normals.length !== normalizedVertices.length)) {
      throw new RangeError("Mesh normals must be empty or match the vertex count.");
    }
    const normalizedNormals = normals.map((normal, index) => assertPoint3D(normal, `mesh normals[${index}]`));
    const texture = normalizeOptionalTexture(
      kindOptions.texture,
      kindOptions.texturePixels,
      kindOptions.textureWidth,
      kindOptions.textureHeight,
      "mesh texture",
    );
    const darkTexture = normalizeOptionalTexture(
      kindOptions.darkTexture,
      kindOptions.darkTexturePixels,
      kindOptions.darkTextureWidth,
      kindOptions.darkTextureHeight,
      "mesh dark texture",
    );
    const uvs = kindOptions.uvs ?? [];
    if (!Array.isArray(uvs)) throw new TypeError("Mesh UVs must be an array.");
    const normalizedUvs = uvs.map((uv, index) => assertPoint(uv, `mesh uvs[${index}]`));
    if (texture.pixels) {
      if (normalizedUvs.length !== normalizedVertices.length) throw new RangeError("Textured mesh UVs must match the vertex count.");
    } else if (normalizedUvs.length !== 0) {
      throw new RangeError("Mesh UVs require texture pixels.");
    }
    if (darkTexture.pixels && !texture.pixels) throw new RangeError("A dark mesh texture requires a light texture.");
    const gloss = assertUnitInterval(kindOptions.gloss ?? 0, "mesh gloss");
    const shadow = assertUnitInterval(kindOptions.shadow ?? 0, "mesh shadow");
    const lightPosition = assertPoint3D(kindOptions.lightPosition ?? [-10, 10, 10], "mesh light position");
    super("mesh", {
      ...nodeOptions,
      vertices: normalizedVertices,
      triangles: normalizedTriangles,
      colors: normalizedColors,
      normals: normalizedNormals,
      uvs: normalizedUvs,
      texturePixels: texture.pixels,
      textureWidth: texture.width,
      textureHeight: texture.height,
      darkTexturePixels: darkTexture.pixels,
      darkTextureWidth: darkTexture.width,
      darkTextureHeight: darkTexture.height,
      textureResampling: assertEnum(kindOptions.textureResampling ?? "bicubic", IMAGE_RESAMPLING, "mesh texture resampling"),
      gloss,
      shadow,
      lightPosition,
      unlit: assertBoolean(kindOptions.unlit ?? false, "mesh unlit"),
      doubleSided: assertBoolean(kindOptions.doubleSided ?? false, "mesh doubleSided"),
    });
  }
}

export class Surface extends Mobject {
  constructor(vertices, patches, options = {}) {
    const { nodeOptions, kindOptions } = partitionNodeOptions(
      options,
      new Set(["colors", "strokeColors", "strokeRadii", "unlit", "doubleSided"]),
      "Surface",
    );
    const normalizedVertices = assertPoints3D(vertices, 3, 1_000_000, "surface vertices");
    if (!Array.isArray(patches) || patches.length === 0 || patches.length > 2_000_000) {
      throw new RangeError("Surface patches must contain 1–2,000,000 patches.");
    }
    let cornerCount = 0;
    const normalizedPatches = patches.map((patch, patchIndex) => {
      if (!Array.isArray(patch) || patch.length < 3 || patch.length > 1_024) {
        throw new RangeError(`surface patches[${patchIndex}] must contain 3–1,024 indices.`);
      }
      cornerCount += patch.length;
      return patch.map((index, cornerIndex) => assertIntegerRange(
        index,
        `surface patches[${patchIndex}][${cornerIndex}]`,
        0,
        normalizedVertices.length - 1,
      ));
    });
    if (!Array.isArray(kindOptions.colors) || kindOptions.colors.length !== cornerCount) {
      throw new RangeError("Surface colors must match the flattened patch corners.");
    }
    if (!Array.isArray(kindOptions.strokeColors) || kindOptions.strokeColors.length !== normalizedPatches.length) {
      throw new RangeError("Surface strokeColors must match the patch count.");
    }
    if (!Array.isArray(kindOptions.strokeRadii) || kindOptions.strokeRadii.length !== normalizedPatches.length) {
      throw new RangeError("Surface strokeRadii must match the patch count.");
    }
    super("surface", {
      ...nodeOptions,
      vertices: normalizedVertices,
      patches: normalizedPatches,
      colors: kindOptions.colors.map((color, index) => assertColor(color, `surface colors[${index}]`)),
      strokeColors: kindOptions.strokeColors.map((color, index) => assertColor(color, `surface strokeColors[${index}]`)),
      strokeRadii: kindOptions.strokeRadii.map((radius, index) => {
        const value = assertNonNegative(radius, `surface strokeRadii[${index}]`);
        if (value > 100) throw new RangeError(`surface strokeRadii[${index}] must not exceed 100.`);
        return value;
      }),
      unlit: assertBoolean(kindOptions.unlit ?? false, "surface unlit"),
      doubleSided: assertBoolean(kindOptions.doubleSided ?? false, "surface doubleSided"),
    });
  }
}

export class RetainedNode extends Mobject {
  constructor(node) {
    const source = structuredClone(assertObject(node, "retained node"));
    const { type, id, style, transform, parent, zIndex, appearAt, disappearAt, ...kind } = source;
    assertString(type, "retained node type", { max: 80 });
    const { nodeOptions } = partitionNodeOptions(
      { id: assertId(id, "retained node id"), style, transform, parent, zIndex, appearAt, disappearAt },
      new Set(),
      "RetainedNode",
    );
    for (const [key, value] of Object.entries(nodeOptions)) if (value === undefined) delete nodeOptions[key];
    super(type, { ...nodeOptions, ...kind });
  }

  static from(node) { return new RetainedNode(node); }
}

export class PointCloud extends Mobject {
  constructor(points, options = {}) {
    if (!Array.isArray(points) || points.length === 0 || points.length > 100_000) throw new RangeError("PointCloud requires 1–100,000 points.");
    const normalizedPoints = points.map((point, index) => {
      if (Array.isArray(point)) {
        const [x, y] = assertPoint(point, `points[${index}]`);
        return { x, y };
      }
      if (!point || typeof point !== "object") throw new TypeError(`points[${index}] must be a [x, y] pair or point-mark object.`);
      const normalized = { x: assertFinite(point.x, `points[${index}].x`), y: assertFinite(point.y, `points[${index}].y`) };
      if (point.color !== undefined) normalized.color = assertString(point.color, `points[${index}].color`);
      if (point.radius !== undefined) normalized.radius = assertPositive(point.radius, `points[${index}].radius`);
      return normalized;
    });
    const radius = assertPositive(options.radius ?? 0.06, "point radius");
    super("pointCloud", { ...options, points: normalizedPoints, radius, screenSpaceRadius: options.screenSpaceRadius ?? false });
  }
}

export class DotCloud extends PointCloud {}

export class Billboard extends Mobject {
  constructor(anchor, base = ORIGIN, options = {}) {
    super("billboard", { ...options, anchor: assertPoint3D(anchor, "billboard anchor"), base: assertPoint(base, "billboard base") });
  }
}

export class PathReference extends Mobject {
  constructor(source, options = {}) { super("pathRef", { ...options, source: assertId(idOf(source), "path source") }); }
}

export class Group extends Mobject {
  constructor(...items) {
    let options = {};
    const candidate = items.at(-1);
    if (candidate && typeof candidate === "object" && !(candidate instanceof Mobject) && !Array.isArray(candidate)) options = items.pop();
    super("group", options);
    this._members = [];
    this.add(...items);
  }
  get members() { return [...this._members]; }
  add(...items) {
    for (const member of items.flat(Infinity)) {
      if (!(member instanceof Mobject)) throw new TypeError("Group members must be Mobjects.");
      if (member === this || (member instanceof Group && member._contains(this))) throw new RangeError("A Group cannot contain itself.");
      this._members.push(member);
    }
    return this;
  }
  _contains(target) { return this._members.some((member) => member === target || (member instanceof Group && member._contains(target))); }
  _descendants() { return this._members.flatMap((member) => member instanceof Group ? [member, ...member._descendants()] : [member]); }
  fill(color) { super.fill(color); for (const member of this._members) member.fill(color); return this; }
  fillGradient(gradient) { super.fillGradient(gradient); for (const member of this._members) member.fillGradient(gradient); return this; }
  stroke(color, width) { super.stroke(color, width); for (const member of this._members) member.stroke(color, width); return this; }
  strokeGradient(gradient, width) { super.strokeGradient(gradient, width); for (const member of this._members) member.strokeGradient(gradient, width); return this; }
  opacity(value) { super.opacity(value); for (const member of this._members) member.opacity(value); return this; }
}

export class VGroup extends Group {}

export function animate(target, property, from, to, options = {}) {
  if (typeof property !== "string" || property.length === 0) throw new TypeError("Animation property is required.");
  const start = assertNonNegative(options.start ?? 0, "animation start");
  const duration = assertPositive(options.duration ?? 1, "animation duration");
  const easing = assertEnum(options.easing ?? "smooth", EASINGS, "animation easing");
  assertTrackValue(from, "animation start value");
  assertTrackValue(to, "animation end value");
  const targetIds = target instanceof Group && GROUP_STYLE_PROPERTIES.has(property)
    ? target._descendants().filter((member) => !(member instanceof Group)).map(({ id }) => id)
    : [idOf(target)];
  const tracks = targetIds.map((targetId) => ({
    target: targetId,
    property,
    keyframes: [
      { at: start, value: structuredClone(from) },
      { at: start + duration, value: structuredClone(to), easing },
    ],
  }));
  return tracks.length === 1 ? tracks[0] : tracks;
}

export const FadeIn = (target, options = {}) => animate(target, "opacity", 0, 1, options);
export const FadeOut = (target, options = {}) => animate(target, "opacity", 1, 0, options);
export const Create = (target, options = {}) => animate(target, "drawProgress", 0, 1, options);
export const Rotate = (target, radians = Math.PI * 2, options = {}) => animate(target, "rotation", options.from ?? 0, radians, options);

export function MoveTo(target, point, options = {}) {
  const destination = assertPoint(point, "destination");
  const from = assertPoint(options.from ?? [0, 0], "animation origin");
  return [
    animate(target, "x", from[0], destination[0], options),
    animate(target, "y", from[1], destination[1], options),
  ];
}

export function MoveCamera(to, options = {}) {
  if (!to || typeof to !== "object" || Array.isArray(to)) throw new TypeError("MoveCamera destination must be an object.");
  const from = options.from ?? {};
  if (!from || typeof from !== "object" || Array.isArray(from)) throw new TypeError("MoveCamera origin must be an object.");
  const properties = [
    ["x", "cameraX", 0],
    ["y", "cameraY", 0],
    ["zoom", "cameraZoom", 1],
    ["rotation", "cameraRotation", 0],
  ];
  const tracks = properties
    .filter(([key]) => to[key] !== undefined)
    .map(([key, property, fallback]) => animate("__camera__", property, from[key] ?? fallback, to[key], options));
  if (tracks.length === 0) throw new TypeError("MoveCamera needs x, y, zoom, or rotation.");
  return tracks;
}

export function OrbitCamera(position, options = {}) {
  const destination = assertPoint3D(position, "camera orbit destination");
  const from = assertPoint3D(options.from ?? [0, 0, 8], "camera orbit origin");
  return animate("__camera__", "camera3dOrbit", from, destination, options);
}

function trackValuesEqual(left, right) {
  if (Object.is(left, right)) return true;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left)
      && Array.isArray(right)
      && left.length === right.length
      && left.every((value, index) => trackValuesEqual(value, right[index]));
  }
  if (!left || !right || typeof left !== "object" || typeof right !== "object") return false;
  const leftKeys = Object.keys(left).sort();
  const rightKeys = Object.keys(right).sort();
  return leftKeys.length === rightKeys.length
    && leftKeys.every((key, index) => key === rightKeys[index] && trackValuesEqual(left[key], right[key]));
}

function assertInterpolatable(from, to, property) {
  if (Array.isArray(from) || Array.isArray(to)) {
    if (!Array.isArray(from) || !Array.isArray(to) || from.length !== to.length) {
      throw new RangeError(`Transform property ${property} needs equally sized values.`);
    }
    for (let index = 0; index < from.length; index += 1) {
      assertInterpolatable(from[index], to[index], `${property}[${index}]`);
    }
    return;
  }
  if (from && to && typeof from === "object" && typeof to === "object") {
    if ("op" in from || "op" in to) {
      if (from.op !== to.op) throw new RangeError(`Transform property ${property} needs matching path commands.`);
      return;
    }
    if ("stops" in from || "stops" in to) {
      if (!Array.isArray(from.stops) || !Array.isArray(to.stops) || from.stops.length !== to.stops.length) {
        throw new RangeError(`Transform property ${property} needs gradients with matching stops.`);
      }
    }
    return;
  }
  if (typeof from !== typeof to) throw new TypeError(`Transform property ${property} has incompatible values.`);
}

function transformPairs(source, target, result = []) {
  if (!(source instanceof Mobject) || !(target instanceof Mobject)) {
    throw new TypeError("Transform source and target must be Mobjects.");
  }
  if (source.node.type !== target.node.type) {
    throw new RangeError(`Transform cannot interpolate ${source.node.type} into ${target.node.type} with the current retained properties.`);
  }
  result.push([source, target]);
  if (source instanceof Group || target instanceof Group) {
    if (!(source instanceof Group) || !(target instanceof Group) || source._members.length !== target._members.length) {
      throw new RangeError("Transform groups need matching member hierarchies.");
    }
    for (let index = 0; index < source._members.length; index += 1) {
      transformPairs(source._members[index], target._members[index], result);
    }
  }
  return result;
}

function transformPairTracks(source, target, options) {
  const fromNode = source.node;
  const toNode = target.node;
  const tracks = [];
  const addTrack = (property, from, to) => {
    if (trackValuesEqual(from, to)) return;
    if ((property === "fill" || property === "stroke") && (from === null || to === null)) {
      from ??= "#00000000";
      to ??= "#00000000";
    }
    if (from === null || to === null || from === undefined || to === undefined) {
      throw new RangeError(`Transform property ${property} cannot interpolate an absent retained value.`);
    }
    assertInterpolatable(from, to, property);
    tracks.push(animate(source, property, from, to, options));
  };

  for (const [property, fallback] of Object.entries(TRANSFORM_DEFAULTS)) {
    addTrack(property, fromNode.transform?.[property] ?? fallback, toNode.transform?.[property] ?? fallback);
  }
  for (const [property, fallback] of Object.entries(STYLE_DEFAULTS)) {
    addTrack(property, fromNode.style?.[property] ?? fallback, toNode.style?.[property] ?? fallback);
  }

  const recognized = new Set(["id", "type", "transform", "style"]);
  for (const key of new Set([...Object.keys(fromNode), ...Object.keys(toNode)])) {
    if (recognized.has(key)) continue;
    const property = DIRECT_TRACK_PROPERTIES.get(key);
    if (property) {
      addTrack(property, fromNode[key], toNode[key]);
      recognized.add(key);
      continue;
    }
    if (!trackValuesEqual(fromNode[key], toNode[key])) {
      throw new RangeError(`Transform cannot animate retained field ${key}.`);
    }
  }
  return tracks;
}

function attachAnimationEvents(tracks, events) {
  if (events.length > 0) {
    Object.defineProperty(tracks, ANIMATION_EVENTS, {
      configurable: false,
      enumerable: false,
      value: events,
      writable: false,
    });
  }
  return tracks;
}

function animationEnd(tracks) {
  return tracks.reduce(
    (end, track) => Math.max(end, ...(track.keyframes ?? []).map(({ at }) => at)),
    0,
  );
}

/** Interpolate every retained property that both Mobjects can represent faithfully. */
export function Transform(source, target, options = {}) {
  const tracks = transformPairs(source, target).flatMap(([from, to]) => transformPairTracks(from, to, options));
  if (tracks.length === 0) tracks.push(animate(source, "opacity", source.node.style?.opacity ?? 1, source.node.style?.opacity ?? 1, options));
  return tracks;
}

/** Transform the source, then swap its retained hierarchy for the target hierarchy. */
export function ReplacementTransform(source, target, options = {}) {
  if (source.id === target.id) throw new RangeError("ReplacementTransform source and target need distinct ids.");
  const tracks = Transform(source, target, options);
  return attachAnimationEvents(tracks, [{ type: "replace", at: animationEnd(tracks), source: source.id, target }]);
}

function isAnimationOptions(value) {
  return value && typeof value === "object" && !Array.isArray(value) && !(value instanceof Mobject)
    && !("target" in value) && !("property" in value) && !("keyframes" in value);
}

function splitAnimationArguments(args) {
  const values = [...args];
  const options = isAnimationOptions(values.at(-1)) ? values.pop() : {};
  if (values.length === 0) throw new TypeError("Animation composition needs at least one animation.");
  return { animations: values, options };
}

function collectAnimationParts(animation, label = "animation") {
  const tracks = [];
  const events = [];
  const visit = (value, path) => {
    if (Array.isArray(value)) {
      if (value[ANIMATION_EVENTS]) events.push(...value[ANIMATION_EVENTS]);
      value.forEach((member, index) => visit(member, `${path}[${index}]`));
      return;
    }
    if (!value || typeof value !== "object") throw new TypeError(`${path} must be a track or track array.`);
    tracks.push(structuredClone(value));
  };
  visit(animation, label);
  if (tracks.length === 0) throw new TypeError(`${label} needs at least one track.`);
  return { tracks, events: events.map((event) => ({ ...event })) };
}

function transformAnimationParts(parts, scale, offset) {
  return {
    tracks: parts.tracks.map((track) => ({
      ...track,
      keyframes: track.keyframes.map((keyframe) => ({ ...keyframe, at: keyframe.at * scale + offset })),
    })),
    events: parts.events.map((event) => ({ ...event, at: event.at * scale + offset })),
  };
}

function requestedDuration(options, label) {
  if (options.duration !== undefined && options.runTime !== undefined && options.duration !== options.runTime) {
    throw new RangeError(`${label}.duration and ${label}.runTime must match when both are supplied.`);
  }
  const value = options.runTime ?? options.duration;
  return value === undefined ? undefined : assertPositive(value, `${label} duration`);
}

function composeAnimations(args, defaultLagRatio, label) {
  const { animations, options } = splitAnimationArguments(args);
  const lagRatio = assertNonNegative(options.lagRatio ?? defaultLagRatio, `${label} lagRatio`);
  const start = assertNonNegative(options.start ?? 0, `${label} start`);
  const children = animations.map((animation, index) => collectAnimationParts(animation, `${label}[${index}]`));
  let nextStart = 0;
  let naturalEnd = 0;
  const scheduled = [];
  for (const child of children) {
    const duration = animationEnd(child.tracks);
    const shifted = transformAnimationParts(child, 1, nextStart);
    scheduled.push(shifted);
    naturalEnd = Math.max(naturalEnd, nextStart + duration, ...shifted.events.map(({ at }) => at));
    nextStart += duration * lagRatio;
  }
  if (naturalEnd <= 0) throw new RangeError(`${label} animations must have positive duration.`);
  const duration = requestedDuration(options, label);
  const scale = duration === undefined ? 1 : duration / naturalEnd;
  const tracks = [];
  const events = [];
  for (const part of scheduled) {
    const transformed = transformAnimationParts(part, scale, start);
    tracks.push(...transformed.tracks);
    events.push(...transformed.events);
  }
  return attachAnimationEvents(tracks, events);
}

/** Play animations in parallel unless lagRatio delays each subsequent start. */
export function AnimationGroup(...args) { return composeAnimations(args, 0, "AnimationGroup"); }

/** Start each animation shortly after the preceding animation begins. */
export function LaggedStart(...args) { return composeAnimations(args, 0.05, "LaggedStart"); }

/** Play animations one after another. */
export function Succession(...args) { return composeAnimations(args, 1, "Succession"); }

function normalizeKeyframes(keyframes, label, numberOnly = false) {
  if (!Array.isArray(keyframes) || keyframes.length === 0) throw new TypeError(`${label} needs at least one keyframe.`);
  let previous = -1;
  return keyframes.map((keyframe, index) => {
    if (!keyframe || typeof keyframe !== "object" || Array.isArray(keyframe)) throw new TypeError(`${label}[${index}] must be an object.`);
    const at = assertNonNegative(keyframe.at, `${label}[${index}].at`);
    if (at < previous) throw new RangeError(`${label} times must be nondecreasing.`);
    previous = at;
    const value = numberOnly
      ? assertFinite(keyframe.value, `${label}[${index}].value`)
      : assertTrackValue(keyframe.value, `${label}[${index}].value`);
    const normalized = { at, value: structuredClone(value) };
    if (keyframe.easing !== undefined) normalized.easing = assertEnum(keyframe.easing, EASINGS, `${label}[${index}].easing`);
    return normalized;
  });
}

function collectSceneNodes(item, parent, result, visiting) {
  if (Array.isArray(item)) {
    for (const member of item) collectSceneNodes(member, parent, result, visiting);
    return;
  }
  if (item instanceof Group) {
    if (visiting.has(item)) throw new RangeError("Group hierarchy contains a cycle.");
    visiting.add(item);
    const node = item.toJSON();
    if (parent !== null) node.parent = parent;
    result.push(node);
    for (const member of item._members) collectSceneNodes(member, item.id, result, visiting);
    visiting.delete(item);
    return;
  }
  if (item instanceof Mobject) {
    const node = item.toJSON();
    if (parent !== null) node.parent = parent;
    result.push(node);
    return;
  }
  if (item && typeof item === "object") {
    const node = structuredClone(item);
    if (parent !== null) node.parent = parent;
    result.push(node);
    return;
  }
  throw new TypeError("Scene members must be Mobjects or retained node objects.");
}

function normalizeCamera(camera) {
  if (!camera || typeof camera !== "object" || Array.isArray(camera)) throw new TypeError("camera must be an object.");
  const allowed = new Set(["x", "y", "zoom", "rotation"]);
  const result = {};
  for (const [key, value] of Object.entries(camera)) {
    if (!allowed.has(key)) throw new TypeError(`Unknown camera property: ${key}.`);
    result[key] = key === "zoom" ? assertPositive(value, "camera.zoom") : assertFinite(value, `camera.${key}`);
  }
  return result;
}

function normalizeCamera3D(camera) {
  if (!camera || typeof camera !== "object" || Array.isArray(camera)) throw new TypeError("camera3D must be an object.");
  const vectors = new Set(["position", "target", "up", "lightDirection"]);
  const numbers = new Set(["fovY", "near", "far", "ambient"]);
  const result = {};
  for (const [key, value] of Object.entries(camera)) {
    if (vectors.has(key)) result[key] = assertPoint3D(value, `camera3D.${key}`);
    else if (numbers.has(key)) result[key] = assertFinite(value, `camera3D.${key}`);
    else throw new TypeError(`Unknown camera3D property: ${key}.`);
  }
  if ("fovY" in result && (result.fovY <= 0.05 || result.fovY >= Math.PI)) throw new RangeError("camera3D.fovY must be between 0.05 and PI.");
  if ("near" in result && result.near <= 0) throw new RangeError("camera3D.near must be positive.");
  if ("far" in result && result.far <= 0) throw new RangeError("camera3D.far must be positive.");
  if ("ambient" in result) assertUnitInterval(result.ambient, "camera3D.ambient");
  return result;
}

function mergeTrackSequence(tracks, label) {
  const ordered = tracks
    .map((track, index) => ({ track, index }))
    .sort((left, right) => {
      const startDifference = left.track.keyframes[0].at - right.track.keyframes[0].at;
      return startDifference === 0 ? left.index - right.index : startDifference;
    });
  const first = ordered[0].track;
  const merged = {
    target: first.target,
    property: first.property,
    keyframes: structuredClone(first.keyframes),
  };
  for (const { track } of ordered.slice(1)) {
    const previous = merged.keyframes.at(-1);
    const next = track.keyframes[0];
    if (next.at < previous.at - 1e-9) {
      throw new RangeError(`${label} contains overlapping animations for ${track.target}.${track.property}.`);
    }
    if (next.at > previous.at) {
      merged.keyframes.push({ at: next.at, value: structuredClone(previous.value) });
    }
    const boundary = merged.keyframes.at(-1);
    const startIndex = boundary.at === next.at && trackValuesEqual(boundary.value, next.value) ? 1 : 0;
    merged.keyframes.push(...structuredClone(track.keyframes.slice(startIndex)));
  }
  return merged;
}

function mergeScheduledTracks(tracks, label) {
  const groups = new Map();
  for (const track of tracks) {
    const key = `${track.target}\u0000${track.property}`;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(track);
  }
  return [...groups.values()].map((members) => mergeTrackSequence(members, label));
}

function appendSceneTrack(data, track) {
  const existingIndex = data.tracks.findIndex((candidate) => (
    candidate.target === track.target
    && candidate.property === track.property
    && Array.isArray(candidate.keyframes)
    && candidate.keyframes.length > 0
    && candidate.keyframesFrom === undefined
  ));
  if (existingIndex === -1) {
    data.tracks.push(track);
    return;
  }
  data.tracks[existingIndex] = mergeTrackSequence([data.tracks[existingIndex], track], "Scene.play");
}

function hierarchyIds(nodes, rootId) {
  const result = new Set([rootId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const node of nodes) {
      if (node.parent !== undefined && result.has(node.parent) && !result.has(node.id)) {
        result.add(node.id);
        changed = true;
      }
    }
  }
  return result;
}

function applyReplacementEvent(data, event, at) {
  const sourceNode = data.nodes.find(({ id }) => id === event.source);
  if (!sourceNode) throw new RangeError(`ReplacementTransform source ${event.source} is not in the Scene.`);
  const targetNodes = [];
  collectSceneNodes(event.target, null, targetNodes, new Set());
  const targetRoot = targetNodes[0];
  if (!targetRoot) throw new RangeError("ReplacementTransform target hierarchy is empty.");
  for (const targetNode of targetNodes) {
    const existing = data.nodes.find(({ id }) => id === targetNode.id);
    if (existing && existing.type !== targetNode.type) {
      throw new RangeError(`ReplacementTransform target id ${targetNode.id} already has a different retained type.`);
    }
    if (!existing) data.nodes.push(targetNode);
  }
  const sourceIds = hierarchyIds(data.nodes, event.source);
  const targetIds = hierarchyIds(data.nodes, targetRoot.id);
  for (const node of data.nodes) {
    if (sourceIds.has(node.id)) node.disappearAt = at;
    if (targetIds.has(node.id)) node.appearAt = at;
  }
}

export class Scene {
  constructor(options = {}) {
    const {
      nodes = [], tracks = [], signals = [], bindings = [], controls = [], audio = [], captions = [],
      ...settings
    } = options;
    this.data = {
      ...EMPTY_SCENE,
      title: settings.title ?? "Manim web scene",
      ...settings,
      nodes: structuredClone(nodes),
      tracks: structuredClone(tracks),
      signals: structuredClone(signals),
      bindings: structuredClone(bindings),
      controls: structuredClone(controls),
      audio: structuredClone(audio),
      captions: structuredClone(captions),
    };
    this.cursor = this.data.tracks.reduce(
      (end, track) => Math.max(end, ...(track.keyframes ?? []).map(({ at }) => at)),
      0,
    );
    const retainedDuration = this.data.nodes.reduce(
      (end, node) => node?.type === "tracePath"
        ? Math.max(end, ...(node.frames ?? []).map(({ at }) => at))
        : end,
      0,
    );
    this.data.duration = Math.max(this.data.duration, this.cursor, retainedDuration, 0.25);
  }

  add(...objects) {
    const nodes = [];
    for (const object of objects) collectSceneNodes(object, null, nodes, new Set());
    this.data.nodes.push(...nodes);
    this.data.duration = nodes.reduce(
      (duration, node) => node.type === "tracePath"
        ? Math.max(duration, ...node.frames.map(({ at }) => at))
        : duration,
      this.data.duration,
    );
    return this;
  }
  play(...animations) {
    const { animations: animationValues, options } = splitAnimationArguments(animations);
    const allowedOptions = new Set(["duration", "runTime", "lagRatio"]);
    for (const key of Object.keys(options)) {
      if (!allowedOptions.has(key)) throw new TypeError(`Unknown Scene.play option: ${key}.`);
    }
    const grouped = composeAnimations([
      ...animationValues,
      {
        lagRatio: options.lagRatio ?? 0,
        ...(options.duration === undefined ? {} : { duration: options.duration }),
        ...(options.runTime === undefined ? {} : { runTime: options.runTime }),
      },
    ], 0, "Scene.play");
    const relativeTracks = grouped.map((track, index) => {
      if (!track || typeof track !== "object" || Array.isArray(track)) throw new TypeError(`animation[${index}] must be a track.`);
      return {
        target: idOf(track.target),
        property: assertString(track.property, `animation[${index}].property`),
        keyframes: normalizeKeyframes(track.keyframes, `animation[${index}].keyframes`),
      };
    });
    const offset = this.cursor;
    for (const track of relativeTracks) {
      for (const keyframe of track.keyframes) keyframe.at += offset;
    }
    const tracks = mergeScheduledTracks(relativeTracks, "Scene.play");
    for (const track of tracks) appendSceneTrack(this.data, track);
    const events = (grouped[ANIMATION_EVENTS] ?? []).map((event) => ({ ...event, at: event.at + offset }));
    for (const event of events) {
      if (event.type === "replace") applyReplacementEvent(this.data, event, event.at);
    }
    const end = Math.max(
      offset,
      ...tracks.flatMap((track) => track.keyframes.map(({ at }) => at)),
      ...events.map(({ at }) => at),
    );
    this.cursor = end;
    this.data.duration = Math.max(this.data.duration, end, 0.25);
    return this;
  }
  wait(seconds = 1) { this.cursor += assertNonNegative(seconds, "wait duration"); this.data.duration = Math.max(this.data.duration, this.cursor, 0.25); return this; }
  track(target, property, keyframes) {
    const normalized = normalizeKeyframes(keyframes, "track keyframes");
    this.data.tracks.push({ target: idOf(target), property: assertString(property, "track property"), keyframes: normalized });
    this.cursor = Math.max(this.cursor, ...normalized.map(({ at }) => at));
    this.data.duration = Math.max(this.data.duration, this.cursor);
    return this;
  }
  signal(id, keyframes) {
    const normalized = normalizeKeyframes(keyframes, "signal keyframes", true);
    this.data.signals.push({ id: assertId(id, "signal id"), keyframes: normalized });
    this.data.duration = Math.max(this.data.duration, ...normalized.map(({ at }) => at), 0.25);
    return this;
  }
  bind(target, property, expression) {
    assertTrackValue(expression, "binding expression");
    this.data.bindings.push({ target: idOf(target), property: assertString(property, "binding property"), expression: structuredClone(expression) });
    return this;
  }
  control(signal, options = {}) {
    const min = assertFinite(options.min ?? 0, "control min");
    const max = assertFinite(options.max ?? 1, "control max");
    if (max <= min) throw new RangeError("control max must be greater than min.");
    const value = assertFinite(options.default ?? min, "control default");
    if (value < min || value > max) throw new RangeError("control default must be within min and max.");
    this.data.controls.push({
      id: assertId(options.id ?? `${signal}-control`, "control id"),
      label: assertString(options.label ?? signal, "control label", { max: 80 }),
      signal: assertId(signal, "control signal"),
      min,
      max,
      step: assertPositive(options.step ?? 0.01, "control step"),
      default: value,
      timeline: options.timeline ?? false,
    });
    return this;
  }
  setCamera(camera) { this.data.camera = { ...(this.data.camera ?? {}), ...normalizeCamera(camera) }; return this; }
  setCamera3D(camera) {
    const next = { ...(this.data.camera3d ?? {}), ...normalizeCamera3D(camera) };
    const near = next.near ?? 0.1;
    const far = next.far ?? 100;
    if (far <= near) throw new RangeError("camera3D.far must be greater than camera3D.near.");
    this.data.camera3d = next;
    return this;
  }
  toJSON() { return structuredClone(this.data); }
  toString() { return JSON.stringify(this.data); }
}

async function initializeRuntime(wasmUrl) {
  const source = wasmUrl === undefined ? "<package-default>" : String(wasmUrl);
  if (runtimeInitializationPromise && source !== runtimeSource) {
    throw new Error(`The Wasm runtime is already initialized from ${runtimeSource}; one document cannot mix Wasm binaries.`);
  }
  if (!runtimeInitializationPromise) {
    runtimeSource = source;
    runtimeInitializationPromise = initRuntime(wasmUrl).catch((error) => {
      runtimeInitializationPromise = null;
      runtimeSource = undefined;
      throw error;
    });
  }
  return runtimeInitializationPromise;
}

export async function createManimPlayer(options) {
  const canvas = options?.canvas;
  if (!(canvas instanceof HTMLCanvasElement)) throw new TypeError("createManimPlayer requires a canvas element.");
  if (!navigator.gpu) throw new Error("This browser does not support WebGPU.");
  await initializeRuntime(options?.wasmUrl);
  const fontFaces = options?.fonts ?? [];
  if (!Array.isArray(fontFaces) || fontFaces.length > 64) throw new RangeError("fonts must contain at most 64 font faces.");
  const fonts = fontFaces.map((face, index) => normalizeFontFace(face, `fonts[${index}]`));
  const fontBytesTotal = fonts.reduce((total, face) => total + face.data.byteLength, 0);
  if (fontBytesTotal > 128 * 1024 * 1024) throw new RangeError("fonts must not exceed 128 MiB in total.");
  let engine;
  try {
    engine = await wasmRuntime.create_player(canvas);
    for (const font of fonts) engine.register_font(font.family, font.weight, font.slant, font.data);
    engine.load_scene(JSON.stringify(sceneObject(options?.scene ?? EMPTY_SCENE)));
    engine.set_paused(options?.autoplay === false);

    let disposed = false;
    const ready = () => {
      if (disposed || engine.is_destroyed()) throw new Error("This Manim player has been destroyed.");
    };
    const player = {
      canvas,
      load: (scene) => { ready(); engine.load_scene(JSON.stringify(sceneObject(scene))); return player; },
      validate: (scene) => { ready(); return JSON.parse(wasmRuntime.validate_scene(JSON.stringify(sceneObject(scene)))); },
      evaluate: (scene, time) => { ready(); return JSON.parse(wasmRuntime.evaluate_scene(JSON.stringify(sceneObject(scene)), assertNonNegative(time, "evaluation time"))); },
      play: () => { ready(); engine.resume(); },
      pause: () => { ready(); engine.set_paused(true); },
      seek: (time) => { ready(); engine.seek(assertNonNegative(time, "seek time")); },
      time: () => { ready(); return engine.current_time(); },
      setSignal: (id, value) => { ready(); engine.set_signal(assertId(id, "signal id"), assertFinite(value, "signal value")); },
      registerFont: (family, data, fontOptions = {}) => {
        ready();
        assertObject(fontOptions, "font options");
        for (const key of Object.keys(fontOptions)) {
          if (key !== "weight" && key !== "slant") throw new TypeError(`Unknown font option: ${key}.`);
        }
        const font = normalizeFontFace({ family, data, weight: fontOptions.weight, slant: fontOptions.slant });
        engine.register_font(font.family, font.weight, font.slant, font.data);
        return player;
      },
      setSize: (width, height) => { ready(); engine.set_render_size(assertPositive(width, "render width"), assertPositive(height, "render height")); },
      clearSize: () => { ready(); engine.clear_render_size(); },
      reset: () => { ready(); engine.reset_clock(); },
      diagnostics: () => { ready(); return { recoveryCount: engine.recovery_count(), registeredFontFaces: engine.registered_font_count(), webgpu: true }; },
      destroy: () => {
        if (disposed) return;
        disposed = true;
        engine.destroy();
      },
    };
    return player;
  } catch (error) {
    try { engine?.destroy(); } catch { /* Engine creation may not have completed. */ }
    throw error;
  }
}
