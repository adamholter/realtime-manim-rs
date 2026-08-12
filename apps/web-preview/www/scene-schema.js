const NODE_TYPES = new Set([
  "group",
  "billboard",
  "circle",
  "rect",
  "line",
  "arrow",
  "polyline",
  "path",
  "path3d",
  "tracePath",
  "pathRef",
  "text",
  "markupText",
  "mathTex",
  "svg",
  "image",
  "pointCloud",
  "mesh",
  "surface",
  "customShaderMesh",
]);
const PROPERTIES = new Set([
  "x",
  "y",
  "z",
  "rotation",
  "rotationX",
  "rotationY",
  "scaleX",
  "scaleY",
  "scaleZ",
  "opacity",
  "strokeWidth",
  "dashOffset",
  "drawStart",
  "drawProgress",
  "drawRange",
  "fill",
  "fillGradient",
  "stroke",
  "strokeGradient",
  "radius",
  "points",
  "vertices",
  "normals",
  "colors",
  "surfaceColors",
  "strokeRadii",
  "lightPosition",
  "commands",
  "pathData",
  "transform2d",
  "affine2d",
  "billboardAnchor",
  "cameraX",
  "cameraY",
  "cameraZoom",
  "cameraRotation",
  "camera3dPosition",
  "camera3dTarget",
  "camera3dUp",
  "camera3dFovY",
  "camera3d",
  "camera3dOrbit",
  "shaderVertexData",
  "shaderUniformValues",
]);
const STROKE_CAPS = new Set(["butt", "square", "round"]);
const STROKE_JOINS = new Set(["miter", "miterClip", "round", "bevel"]);
const CAMERA_PROPERTIES = new Set([
  "cameraX",
  "cameraY",
  "cameraZoom",
  "cameraRotation",
  "camera3dPosition",
  "camera3dTarget",
  "camera3dUp",
  "camera3dFovY",
  "camera3d",
  "camera3dOrbit",
]);
const COLOR_PROPERTIES = new Set(["fill", "stroke"]);
const GRADIENT_PROPERTIES = new Set(["fillGradient", "strokeGradient"]);
const NON_NUMERIC_PROPERTIES = new Set([
  ...COLOR_PROPERTIES,
  ...GRADIENT_PROPERTIES,
  "points",
  "vertices",
  "normals",
  "colors",
  "surfaceColors",
  "strokeRadii",
  "lightPosition",
  "commands",
  "pathData",
  "transform2d",
  "affine2d",
  "billboardAnchor",
  "camera3dPosition",
  "camera3dTarget",
  "camera3dUp",
  "camera3d",
  "camera3dOrbit",
  "shaderVertexData",
  "shaderUniformValues",
]);
const EASINGS = new Set([
  "linear",
  "smooth",
  "manimSmooth",
  "easeIn",
  "easeOut",
  "easeInOut",
  "thereAndBack",
  "bounce",
]);
const CORRESPONDENCE_KINDS = new Set([
  "transformMatchingTex",
  "transformMatchingShapes",
]);
const CORRESPONDENCE_MODES = new Set([
  "transform",
  "keyMapped",
  "transformMismatches",
  "fadeTransformMismatches",
  "fadeOut",
  "fadeIn",
]);
const CORRESPONDENCE_KEYS = new Set([
  "id",
  "kind",
  "mode",
  "keys",
  "targetKeys",
  "sourceNodes",
  "targetNodes",
  "start",
  "end",
  "pathArc",
]);
const EXPR_OPS = new Set([
  "constant",
  "time",
  "signal",
  "add",
  "multiply",
  "subtract",
  "divide",
  "sin",
  "cos",
  "abs",
  "min",
  "max",
  "clamp",
  "lerp",
]);
const PATH_OPS = new Set(["moveTo", "lineTo", "quadTo", "cubicTo", "close"]);
const HEX_COLOR = /^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/i;
const ID = /^[A-Za-z0-9_.-]{1,80}$/;
const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;
const AUDIO_MIME_TYPES = new Set([
  "audio/wav",
  "audio/x-wav",
  "audio/mpeg",
  "audio/mp4",
  "audio/aac",
  "audio/ogg",
  "audio/webm",
  "audio/flac",
]);

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function object(value, name) {
  assert(value && typeof value === "object" && !Array.isArray(value), `${name} must be an object.`);
  return value;
}

function number(value, name, minimum = -100_000, maximum = 100_000) {
  assert(Number.isFinite(value), `${name} must be a finite number.`);
  assert(value >= minimum && value <= maximum, `${name} must be ${minimum}–${maximum}.`);
  return value;
}

function integer(value, name, minimum, maximum) {
  number(value, name, minimum, maximum);
  assert(Number.isInteger(value), `${name} must be an integer.`);
  return value;
}

function text(value, name, maximum = 4_000) {
  assert(typeof value === "string" && value.length > 0 && value.length <= maximum, `${name} must contain 1–${maximum} characters.`);
  return value;
}

function id(value, name) {
  assert(typeof value === "string" && ID.test(value), `${name} is invalid.`);
  return value;
}

function color(value, name) {
  assert(typeof value === "string" && HEX_COLOR.test(value), `${name} must be #rrggbb or #rrggbbaa.`);
  return value.toLowerCase();
}

function optionalColor(value, name) {
  return value == null ? null : color(value, name);
}

function linearGradient(value, name) {
  if (value == null) return null;
  object(value, name);
  const from = point(value.from, `${name}.from`);
  const to = point(value.to, `${name}.to`);
  const dx = to[0] - from[0];
  const dy = to[1] - from[1];
  assert(dx * dx + dy * dy > Number.EPSILON, `${name} endpoints must be distinct.`);
  assert(
    Array.isArray(value.stops) && value.stops.length >= 2 && value.stops.length <= 64,
    `${name}.stops must contain 2–64 values.`,
  );
  let previous = -1;
  const stops = value.stops.map((stop, index) => {
    const stopName = `${name}.stops[${index}]`;
    object(stop, stopName);
    const offset = number(stop.offset, `${stopName}.offset`, 0, 1);
    assert(offset >= previous, `${name}.stops must be ordered.`);
    previous = offset;
    return { offset, color: color(stop.color, `${stopName}.color`) };
  });
  const spread = value.spread ?? "pad";
  assert(["pad", "repeat", "reflect"].includes(spread), `${name}.spread is invalid.`);
  const space = value.space ?? "local";
  assert(["local", "world"].includes(space), `${name}.space is invalid.`);
  return { from, to, stops, spread, space };
}

function point(value, name) {
  assert(Array.isArray(value) && value.length === 2, `${name} must be [x, y].`);
  return [number(value[0], `${name}[0]`), number(value[1], `${name}[1]`)];
}

function point3(value, name) {
  assert(Array.isArray(value) && value.length === 3, `${name} must be [x, y, z].`);
  return [
    number(value[0], `${name}[0]`),
    number(value[1], `${name}[1]`),
    number(value[2], `${name}[2]`),
  ];
}

function points(value, name, minimum = 1) {
  assert(Array.isArray(value) && value.length >= minimum && value.length <= 100_000, `${name} must contain ${minimum}–100,000 points.`);
  return value.map((item, index) => point(item, `${name}[${index}]`));
}

function transform(value = {}, name = "transform") {
  object(value, name);
  return {
    x: number(value.x ?? 0, `${name}.x`),
    y: number(value.y ?? 0, `${name}.y`),
    z: number(value.z ?? 0, `${name}.z`),
    rotation: number(value.rotation ?? 0, `${name}.rotation`, -1_000, 1_000),
    rotationX: number(value.rotationX ?? 0, `${name}.rotationX`, -1_000, 1_000),
    rotationY: number(value.rotationY ?? 0, `${name}.rotationY`, -1_000, 1_000),
    scaleX: number(value.scaleX ?? 1, `${name}.scaleX`, -1_000, 1_000),
    scaleY: number(value.scaleY ?? 1, `${name}.scaleY`, -1_000, 1_000),
    scaleZ: number(value.scaleZ ?? 1, `${name}.scaleZ`, -1_000, 1_000),
  };
}

function style(value = {}, name = "style") {
  object(value, name);
  const strokeCap = value.strokeCap ?? "butt";
  assert(STROKE_CAPS.has(strokeCap), `${name}.strokeCap must be butt, square, or round.`);
  const strokeJoin = value.strokeJoin ?? "miter";
  assert(STROKE_JOINS.has(strokeJoin), `${name}.strokeJoin must be miter, miterClip, round, or bevel.`);
  const dashArray = value.dashArray ?? [];
  assert(Array.isArray(dashArray) && dashArray.length <= 64, `${name}.dashArray must contain at most 64 values.`);
  return {
    fill: optionalColor(value.fill ?? null, `${name}.fill`),
    fillGradient: linearGradient(value.fillGradient ?? null, `${name}.fillGradient`),
    stroke: optionalColor(value.stroke === undefined ? "#f8fafc" : value.stroke, `${name}.stroke`),
    strokeGradient: linearGradient(value.strokeGradient ?? null, `${name}.strokeGradient`),
    strokeWidth: number(value.strokeWidth ?? 0.04, `${name}.strokeWidth`, 0, 10),
    strokeCap,
    strokeJoin,
    dashArray: dashArray.map((entry, index) => number(entry, `${name}.dashArray[${index}]`, 0.000_01, 100_000)),
    dashOffset: number(value.dashOffset ?? 0, `${name}.dashOffset`),
    opacity: number(value.opacity ?? 1, `${name}.opacity`, 0, 1),
    drawStart: number(value.drawStart ?? 0, `${name}.drawStart`, 0, 1),
    drawProgress: number(value.drawProgress ?? 1, `${name}.drawProgress`, 0, 1),
  };
}

function baseNode(candidate, index, duration) {
  const name = `nodes[${index}]`;
  object(candidate, name);
  assert(NODE_TYPES.has(candidate.type), `${name}.type must be one of: ${[...NODE_TYPES].join(", ")}.`);
  const node = {
    id: id(candidate.id, `${name}.id`),
    ...(candidate.parent == null ? {} : { parent: id(candidate.parent, `${name}.parent`) }),
    zIndex: integer(candidate.zIndex ?? 0, `${name}.zIndex`, -100_000, 100_000),
    transform: transform(candidate.transform, `${name}.transform`),
    style: style(candidate.style, `${name}.style`),
    appearAt: number(candidate.appearAt ?? 0, `${name}.appearAt`, 0, duration),
    ...(candidate.disappearAt == null
      ? {}
      : { disappearAt: number(candidate.disappearAt, `${name}.disappearAt`, 0, duration + 0.0001) }),
    type: candidate.type,
  };
  if (node.disappearAt !== undefined) {
    assert(node.disappearAt > node.appearAt, `${name}.disappearAt must be after appearAt.`);
  }
  return node;
}

function pathCommand(candidate, name) {
  object(candidate, name);
  assert(PATH_OPS.has(candidate.op), `${name}.op is unsupported.`);
  if (candidate.op === "close") return { op: "close" };
  if (candidate.op === "moveTo" || candidate.op === "lineTo") {
    return { op: candidate.op, x: number(candidate.x, `${name}.x`), y: number(candidate.y, `${name}.y`) };
  }
  if (candidate.op === "quadTo") {
    return {
      op: "quadTo",
      cx: number(candidate.cx, `${name}.cx`),
      cy: number(candidate.cy, `${name}.cy`),
      x: number(candidate.x, `${name}.x`),
      y: number(candidate.y, `${name}.y`),
    };
  }
  return {
    op: "cubicTo",
    c1x: number(candidate.c1x, `${name}.c1x`),
    c1y: number(candidate.c1y, `${name}.c1y`),
    c2x: number(candidate.c2x, `${name}.c2x`),
    c2y: number(candidate.c2y, `${name}.c2y`),
    x: number(candidate.x, `${name}.x`),
    y: number(candidate.y, `${name}.y`),
  };
}

function pathCommandDataLength(commands) {
  return commands.reduce((total, command) => total + ({
    moveTo: 2,
    lineTo: 2,
    quadTo: 4,
    cubicTo: 6,
    close: 0,
  })[command.op], 0);
}

function pathCommand3d(candidate, name) {
  object(candidate, name);
  assert(PATH_OPS.has(candidate.op), `${name}.op is unsupported.`);
  if (candidate.op === "close") return { op: "close" };
  if (candidate.op === "moveTo" || candidate.op === "lineTo") {
    return {
      op: candidate.op,
      x: number(candidate.x, `${name}.x`),
      y: number(candidate.y, `${name}.y`),
      z: number(candidate.z, `${name}.z`),
    };
  }
  if (candidate.op === "quadTo") {
    return {
      op: "quadTo",
      cx: number(candidate.cx, `${name}.cx`),
      cy: number(candidate.cy, `${name}.cy`),
      cz: number(candidate.cz, `${name}.cz`),
      x: number(candidate.x, `${name}.x`),
      y: number(candidate.y, `${name}.y`),
      z: number(candidate.z, `${name}.z`),
    };
  }
  return {
    op: "cubicTo",
    c1x: number(candidate.c1x, `${name}.c1x`),
    c1y: number(candidate.c1y, `${name}.c1y`),
    c1z: number(candidate.c1z, `${name}.c1z`),
    c2x: number(candidate.c2x, `${name}.c2x`),
    c2y: number(candidate.c2y, `${name}.c2y`),
    c2z: number(candidate.c2z, `${name}.c2z`),
    x: number(candidate.x, `${name}.x`),
    y: number(candidate.y, `${name}.y`),
    z: number(candidate.z, `${name}.z`),
  };
}

function validateNode(candidate, index, duration) {
  const name = `nodes[${index}]`;
  const node = baseNode(candidate, index, duration);
  switch (candidate.type) {
    case "group":
      return node;
    case "billboard":
      return { ...node, anchor: point3(candidate.anchor, `${name}.anchor`), base: point(candidate.base, `${name}.base`) };
    case "circle":
      return { ...node, radius: number(candidate.radius, `${name}.radius`, 0.0001, 10_000) };
    case "rect":
      return {
        ...node,
        width: number(candidate.width, `${name}.width`, 0.0001, 10_000),
        height: number(candidate.height, `${name}.height`, 0.0001, 10_000),
        cornerRadius: number(candidate.cornerRadius ?? 0, `${name}.cornerRadius`, 0, 10_000),
      };
    case "line":
      return { ...node, from: point(candidate.from, `${name}.from`), to: point(candidate.to, `${name}.to`) };
    case "arrow":
      return {
        ...node,
        from: point(candidate.from, `${name}.from`),
        to: point(candidate.to, `${name}.to`),
        tipSize: number(candidate.tipSize ?? 0.24, `${name}.tipSize`, 0.001, 100),
      };
    case "polyline":
      return { ...node, points: points(candidate.points, `${name}.points`, 2), closed: candidate.closed === true };
    case "path":
      assert(Array.isArray(candidate.commands) && candidate.commands.length >= 1 && candidate.commands.length <= 50_000, `${name}.commands must contain 1–50,000 commands.`);
      return { ...node, commands: candidate.commands.map((command, commandIndex) => pathCommand(command, `${name}.commands[${commandIndex}]`)) };
    case "path3d":
      assert(Array.isArray(candidate.commands) && candidate.commands.length >= 1 && candidate.commands.length <= 50_000, `${name}.commands must contain 1–50,000 commands.`);
      return { ...node, commands: candidate.commands.map((command, commandIndex) => pathCommand3d(command, `${name}.commands[${commandIndex}]`)) };
    case "tracePath": {
      assert(Array.isArray(candidate.segments) && candidate.segments.length >= 1 && candidate.segments.length <= 100_000, `${name}.segments must contain 1–100,000 cubic segments.`);
      const segments = candidate.segments.map((segment, segmentIndex) => {
        const segmentName = `${name}.segments[${segmentIndex}]`;
        object(segment, segmentName);
        return {
          start: point(segment.start, `${segmentName}.start`),
          control1: point(segment.control1, `${segmentName}.control1`),
          control2: point(segment.control2, `${segmentName}.control2`),
          end: point(segment.end, `${segmentName}.end`),
        };
      });
      assert(Array.isArray(candidate.frames) && candidate.frames.length >= 1 && candidate.frames.length <= 10_000, `${name}.frames must contain 1–10,000 frames.`);
      let previous = -1;
      const frames = candidate.frames.map((frame, frameIndex) => {
        const frameName = `${name}.frames[${frameIndex}]`;
        object(frame, frameName);
        const at = number(frame.at, `${frameName}.at`, 0, duration);
        assert(at >= previous, `${name}.frames must be ordered.`);
        previous = at;
        const start = integer(frame.start, `${frameName}.start`, 0, segments.length - 1);
        const count = integer(frame.count, `${frameName}.count`, 1, segments.length);
        assert(start + count <= segments.length, `${frameName} exceeds the segment list.`);
        return { at, start, count, closed: frame.closed === true };
      });
      return { ...node, segments, frames };
    }
    case "pathRef":
      return { ...node, source: id(candidate.source, `${name}.source`) };
    case "text":
      assert(["left", "center", "right"].includes(candidate.align ?? "center"), `${name}.align is invalid.`);
      assert(["normal", "bold"].includes(candidate.weight ?? "normal"), `${name}.weight is invalid.`);
      assert(["normal", "italic"].includes(candidate.slant ?? "normal"), `${name}.slant is invalid.`);
      return {
        ...node,
        text: text(candidate.text, `${name}.text`),
        fontSize: number(candidate.fontSize, `${name}.fontSize`, 0.01, 100),
        fontFamily: text(candidate.fontFamily ?? "Noto Sans", `${name}.fontFamily`, 120),
        align: candidate.align ?? "center",
        weight: candidate.weight ?? "normal",
        slant: candidate.slant ?? "normal",
      };
    case "markupText": {
      assert(["left", "center", "right"].includes(candidate.align ?? "center"), `${name}.align is invalid.`);
      const hasMarkup = typeof candidate.markup === "string";
      assert(
        hasMarkup !== (Array.isArray(candidate.spans) && candidate.spans.length > 0),
        `${name} must provide exactly one of markup or spans.`,
      );
      if (hasMarkup) assert(candidate.markup.length >= 1 && candidate.markup.length <= 64 * 1024, `${name}.markup must contain 1 byte–64 KiB.`);
      assert((candidate.spans ?? []).length <= 1_000, `${name}.spans must contain at most 1,000 spans.`);
      let totalLength = 0;
      const spans = (candidate.spans ?? []).map((span, spanIndex) => {
        const spanName = `${name}.spans[${spanIndex}]`;
        object(span, spanName);
        const allowedSpanKeys = new Set(["text", "color", "weight", "slant", "fontFamily", "fontScale", "rise", "letterSpacing", "background", "underline", "underlineColor", "strikethrough", "strikethroughColor"]);
        for (const key of Object.keys(span)) assert(allowedSpanKeys.has(key), `${spanName}.${key} is unsupported.`);
        const spanText = text(span.text, `${spanName}.text`);
        assert(["normal", "bold"].includes(span.weight ?? "normal"), `${spanName}.weight is invalid.`);
        assert(["normal", "italic"].includes(span.slant ?? "normal"), `${spanName}.slant is invalid.`);
        totalLength += spanText.length;
        return {
          text: spanText,
          ...(span.color == null ? {} : { color: color(span.color, `${spanName}.color`) }),
          weight: span.weight ?? "normal",
          slant: span.slant ?? "normal",
          ...(span.fontFamily == null ? {} : { fontFamily: text(span.fontFamily, `${spanName}.fontFamily`, 120) }),
          fontScale: number(span.fontScale ?? 1, `${spanName}.fontScale`, 0.01, 100),
          rise: number(span.rise ?? 0, `${spanName}.rise`, -100, 100),
          letterSpacing: number(span.letterSpacing ?? 0, `${spanName}.letterSpacing`, -100, 100),
          ...(span.background == null ? {} : { background: color(span.background, `${spanName}.background`) }),
          underline: ["none", "single", "double", "low", "error"].includes(span.underline ?? "none") ? (span.underline ?? "none") : (() => { throw new Error(`${spanName}.underline is invalid.`); })(),
          ...(span.underlineColor == null ? {} : { underlineColor: color(span.underlineColor, `${spanName}.underlineColor`) }),
          strikethrough: (() => {
            const value = span.strikethrough ?? false;
            assert(typeof value === "boolean", `${spanName}.strikethrough must be boolean.`);
            return value;
          })(),
          ...(span.strikethroughColor == null ? {} : { strikethroughColor: color(span.strikethroughColor, `${spanName}.strikethroughColor`) }),
        };
      });
      assert(totalLength <= 64 * 1024, `${name}.spans are limited to 64 KiB total.`);
      return {
        ...node,
        spans,
        ...(hasMarkup ? { markup: candidate.markup } : { markup: null }),
        fontSize: number(candidate.fontSize, `${name}.fontSize`, 0.01, 100),
        fontFamily: text(candidate.fontFamily ?? "Noto Sans", `${name}.fontFamily`, 120),
        align: candidate.align ?? "center",
      };
    }
    case "mathTex":
      return {
        ...node,
        tex: text(candidate.tex, `${name}.tex`),
        fontSize: number(candidate.fontSize, `${name}.fontSize`, 0.01, 100),
      };
    case "svg":
      return {
        ...node,
        svg: text(candidate.svg, `${name}.svg`, 4_000_000),
        height: number(candidate.height, `${name}.height`, 0.01, 100),
        preserveStyles: candidate.preserveStyles !== false,
      };
    case "image": {
      const pixelWidth = integer(candidate.pixelWidth, `${name}.pixelWidth`, 1, 8_192);
      const pixelHeight = integer(candidate.pixelHeight, `${name}.pixelHeight`, 1, 8_192);
      assert(
        pixelWidth * pixelHeight <= 16_777_216,
        `${name} exceeds 16,777,216 pixels.`,
      );
      const expectedBytes = pixelWidth * pixelHeight * 4;
      const expectedCharacters = Math.ceil(expectedBytes / 3) * 4;
      const pixelsValue = text(candidate.pixels, `${name}.pixels`, 90_000_000);
      assert(
        pixelsValue.length === expectedCharacters &&
          /^[A-Za-z0-9+/]+={0,2}$/.test(pixelsValue),
        `${name}.pixels must be base64 RGBA data matching its dimensions.`,
      );
      const corners = points(candidate.corners, `${name}.corners`, 4);
      assert(corners.length === 4, `${name}.corners must contain exactly four points.`);
      assert(
        ["nearest", "box", "bilinear", "hamming", "bicubic", "lanczos"].includes(
          candidate.resampling ?? "bicubic",
        ),
        `${name}.resampling is invalid.`,
      );
      return {
        ...node,
        pixels: pixelsValue,
        pixelWidth,
        pixelHeight,
        corners,
        resampling: candidate.resampling ?? "bicubic",
      };
    }
    case "pointCloud":
      assert(Array.isArray(candidate.points) && candidate.points.length >= 1 && candidate.points.length <= 100_000, `${name}.points must contain 1–100,000 marks.`);
      return {
        ...node,
        radius: number(candidate.radius ?? 0.08, `${name}.radius`, 0.001, 100),
        screenSpaceRadius: candidate.screenSpaceRadius === true,
        points: candidate.points.map((mark, markIndex) => {
          object(mark, `${name}.points[${markIndex}]`);
          return {
            x: number(mark.x, `${name}.points[${markIndex}].x`),
            y: number(mark.y, `${name}.points[${markIndex}].y`),
            ...(mark.color == null ? {} : { color: color(mark.color, `${name}.points[${markIndex}].color`) }),
            ...(mark.radius == null ? {} : { radius: number(mark.radius, `${name}.points[${markIndex}].radius`, 0.001, 100) }),
          };
        }),
      };
    case "surface": {
      assert(
        Array.isArray(candidate.vertices) &&
          candidate.vertices.length >= 3 &&
          candidate.vertices.length <= 1_000_000,
        `${name}.vertices must contain 3–1,000,000 points.`,
      );
      const vertices = candidate.vertices.map((vertex, vertexIndex) =>
        point3(vertex, `${name}.vertices[${vertexIndex}]`),
      );
      assert(
        Array.isArray(candidate.patches) &&
          candidate.patches.length >= 1 &&
          candidate.patches.length <= 2_000_000,
        `${name}.patches must contain 1–2,000,000 patches.`,
      );
      let cornerCount = 0;
      const patches = candidate.patches.map((patch, patchIndex) => {
        const patchName = `${name}.patches[${patchIndex}]`;
        assert(
          Array.isArray(patch) && patch.length >= 3 && patch.length <= 1_024,
          `${patchName} must contain 3–1,024 indices.`,
        );
        cornerCount += patch.length;
        return patch.map((index, indexPosition) =>
          integer(index, `${patchName}[${indexPosition}]`, 0, vertices.length - 1),
        );
      });
      const colors = (candidate.colors ?? []).map((value, colorIndex) =>
        color(value, `${name}.colors[${colorIndex}]`),
      );
      const strokeColors = (candidate.strokeColors ?? []).map((value, colorIndex) =>
        color(value, `${name}.strokeColors[${colorIndex}]`),
      );
      const strokeRadii = (candidate.strokeRadii ?? []).map((value, radiusIndex) =>
        number(value, `${name}.strokeRadii[${radiusIndex}]`, 0, 100),
      );
      assert(colors.length === cornerCount, `${name}.colors must match the flattened patch corners.`);
      assert(strokeColors.length === patches.length, `${name}.strokeColors must match the patch count.`);
      assert(strokeRadii.length === patches.length, `${name}.strokeRadii must match the patch count.`);
      return {
        ...node,
        vertices,
        patches,
        colors,
        strokeColors,
        strokeRadii,
        unlit: candidate.unlit === true,
        doubleSided: candidate.doubleSided === true,
      };
    }
    case "mesh": {
      assert(
        Array.isArray(candidate.vertices) &&
          candidate.vertices.length >= 3 &&
          candidate.vertices.length <= 1_000_000,
        `${name}.vertices must contain 3–1,000,000 points.`,
      );
      assert(
        Array.isArray(candidate.triangles) &&
          candidate.triangles.length >= 1 &&
          candidate.triangles.length <= 2_000_000,
        `${name}.triangles must contain 1–2,000,000 faces.`,
      );
      const vertices = candidate.vertices.map((vertex, vertexIndex) =>
        point3(vertex, `${name}.vertices[${vertexIndex}]`),
      );
      const triangles = candidate.triangles.map((triangle, triangleIndex) => {
        const triangleName = `${name}.triangles[${triangleIndex}]`;
        assert(Array.isArray(triangle) && triangle.length === 3, `${triangleName} must contain three indices.`);
        return triangle.map((index, indexPosition) =>
          integer(index, `${triangleName}[${indexPosition}]`, 0, vertices.length - 1),
        );
      });
      const colors = (candidate.colors ?? []).map((value, colorIndex) =>
        color(value, `${name}.colors[${colorIndex}]`),
      );
      assert(
        colors.length === 0 || colors.length === vertices.length,
        `${name}.colors must be empty or match the vertex count.`,
      );
      const normals = (candidate.normals ?? []).map((value, normalIndex) =>
        point3(value, `${name}.normals[${normalIndex}]`),
      );
      assert(
        normals.length === 0 || normals.length === vertices.length,
        `${name}.normals must be empty or match the vertex count.`,
      );
      const uvs = (candidate.uvs ?? []).map((value, uvIndex) =>
        point(value, `${name}.uvs[${uvIndex}]`),
      );
      const texturePixels = candidate.texturePixels ?? "";
      assert(
        typeof texturePixels === "string" && texturePixels.length <= 90_000_000,
        `${name}.texturePixels must be a base64 string.`,
      );
      const textureWidth = integer(candidate.textureWidth ?? 0, `${name}.textureWidth`, 0, 8_192);
      const textureHeight = integer(candidate.textureHeight ?? 0, `${name}.textureHeight`, 0, 8_192);
      const darkTexturePixels = candidate.darkTexturePixels ?? "";
      assert(
        typeof darkTexturePixels === "string" && darkTexturePixels.length <= 90_000_000,
        `${name}.darkTexturePixels must be a base64 string.`,
      );
      const darkTextureWidth = integer(candidate.darkTextureWidth ?? 0, `${name}.darkTextureWidth`, 0, 8_192);
      const darkTextureHeight = integer(candidate.darkTextureHeight ?? 0, `${name}.darkTextureHeight`, 0, 8_192);
      const textureResampling = candidate.textureResampling ?? "bicubic";
      assert(
        ["nearest", "box", "bilinear", "hamming", "bicubic", "lanczos"].includes(
          textureResampling,
        ),
        `${name}.textureResampling is invalid.`,
      );
      if (texturePixels) {
        assert(uvs.length === vertices.length, `${name}.uvs must match the textured vertex count.`);
        assert(textureWidth > 0 && textureHeight > 0, `${name} texture dimensions must be nonzero.`);
        const expectedCharacters = Math.ceil((textureWidth * textureHeight * 4) / 3) * 4;
        assert(
          texturePixels.length === expectedCharacters &&
            /^[A-Za-z0-9+/]+={0,2}$/.test(texturePixels),
          `${name}.texturePixels must be base64 RGBA data matching its dimensions.`,
        );
        if (darkTexturePixels) {
          assert(darkTextureWidth > 0 && darkTextureHeight > 0, `${name} dark texture dimensions must be nonzero.`);
          const expectedDarkCharacters = Math.ceil((darkTextureWidth * darkTextureHeight * 4) / 3) * 4;
          assert(
            darkTexturePixels.length === expectedDarkCharacters &&
              /^[A-Za-z0-9+/]+={0,2}$/.test(darkTexturePixels),
            `${name}.darkTexturePixels must be base64 RGBA data matching its dimensions.`,
          );
        } else {
          assert(darkTextureWidth === 0 && darkTextureHeight === 0, `${name} dark texture dimensions require dark texture pixels.`);
        }
      } else {
        assert(uvs.length === 0, `${name}.uvs require texture pixels.`);
        assert(textureWidth === 0 && textureHeight === 0, `${name} texture dimensions require texture pixels.`);
        assert(!darkTexturePixels && darkTextureWidth === 0 && darkTextureHeight === 0, `${name} dark texture requires a light texture.`);
      }
      return {
        ...node,
        vertices,
        triangles,
        colors,
        normals,
        uvs,
        texturePixels,
        textureWidth,
        textureHeight,
        darkTexturePixels,
        darkTextureWidth,
        darkTextureHeight,
        textureResampling,
        gloss: number(candidate.gloss ?? 0, `${name}.gloss`, 0, 1),
        shadow: number(candidate.shadow ?? 0, `${name}.shadow`, 0, 1),
        lightPosition: point3(
          candidate.lightPosition ?? [-10, 10, 10],
          `${name}.lightPosition`,
        ),
        unlit: candidate.unlit === true,
        doubleSided: candidate.doubleSided === true,
      };
    }
    case "customShaderMesh": {
      const vertexWgsl = text(candidate.vertexWgsl, `${name}.vertexWgsl`, 1_000_000);
      const fragmentWgsl = text(candidate.fragmentWgsl, `${name}.fragmentWgsl`, 1_000_000);
      const vertexStride = integer(candidate.vertexStride, `${name}.vertexStride`, 4, 2_048);
      assert(vertexStride % 4 === 0, `${name}.vertexStride must be four-byte aligned.`);
      assert(
        Array.isArray(candidate.vertexData) &&
          candidate.vertexData.length >= 1 &&
          candidate.vertexData.length <= 16_000_000,
        `${name}.vertexData must contain 1–16,000,000 floats.`,
      );
      const vertexData = candidate.vertexData.map((value, valueIndex) =>
        number(value, `${name}.vertexData[${valueIndex}]`),
      );
      assert(
        (vertexData.length * 4) % vertexStride === 0,
        `${name}.vertexData must contain complete vertices.`,
      );
      const vertexCount = (vertexData.length * 4) / vertexStride;
      assert(
        Array.isArray(candidate.attributes) &&
          candidate.attributes.length >= 1 &&
          candidate.attributes.length <= 16,
        `${name}.attributes must contain 1–16 entries.`,
      );
      const attributeFormats = new Map([
        ["float32", 4],
        ["float32x2", 8],
        ["float32x3", 12],
        ["float32x4", 16],
        ["sint32", 4],
        ["sint32x2", 8],
        ["sint32x3", 12],
        ["sint32x4", 16],
        ["uint32", 4],
        ["uint32x2", 8],
        ["uint32x3", 12],
        ["uint32x4", 16],
      ]);
      const locations = new Set();
      const attributes = candidate.attributes.map((attribute, attributeIndex) => {
        const attributeName = `${name}.attributes[${attributeIndex}]`;
        object(attribute, attributeName);
        const location = integer(attribute.location, `${attributeName}.location`, 0, 15);
        assert(!locations.has(location), `${name}.attribute locations must be unique.`);
        locations.add(location);
        const format = attribute.format;
        assert(attributeFormats.has(format), `${attributeName}.format is unsupported.`);
        const offset = integer(attribute.offset, `${attributeName}.offset`, 0, vertexStride - 1);
        assert(offset % 4 === 0, `${attributeName}.offset must be four-byte aligned.`);
        assert(
          offset + attributeFormats.get(format) <= vertexStride,
          `${attributeName} exceeds the vertex stride.`,
        );
        return {
          name: text(attribute.name, `${attributeName}.name`, 80),
          location,
          offset,
          format,
        };
      });
      const indices = (candidate.indices ?? []).map((value, index) =>
        integer(value, `${name}.indices[${index}]`, 0, vertexCount - 1),
      );
      assert(indices.length <= 16_000_000, `${name}.indices exceed the safety limit.`);
      const primitive = candidate.primitive ?? "triangle-list";
      assert(
        ["point-list", "line-list", "line-strip", "triangle-list", "triangle-strip"].includes(primitive),
        `${name}.primitive is unsupported.`,
      );
      const bindings = new Set();
      const valueCounts = new Map([
        ["float", 1],
        ["vec2", 2],
        ["vec3", 3],
        ["vec4", 4],
        ["mat3", 9],
        ["mat4", 16],
        ["int", 1],
        ["ivec2", 2],
        ["ivec3", 3],
        ["ivec4", 4],
        ["uint", 1],
        ["uvec2", 2],
        ["uvec3", 3],
        ["uvec4", 4],
        ["bool", 1],
        ["bvec2", 2],
        ["bvec3", 3],
        ["bvec4", 4],
      ]);
      const uniforms = (candidate.uniforms ?? []).map((uniform, uniformIndex) => {
        const uniformName = `${name}.uniforms[${uniformIndex}]`;
        object(uniform, uniformName);
        const binding = integer(uniform.binding, `${uniformName}.binding`, 0, 999);
        assert(!bindings.has(binding), `${name}.uniform bindings must be unique.`);
        bindings.add(binding);
        const uniformType = uniform.type;
        const arrayLength = integer(
          uniform.arrayLength ?? 1,
          `${uniformName}.arrayLength`,
          1,
          256,
        );
        assert(
          uniformType === "sampler2D" || valueCounts.has(uniformType),
          `${uniformName}.type is unsupported.`,
        );
        if (uniformType === "sampler2D") {
          assert(arrayLength === 1, `${uniformName} sampler2D arrays are unsupported.`);
          const samplerBinding = integer(
            uniform.samplerBinding,
            `${uniformName}.samplerBinding`,
            0,
            999,
          );
          assert(!bindings.has(samplerBinding), `${name}.uniform bindings must be unique.`);
          bindings.add(samplerBinding);
          const textureWidth = integer(uniform.textureWidth, `${uniformName}.textureWidth`, 1, 8_192);
          const textureHeight = integer(uniform.textureHeight, `${uniformName}.textureHeight`, 1, 8_192);
          assert(
            textureWidth * textureHeight <= 16_777_216,
            `${uniformName} texture exceeds the safety limit.`,
          );
          const texturePixels = text(uniform.texturePixels, `${uniformName}.texturePixels`, 90_000_000);
          const expectedCharacters = Math.ceil((textureWidth * textureHeight * 4) / 3) * 4;
          assert(
            texturePixels.length === expectedCharacters &&
              /^[A-Za-z0-9+/]+={0,2}$/.test(texturePixels),
            `${uniformName}.texturePixels must match its dimensions.`,
          );
          return {
            name: text(uniform.name, `${uniformName}.name`, 80),
            binding,
            samplerBinding,
            type: uniformType,
            arrayLength,
            values: [],
            texturePixels,
            textureWidth,
            textureHeight,
          };
        }
        assert(
          Array.isArray(uniform.values) &&
            uniform.values.length === valueCounts.get(uniformType) * arrayLength,
          `${uniformName}.values has the wrong length.`,
        );
        return {
          name: text(uniform.name, `${uniformName}.name`, 80),
          binding,
          type: uniformType,
          arrayLength,
          values: uniform.values.map((value, valueIndex) =>
            number(value, `${uniformName}.values[${valueIndex}]`),
          ),
          texturePixels: "",
          textureWidth: 0,
          textureHeight: 0,
        };
      });
      return {
        ...node,
        vertexWgsl,
        fragmentWgsl,
        attributes,
        vertexStride,
        vertexData,
        indices,
        primitive,
        uniforms,
        depthTest: candidate.depthTest === true,
      };
    }
    default:
      throw new Error(`${name}.type is unsupported.`);
  }
}

function easing(value, name) {
  const next = value ?? "linear";
  assert(EASINGS.has(next), `${name} is invalid.`);
  return next;
}

function trackValue(value, property, name) {
  if (COLOR_PROPERTIES.has(property)) return color(value, name);
  if (GRADIENT_PROPERTIES.has(property)) return linearGradient(value, name);
  if (property === "points") return points(value, name);
  if (property === "vertices" || property === "normals") {
    assert(
      Array.isArray(value) && value.length >= 3 && value.length <= 1_000_000,
      `${name} must contain 3–1,000,000 3D points.`,
    );
    return value.map((vertex, index) => point3(vertex, `${name}[${index}]`));
  }
  if (property === "colors" || property === "surfaceColors") {
    assert(
      Array.isArray(value) && value.length >= 1 && value.length <= 1_000_000,
      `${name} must contain 1–1,000,000 colors.`,
    );
    return value.map((value, index) => color(value, `${name}[${index}]`));
  }
  if (property === "strokeRadii") {
    assert(
      Array.isArray(value) && value.length >= 1 && value.length <= 2_000_000,
      `${name} must contain 1–2,000,000 stroke radii.`,
    );
    return value.map((radius, index) => number(radius, `${name}[${index}]`, 0, 100));
  }
  if (property === "lightPosition") {
    assert(Array.isArray(value) && value.length === 1, `${name} must contain one 3D point.`);
    return [point3(value?.[0], `${name}[0]`)];
  }
  if (["billboardAnchor", "camera3dPosition", "camera3dTarget", "camera3dUp", "camera3dOrbit"].includes(property)) {
    return point3(value, name);
  }
  if (property === "camera3d") {
    assert(Array.isArray(value) && value.length >= 2, `${name} must contain a field mask and camera values.`);
    const state = value.map((entry, index) => number(entry, `${name}[${index}]`));
    const mask = state[0];
    assert(Number.isInteger(mask) && mask >= 1 && mask <= 15, `${name} field mask is invalid.`);
    const expected = 1 + ((mask & 1) ? 3 : 0) + ((mask & 2) ? 3 : 0) + ((mask & 4) ? 3 : 0) + ((mask & 8) ? 1 : 0);
    assert(state.length === expected, `${name} does not match its field mask.`);
    if (mask & 8) assert(state.at(-1) > 0.05 && state.at(-1) < Math.PI, `${name} fovY is invalid.`);
    return state;
  }
  if (property === "commands") {
    assert(
      Array.isArray(value) && value.length >= 1 && value.length <= 50_000,
      `${name} must contain 1–50,000 path commands.`,
    );
    return value.map((command, index) => pathCommand(command, `${name}[${index}]`));
  }
  if (property === "transform2d") {
    assert(Array.isArray(value) && [5, 6].includes(value.length), `${name} must be [x, y, rotation, scaleX, scaleY] with optional strokeWidth.`);
    return value.map((entry, index) => number(entry, `${name}[${index}]`));
  }
  if (property === "affine2d") {
    assert(Array.isArray(value) && [6, 7].includes(value.length), `${name} must be [a, b, c, d, tx, ty] with optional strokeWidth.`);
    return value.map((entry, index) => number(entry, `${name}[${index}]`));
  }
  if (property === "drawRange") {
    assert(Array.isArray(value) && value.length === 2, `${name} must be [start, end].`);
    const range = value.map((entry, index) => number(entry, `${name}[${index}]`, 0, 1));
    assert(range[0] <= range[1], `${name} start must not exceed end.`);
    return range;
  }
  if (property === "pathData" || property === "shaderVertexData" || property === "shaderUniformValues") {
    assert(
      Array.isArray(value) && value.length >= 1 && value.length <= 16_000_000,
      `${name} must contain 1–16,000,000 floats.`,
    );
    return value.map((entry, index) => number(entry, `${name}[${index}]`));
  }
  return number(value, name);
}

function validateTrack(candidate, index, duration, nodeIds) {
  const name = `tracks[${index}]`;
  object(candidate, name);
  const target = candidate.target === "__camera__" ? "__camera__" : id(candidate.target, `${name}.target`);
  assert(target === "__camera__" || nodeIds.has(target), `${name}.target does not exist.`);
  assert(PROPERTIES.has(candidate.property), `${name}.property is unsupported.`);
  assert(!CAMERA_PROPERTIES.has(candidate.property) || target === "__camera__", `${name} camera properties must target __camera__.`);
  const hasKeyframes = Array.isArray(candidate.keyframes) && candidate.keyframes.length > 0;
  const hasReference = candidate.keyframesFrom != null;
  assert(hasKeyframes !== hasReference, `${name} must declare exactly one of keyframes or keyframesFrom.`);
  if (hasReference) {
    const keyframesFrom = candidate.keyframesFrom === "__camera__"
      ? "__camera__"
      : id(candidate.keyframesFrom, `${name}.keyframesFrom`);
    assert(keyframesFrom === "__camera__" || nodeIds.has(keyframesFrom), `${name}.keyframesFrom does not exist.`);
    return { target, property: candidate.property, keyframesFrom };
  }
  assert(candidate.keyframes.length <= 10_000, `${name}.keyframes must contain 1–10,000 values.`);
  let previous = -1;
  const keyframes = candidate.keyframes.map((keyframe, keyframeIndex) => {
    object(keyframe, `${name}.keyframes[${keyframeIndex}]`);
    const at = number(keyframe.at, `${name}.keyframes[${keyframeIndex}].at`, 0, duration);
    assert(at >= previous, `${name}.keyframes must be ordered.`);
    previous = at;
    return {
      at,
      value: trackValue(keyframe.value, candidate.property, `${name}.keyframes[${keyframeIndex}].value`),
      easing: easing(keyframe.easing, `${name}.keyframes[${keyframeIndex}].easing`),
    };
  });
  if (candidate.property === "camera3d") {
    assert(keyframes.every((keyframe) => keyframe.value[0] === keyframes[0].value[0]), `${name} camera3d field mask must remain constant.`);
  }
  return { target, property: candidate.property, keyframes };
}

function validateExpression(candidate, signalIds, name = "expression", depth = 0) {
  object(candidate, name);
  assert(depth <= 32, `${name} is too deeply nested.`);
  assert(EXPR_OPS.has(candidate.op), `${name}.op is unsupported.`);
  const next = depth + 1;
  if (candidate.op === "constant") return { op: "constant", value: number(candidate.value, `${name}.value`) };
  if (candidate.op === "time") return { op: "time" };
  if (candidate.op === "signal") {
    const signalId = id(candidate.id, `${name}.id`);
    assert(signalIds.has(signalId), `${name} references missing signal ${signalId}.`);
    return { op: "signal", id: signalId };
  }
  if (["add", "multiply", "min", "max"].includes(candidate.op)) {
    assert(Array.isArray(candidate.args) && candidate.args.length >= 1 && candidate.args.length <= 32, `${name}.args must contain 1–32 expressions.`);
    return { op: candidate.op, args: candidate.args.map((arg, index) => validateExpression(arg, signalIds, `${name}.args[${index}]`, next)) };
  }
  if (candidate.op === "subtract" || candidate.op === "divide") {
    return {
      op: candidate.op,
      left: validateExpression(candidate.left, signalIds, `${name}.left`, next),
      right: validateExpression(candidate.right, signalIds, `${name}.right`, next),
    };
  }
  if (["sin", "cos", "abs"].includes(candidate.op)) {
    return { op: candidate.op, value: validateExpression(candidate.value, signalIds, `${name}.value`, next) };
  }
  if (candidate.op === "clamp") {
    return {
      op: "clamp",
      value: validateExpression(candidate.value, signalIds, `${name}.value`, next),
      min: number(candidate.min, `${name}.min`),
      max: number(candidate.max, `${name}.max`),
    };
  }
  return {
    op: "lerp",
    from: validateExpression(candidate.from, signalIds, `${name}.from`, next),
    to: validateExpression(candidate.to, signalIds, `${name}.to`, next),
    amount: validateExpression(candidate.amount, signalIds, `${name}.amount`, next),
  };
}

export function validateScene(candidate) {
  object(candidate, "scene");
  assert(candidate.version === 2, "Scene version must be 2.");
  const titleValue = text(candidate.title, "title", 120);
  const width = number(candidate.width ?? 16, "width", 4, 40);
  const height = number(candidate.height ?? 9, "height", 2.25, 22.5);
  const pixelWidth = integer(candidate.pixelWidth ?? 1280, "pixelWidth", 1, 8_192);
  const pixelHeight = integer(candidate.pixelHeight ?? 720, "pixelHeight", 1, 8_192);
  const duration = number(candidate.duration, "duration", 0.25, 60);
  const fps = integer(candidate.fps ?? 60, "fps", 1, 120);
  const background = color(candidate.background ?? "#0f172a", "background");
  object(candidate.camera ?? {}, "camera");
  const camera = {
    x: number(candidate.camera?.x ?? 0, "camera.x"),
    y: number(candidate.camera?.y ?? 0, "camera.y"),
    zoom: number(candidate.camera?.zoom ?? 1, "camera.zoom", 0.001, 1_000),
    rotation: number(candidate.camera?.rotation ?? 0, "camera.rotation", -1_000, 1_000),
  };
  object(candidate.camera3d ?? {}, "camera3d");
  const camera3d = {
    position: point3(candidate.camera3d?.position ?? [0, 0, 8], "camera3d.position"),
    target: point3(candidate.camera3d?.target ?? [0, 0, 0], "camera3d.target"),
    up: point3(candidate.camera3d?.up ?? [0, 1, 0], "camera3d.up"),
    fovY: number(candidate.camera3d?.fovY ?? Math.PI / 4, "camera3d.fovY", 0.05, Math.PI - 0.0001),
    near: number(candidate.camera3d?.near ?? 0.1, "camera3d.near", 0.0001),
    far: number(candidate.camera3d?.far ?? 100, "camera3d.far", 0.0002),
    ambient: number(candidate.camera3d?.ambient ?? 0.28, "camera3d.ambient", 0, 1),
    lightDirection: point3(candidate.camera3d?.lightDirection ?? [-0.4, 0.7, 1], "camera3d.lightDirection"),
  };
  assert(camera3d.far > camera3d.near, "camera3d.far must exceed near.");
  assert(Array.isArray(candidate.nodes) && candidate.nodes.length >= 1 && candidate.nodes.length <= 5_000, "nodes must contain 1–5,000 items.");
  const nodes = candidate.nodes.map((node, index) => validateNode(node, index, duration));
  const nodeIds = new Set();
  for (const node of nodes) {
    assert(!nodeIds.has(node.id), `Duplicate node id: ${node.id}.`);
    nodeIds.add(node.id);
  }
  for (const node of nodes) {
    assert(node.parent == null || (nodeIds.has(node.parent) && node.parent !== node.id), `Node ${node.id} has an invalid parent.`);
    if (node.type === "pathRef") {
      const source = nodes.find((candidateNode) => candidateNode.id === node.source);
      assert(source?.type === "path", `Node ${node.id} must reference a concrete path node.`);
    }
  }

  assert(Array.isArray(candidate.correspondences ?? []), "correspondences must be an array.");
  assert(
    (candidate.correspondences ?? []).length <= 20_000,
    "correspondences must contain at most 20,000 entries.",
  );
  const correspondenceIds = new Set();
  const correspondences = (candidate.correspondences ?? []).map((correspondence, index) => {
    const name = `correspondences[${index}]`;
    object(correspondence, name);
    for (const key of Object.keys(correspondence)) {
      assert(CORRESPONDENCE_KEYS.has(key), `${name}.${key} is unsupported.`);
    }
    const correspondenceId = id(correspondence.id, `${name}.id`);
    assert(!correspondenceIds.has(correspondenceId), `Duplicate correspondence id: ${correspondenceId}.`);
    correspondenceIds.add(correspondenceId);
    assert(CORRESPONDENCE_KINDS.has(correspondence.kind), `${name}.kind is invalid.`);
    assert(CORRESPONDENCE_MODES.has(correspondence.mode), `${name}.mode is invalid.`);
    assert(Array.isArray(correspondence.keys), `${name}.keys must be an array.`);
    assert(Array.isArray(correspondence.targetKeys ?? []), `${name}.targetKeys must be an array.`);
    assert(correspondence.keys.length <= 1_000, `${name}.keys must contain at most 1,000 entries.`);
    assert((correspondence.targetKeys ?? []).length <= 1_000, `${name}.targetKeys must contain at most 1,000 entries.`);
    const semanticKey = (value, keyName) => {
      assert(typeof value === "string" && value.length <= 4_000, `${keyName} must contain at most 4,000 characters.`);
      return value;
    };
    const keys = correspondence.keys.map((key, keyIndex) =>
      semanticKey(key, `${name}.keys[${keyIndex}]`),
    );
    const targetKeys = (correspondence.targetKeys ?? []).map((key, keyIndex) =>
      semanticKey(key, `${name}.targetKeys[${keyIndex}]`),
    );
    assert(keys.length > 0 || targetKeys.length > 0, `${name} must contain at least one semantic key.`);

    const nodeSet = (value, property) => {
      const nodeName = `${name}.${property}`;
      assert(Array.isArray(value ?? []), `${nodeName} must be an array.`);
      assert((value ?? []).length <= 5_000, `${nodeName} must contain at most 5,000 entries.`);
      const result = (value ?? []).map((nodeId, nodeIndex) => id(nodeId, `${nodeName}[${nodeIndex}]`));
      assert(new Set(result).size === result.length, `${nodeName} must not contain duplicate nodes.`);
      assert(result.every((nodeId) => nodeIds.has(nodeId)), `${nodeName} references a missing node.`);
      return result;
    };
    const sourceNodes = nodeSet(correspondence.sourceNodes, "sourceNodes");
    const targetNodes = nodeSet(correspondence.targetNodes, "targetNodes");
    assert(sourceNodes.length > 0 || targetNodes.length > 0, `${name} must reference at least one node.`);
    if (correspondence.mode === "transform" || correspondence.mode === "keyMapped") {
      assert(sourceNodes.length > 0 && targetNodes.length > 0, `${name}.${correspondence.mode} requires source and target nodes.`);
    }
    const start = number(correspondence.start, `${name}.start`, 0, duration);
    const end = number(correspondence.end, `${name}.end`, 0, duration + 0.0001);
    assert(end > start, `${name}.end must be after start.`);
    const pathArc = correspondence.pathArc ?? 0;
    assert(Number.isFinite(pathArc), `${name}.pathArc must be a finite number.`);
    return {
      id: correspondenceId,
      kind: correspondence.kind,
      mode: correspondence.mode,
      keys,
      targetKeys,
      sourceNodes,
      targetNodes,
      start,
      end,
      pathArc,
    };
  });

  const signals = (candidate.signals ?? []).map((signal, index) => {
    object(signal, `signals[${index}]`);
    const signalId = id(signal.id, `signals[${index}].id`);
    assert(Array.isArray(signal.keyframes) && signal.keyframes.length >= 1 && signal.keyframes.length <= 10_000, `signals[${index}].keyframes is invalid.`);
    let previous = -1;
    const keyframes = signal.keyframes.map((keyframe, keyframeIndex) => {
      object(keyframe, `signals[${index}].keyframes[${keyframeIndex}]`);
      const at = number(keyframe.at, `signals[${index}].keyframes[${keyframeIndex}].at`, 0, duration);
      assert(at >= previous, `signals[${index}].keyframes must be ordered.`);
      previous = at;
      return {
        at,
        value: number(keyframe.value, `signals[${index}].keyframes[${keyframeIndex}].value`),
        easing: easing(keyframe.easing, `signals[${index}].keyframes[${keyframeIndex}].easing`),
      };
    });
    return { id: signalId, keyframes };
  });
  const signalIds = new Set(signals.map((signal) => signal.id));
  assert(signalIds.size === signals.length, "Signal ids must be unique.");
  const controls = (candidate.controls ?? []).map((control, index) => {
    const name = `controls[${index}]`;
    object(control, name);
    const controlId = id(control.id, `${name}.id`);
    const signal = id(control.signal, `${name}.signal`);
    assert(signalIds.has(signal), `${name}.signal references missing signal ${signal}.`);
    const min = number(control.min, `${name}.min`);
    const max = number(control.max, `${name}.max`);
    assert(min < max, `${name}.min must be less than max.`);
    const step = number(control.step, `${name}.step`, Number.MIN_VALUE);
    const defaultValue = number(control.default, `${name}.default`, min, max);
    const timeline = control.timeline === true;
    if (timeline) {
      const timelineSignal = signals.find((candidateSignal) => candidateSignal.id === signal);
      const values = timelineSignal.keyframes.map((keyframe) => keyframe.value);
      const nondecreasing = values.every((value, valueIndex) => valueIndex === 0 || values[valueIndex - 1] <= value);
      const nonincreasing = values.every((value, valueIndex) => valueIndex === 0 || values[valueIndex - 1] >= value);
      assert(
        timelineSignal.keyframes.every((keyframe) => keyframe.easing === "linear") &&
          (nondecreasing || nonincreasing) &&
          Math.max(...values) - Math.min(...values) > Number.EPSILON &&
          Math.abs(Math.min(...values) - min) <= 0.0001 &&
          Math.abs(Math.max(...values) - max) <= 0.0001,
        `${name} timeline signal must be linear, monotonic, and span its control range.`,
      );
    }
    return {
      id: controlId,
      label: text(control.label, `${name}.label`, 80),
      signal,
      min,
      max,
      step,
      default: defaultValue,
      timeline,
    };
  });
  assert(
    new Set(controls.map((control) => control.id)).size === controls.length,
    "Control ids must be unique.",
  );
  assert(
    controls.filter((control) => control.timeline).length <= 1,
    "A scene may declare at most one timeline control.",
  );
  const tracks = (candidate.tracks ?? []).map((track, index) => validateTrack(track, index, duration, nodeIds));
  for (const track of tracks) {
    if (track.keyframesFrom == null) continue;
    const sources = tracks.filter(
      (candidateTrack) =>
        candidateTrack.target === track.keyframesFrom &&
        candidateTrack.property === track.property,
    );
    assert(sources.length === 1, `Track ${track.target} references missing or ambiguous ${track.property} keyframes.`);
    assert(Array.isArray(sources[0].keyframes), `Track ${track.target} must reference concrete keyframes.`);
  }
  for (const track of tracks) {
    if (track.property !== "transform2d" && track.property !== "affine2d") continue;
    const keyframes = track.keyframes ?? tracks.find(
      (candidateTrack) =>
        candidateTrack.target === track.keyframesFrom &&
        candidateTrack.property === track.property,
    ).keyframes;
    assert(
      keyframes.every((keyframe) => keyframe.value.length === keyframes[0].value.length),
      `${track.property} keyframes must use one layout.`,
    );
  }
  for (const track of tracks) {
    if (track.property !== "billboardAnchor") continue;
    const node = nodes.find((candidateNode) => candidateNode.id === track.target);
    assert(node?.type === "billboard", "billboardAnchor must target a billboard node.");
  }
  for (const track of tracks) {
    if (track.property !== "pathData") continue;
    const node = nodes.find((candidateNode) => candidateNode.id === track.target);
    assert(node?.type === "path", "pathData must target a concrete path.");
    const keyframes = track.keyframes ?? tracks.find(
      (candidateTrack) =>
        candidateTrack.target === track.keyframesFrom &&
        candidateTrack.property === track.property,
    ).keyframes;
    const expected = pathCommandDataLength(node.commands);
    assert(
      keyframes.every((keyframe) => keyframe.value.length === expected),
      "pathData keyframes must match the target path topology.",
    );
  }
  for (const track of tracks) {
    if (!["vertices", "surfaceColors", "strokeRadii"].includes(track.property)) continue;
    const node = nodes.find((candidateNode) => candidateNode.id === track.target);
    if (node?.type !== "surface") {
      assert(
        !["surfaceColors", "strokeRadii"].includes(track.property),
        `${track.property} must target a compact surface.`,
      );
      continue;
    }
    const keyframes = track.keyframes ?? tracks.find(
      (candidateTrack) =>
        candidateTrack.target === track.keyframesFrom &&
        candidateTrack.property === track.property,
    ).keyframes;
    const expected = track.property === "vertices"
      ? node.vertices.length
      : track.property === "surfaceColors"
        ? node.colors.length + node.strokeColors.length
        : node.strokeRadii.length;
    assert(
      keyframes.every((keyframe) => keyframe.value.length === expected),
      `${track.property} keyframes must match the target surface topology.`,
    );
  }
  for (const track of tracks) {
    if (!["shaderVertexData", "shaderUniformValues"].includes(track.property)) continue;
    const node = nodes.find((candidateNode) => candidateNode.id === track.target);
    assert(node?.type === "customShaderMesh", `${track.property} must target a custom shader mesh.`);
    const expected =
      track.property === "shaderVertexData"
        ? node.vertexData.length
        : node.uniforms.reduce((total, uniform) => total + uniform.values.length, 0);
    const keyframes = track.keyframes ?? tracks.find(
      (candidateTrack) =>
        candidateTrack.target === track.keyframesFrom &&
        candidateTrack.property === track.property,
    ).keyframes;
    assert(
      keyframes.every((keyframe) => keyframe.value.length === expected),
      `${track.property} keyframes must match the target layout.`,
    );
  }
  const bindings = (candidate.bindings ?? []).map((binding, index) => {
    object(binding, `bindings[${index}]`);
    const target = binding.target === "__camera__" ? "__camera__" : id(binding.target, `bindings[${index}].target`);
    assert(target === "__camera__" || nodeIds.has(target), `bindings[${index}].target does not exist.`);
    assert(
      PROPERTIES.has(binding.property) && !NON_NUMERIC_PROPERTIES.has(binding.property),
      `bindings[${index}].property must be numeric.`,
    );
    return {
      target,
      property: binding.property,
      expression: validateExpression(binding.expression, signalIds, `bindings[${index}].expression`),
    };
  });
  assert(Array.isArray(candidate.audio ?? []), "audio must be an array.");
  assert((candidate.audio ?? []).length <= 64, "audio must contain at most 64 clips.");
  let encodedAudioBytes = 0;
  const audio = (candidate.audio ?? []).map((clip, index) => {
    const name = `audio[${index}]`;
    object(clip, name);
    const clipId = id(clip.id, `${name}.id`);
    assert(typeof clip.data === "string" && clip.data.length > 0, `${name}.data is required.`);
    assert(clip.data.length <= 90_000_000 && BASE64.test(clip.data), `${name}.data must be valid base64.`);
    encodedAudioBytes += Math.floor((clip.data.length * 3) / 4);
    assert(encodedAudioBytes <= 64 * 1024 * 1024, "Scene audio exceeds the 64 MiB safety limit.");
    assert(AUDIO_MIME_TYPES.has(clip.mimeType), `${name}.mimeType is unsupported.`);
    return {
      id: clipId,
      data: clip.data,
      mimeType: clip.mimeType,
      startTime: number(clip.startTime, `${name}.startTime`, 0, duration),
      gainDb: number(clip.gainDb ?? 0, `${name}.gainDb`, -120, 60),
    };
  });
  assert(new Set(audio.map((clip) => clip.id)).size === audio.length, "Audio ids must be unique.");
  assert(Array.isArray(candidate.captions ?? []), "captions must be an array.");
  assert((candidate.captions ?? []).length <= 1_000, "captions must contain at most 1,000 entries.");
  const captions = (candidate.captions ?? []).map((caption, index) => {
    const name = `captions[${index}]`;
    object(caption, name);
    const start = number(caption.start, `${name}.start`, 0, duration);
    const end = number(caption.end, `${name}.end`, 0, duration + 0.0001);
    assert(end > start, `${name}.end must be after start.`);
    return { text: text(caption.text, `${name}.text`), start, end };
  });
  return {
    version: 2,
    title: titleValue,
    width,
    height,
    pixelWidth,
    pixelHeight,
    duration,
    fps,
    background,
    camera,
    camera3d,
    nodes,
    tracks,
    correspondences,
    signals,
    bindings,
    controls,
    audio,
    captions,
  };
}

export function parseSceneCode(rawCode) {
  assert(typeof rawCode === "string" && rawCode.trim(), "Animation code is empty.");
  let code = rawCode.trim();
  code = code.replace(/^```(?:javascript|js|json|scene)?\s*/i, "").replace(/\s*```$/i, "").trim();
  const match = code.match(/^scene\s*\(([\s\S]*)\)\s*;?\s*$/);
  assert(match, "Code must be exactly one scene({...}); call.");
  let parsed;
  try {
    parsed = JSON.parse(match[1]);
  } catch (error) {
    throw new Error(`Scene JSON is invalid: ${error.message}`);
  }
  return validateScene(parsed);
}

export function formatSceneCode(scene) {
  return `scene(${JSON.stringify(validateScene(scene), null, 2)});`;
}

export const DEFAULT_SCENE = Object.freeze({
  version: 2,
  title: "General engine proof",
  width: 16,
  height: 9,
  pixelWidth: 1280,
  pixelHeight: 720,
  duration: 6,
  fps: 60,
  background: "#0b1220",
  camera: { x: 0, y: 0, zoom: 1, rotation: 0 },
  nodes: [
    { id: "title", type: "text", text: "GENERAL RUST SCENE ENGINE", fontSize: 0.55, transform: { y: 3.35 }, style: { fill: "#e2e8f0", stroke: null } },
    { id: "ring", type: "circle", radius: 1.2, transform: { x: -4, y: 0.2 }, style: { fill: "#2563eb55", stroke: "#60a5fa", strokeWidth: 0.09 } },
    { id: "square", type: "rect", width: 2, height: 2, cornerRadius: 0.24, style: { fill: "#f9731666", stroke: "#fb923c", strokeWidth: 0.09 } },
    { id: "triangle", type: "polyline", points: [[-1.1, -0.9], [1.1, -0.9], [0, 1.1]], closed: true, transform: { x: 4, y: 0.2 }, style: { fill: "#22c55e55", stroke: "#4ade80", strokeWidth: 0.09 } },
    { id: "baseline", type: "line", from: [-6.5, -2.2], to: [6.5, -2.2], style: { stroke: "#475569", strokeWidth: 0.035 } },
    { id: "caption", type: "text", text: "paths  groups  tracks  signals  camera", fontSize: 0.38, transform: { y: -3.1 }, style: { fill: "#94a3b8", stroke: null } },
  ],
  tracks: [
    { target: "ring", property: "x", keyframes: [{ at: 0, value: -4 }, { at: 3, value: -2.8, easing: "smooth" }, { at: 6, value: -4, easing: "smooth" }] },
    { target: "ring", property: "opacity", keyframes: [{ at: 0, value: 0 }, { at: 1, value: 1, easing: "smooth" }, { at: 6, value: 1 }] },
    { target: "square", property: "rotation", keyframes: [{ at: 0, value: 0 }, { at: 6, value: 6.28318, easing: "smooth" }] },
    { target: "triangle", property: "scaleX", keyframes: [{ at: 0, value: 0.65 }, { at: 3, value: 1.2, easing: "thereAndBack" }, { at: 6, value: 0.65 }] },
    { target: "triangle", property: "scaleY", keyframes: [{ at: 0, value: 0.65 }, { at: 3, value: 1.2, easing: "thereAndBack" }, { at: 6, value: 0.65 }] },
  ],
  signals: [
    { id: "float", keyframes: [{ at: 0, value: 0 }, { at: 1.5, value: 0.55, easing: "smooth" }, { at: 3, value: 0 }, { at: 4.5, value: -0.55, easing: "smooth" }, { at: 6, value: 0 }] },
    { id: "ring-size", keyframes: [{ at: 0, value: 1.2 }, { at: 6, value: 1.2 }] },
  ],
  bindings: [
    { target: "square", property: "y", expression: { op: "signal", id: "float" } },
    { target: "ring", property: "radius", expression: { op: "signal", id: "ring-size" } },
  ],
  controls: [
    { id: "ring-size", label: "Ring radius", signal: "ring-size", min: 0.4, max: 2.2, step: 0.05, default: 1.2 },
  ],
});
