import { createHash, randomBytes } from "node:crypto";
import { spawn } from "node:child_process";
import { createReadStream } from "node:fs";
import { mkdir, mkdtemp, readFile, realpath, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { homedir, tmpdir } from "node:os";
import { basename, extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { OpenRouter } from "@openrouter/agent";
import {
  DEFAULT_SCENE,
  formatSceneCode,
  parseSceneCode,
} from "../apps/web-preview/www/scene-schema.js";

const MODEL = "openai/gpt-5.6-terra";
const PORT = Number.parseInt(process.env.PORT || "8917", 10);
const HOST = process.env.HOST || "127.0.0.1";
const ROOT = resolve(fileURLToPath(new URL("../apps/web-preview/www/", import.meta.url)));
const VENDOR_FILES = new Map([
  [
    "/vendor/webm-muxer.mjs",
    resolve(fileURLToPath(new URL("../node_modules/webm-muxer/build/webm-muxer.mjs", import.meta.url))),
  ],
]);
const SESSION_COOKIE = "realtime_manim_session";
const SESSION_TTL_MS = 8 * 60 * 60 * 1000;
const MAX_BODY_BYTES = 32 * 1024;
const MAX_ASSET_BODY_BYTES = 18 * 1024 * 1024;
const MAX_SESSION_ASSET_BYTES = 32 * 1024 * 1024;
const MAX_TEX_CACHE_ENTRIES = 256;
const MAX_MANIM_CACHE_ENTRIES = 64;
const TECTONIC_BIN = process.env.TECTONIC_BIN || "/opt/homebrew/bin/tectonic";
const PDF2SVG_BIN = process.env.PDF2SVG_BIN || "/opt/homebrew/bin/pdf2svg";
const SANDBOX_EXEC_BIN = "/usr/bin/sandbox-exec";
const USER_HOME = homedir();
const MANIM_PYTHON_BIN = resolve(
  fileURLToPath(new URL("../.venv-manim-reference/bin/python", import.meta.url)),
);
const MANIM_COMPILER = resolve(
  fileURLToPath(new URL("../scripts/compile-manim.py", import.meta.url)),
);
const sessions = new Map();
const typesetCache = new Map();
const manimCompileCache = new Map();
const ASSET_EXTENSIONS = new Set([
  ".png",
  ".jpg",
  ".jpeg",
  ".webp",
  ".gif",
  ".svg",
  ".wav",
  ".mp3",
  ".m4a",
  ".aac",
  ".ogg",
  ".flac",
  ".ttf",
  ".otf",
]);

const SCENE_INSTRUCTIONS = `You are the animation compiler inside Live Math Lab.
Convert the request into exactly one general retained scene program. Output code only.

Return scene({...}); with strict JSON. The root contract is:
{
  "version": 2,
  "title": string,
  "width": 16,
  "height": 9,
  "pixelWidth": 1280,
  "pixelHeight": 720,
  "duration": 0.25..60,
  "fps": 1..120,
  "background": "#rrggbb" or "#rrggbbaa",
  "camera": {"x":0,"y":0,"zoom":1,"rotation":0},
  "camera3d": {"position":[0,0,8],"target":[0,0,0],"up":[0,1,0],"fovY":0.785,"near":0.1,"far":100,"ambient":0.28,"lightDirection":[-0.4,0.7,1]},
  "nodes": [...],
  "tracks": [...],
  "signals": [...],
  "bindings": [...],
  "controls": [...]
}

Coordinates use a 16 by 9 world centered at [0,0], positive y up. Every node needs a unique id and type. Common optional node fields:
"parent": id, "zIndex": integer,
"transform":{"x":0,"y":0,"z":0,"rotation":radians,"rotationX":0,"rotationY":0,"scaleX":1,"scaleY":1,"scaleZ":1},
"style":{"fill":"#rrggbb" or null,"fillGradient":{"from":[x,y],"to":[x,y],"stops":[{"offset":0..1,"color":"#rrggbb"},...]} or null,"stroke":"#rrggbb" or null,"strokeGradient":same gradient shape or null,"strokeWidth":0.04,"opacity":1,"drawStart":0,"drawProgress":1},
"appearAt": seconds, "disappearAt": seconds.

Node types:
- {"type":"group"}: hierarchy/transform container.
- {"type":"circle","radius":number}
- {"type":"rect","width":number,"height":number,"cornerRadius":number}
- {"type":"line","from":[x,y],"to":[x,y]}
- {"type":"arrow","from":[x,y],"to":[x,y],"tipSize":0.24}
- {"type":"polyline","points":[[x,y],...],"closed":boolean}
- {"type":"path","commands":[{"op":"moveTo","x":n,"y":n},{"op":"lineTo",...},{"op":"quadTo","cx":n,"cy":n,"x":n,"y":n},{"op":"cubicTo","c1x":n,"c1y":n,"c2x":n,"c2y":n,"x":n,"y":n},{"op":"close"}]}
- {"type":"pathRef","source":"path-node-id"}: reuse immutable path geometry from a concrete path node while retaining independent style, transform, tracks, and lifetime.
- {"type":"path3d","commands":[...]}: camera-independent moveTo/lineTo/quadTo/cubicTo/close geometry with z/cz/c1z/c2z coordinates, projected by Rust through camera3d at explicit time.
- {"type":"tracePath","segments":[...],"frames":[...]}: one shared cubic segment stream plus explicit-time sliding windows for retained trails and TracedPath-style updaters.
- Tracks may use {"keyframesFrom":"source-node-id"} instead of keyframes to reuse an identical concrete timeline for the same property.
- {"type":"text","text":string,"fontSize":number,"align":"left"|"center"|"right","weight":"normal"|"bold","slant":"normal"|"italic"}
- {"type":"markupText","spans":[{"text":string,"color":"#rrggbb","weight":"normal"|"bold","slant":"normal"|"italic"},...],"fontSize":number,"align":"left"|"center"|"right"}: mixed-style OpenType text.
- {"type":"mathTex","tex":string,"fontSize":number}: real LaTeX math compiled to vector paths.
- {"type":"image","pixels":base64RGBA,"pixelWidth":integer,"pixelHeight":integer,"corners":[topLeft,topRight,bottomLeft,bottomRight],"resampling":"nearest"|"bilinear"|"bicubic"}
- {"type":"pointCloud","radius":number,"points":[{"x":n,"y":n,"color":"#rrggbb","radius":n},...]}
- {"type":"mesh","vertices":[[x,y,z],...],"triangles":[[i,j,k],...],"colors":["#rrggbbaa",...],"normals":[[x,y,z],...],"uvs":[[u,v],...],"texturePixels":base64RGBA|"","textureWidth":integer,"textureHeight":integer,"darkTexturePixels":base64RGBA|"","darkTextureWidth":integer,"darkTextureHeight":integer,"textureResampling":"nearest"|"bilinear"|"bicubic","gloss":0..1,"shadow":0..1,"lightPosition":[x,y,z],"unlit":boolean,"doubleSided":boolean}: smooth-lit or perspective-textured 3D triangle mesh, including Manim-style normal/light-dependent dual-texture materials.
- {"type":"surface","vertices":[[x,y,z],...],"patches":[[i,j,k,...],...],"colors":["#rrggbbaa",...],"strokeColors":["#rrggbbaa",...],"strokeRadii":[number,...],"unlit":boolean,"doubleSided":boolean}: compact indexed Manim surface patches; colors flatten patch corners and stroke arrays match patch count.

Tracks animate a property on a node:
{"target":"node-id","property":"x"|"y"|"z"|"rotation"|"rotationX"|"rotationY"|"scaleX"|"scaleY"|"scaleZ"|"opacity"|"strokeWidth"|"drawStart"|"drawProgress"|"fill"|"fillGradient"|"stroke"|"strokeGradient"|"radius"|"points"|"vertices"|"normals"|"colors"|"lightPosition"|"commands"|"pathData"|"transform2d",
 "keyframes":[{"at":seconds,"value":number|string|[[x,y],...],"easing":"linear"|"smooth"|"easeIn"|"easeOut"|"easeInOut"|"thereAndBack"|"bounce"},...]}
Camera tracks target "__camera__" with cameraX, cameraY, cameraZoom, cameraRotation,
camera3dPosition, camera3dTarget, camera3dUp, or camera3dFovY.

Signals are reusable numeric timelines:
{"id":"signal-id","keyframes":[{"at":seconds,"value":number,"easing":"smooth"},...]}
Bindings attach a numeric expression to a node property:
{"target":"node-id","property":"x","expression":{"op":"signal","id":"signal-id"}}
Expression ops: constant(value), time, signal(id), add(args), multiply(args), subtract(left,right), divide(left,right), sin(value), cos(value), abs(value), min(args), max(args), clamp(value,min,max), lerp(from,to,amount).

Optional controls expose a signal as a live browser slider:
{"id":"radius-control","label":"Radius","signal":"radius","min":0.2,"max":3,"step":0.05,"default":1}

Compilation rules:
- Output only one scene({...}); program. No markdown, comments, imports, functions, WGSL, HTML, or prose.
- Build the requested visual from independently addressable nodes. Never substitute a wave or unrelated canned scene.
- Use text for ordinary labels and mathTex for mathematical notation. Use lines/arrows for axes, pointCloud for scatter data, polyline/path for graphs/frontiers, groups for hierarchy, and tracks/lifetimes for staged explanation.
- Make the scene visually legible: strong contrast, sensible zIndex, world-unit stroke widths around 0.025..0.12, font sizes around 0.28..0.7.
- Use at least 6 nodes for a substantive request and materially different node types when appropriate.
- Animation keyframes must be ordered within duration. Use opacity, transforms, drawProgress, appearAt/disappearAt, camera tracks, and signals to tell the story.
- If looping is requested, end looping properties at their starting values. Otherwise prioritize a clear final frame.
- Do not invent unsupported node types or properties.`;

const MANIM_INSTRUCTIONS = `You are the Manim author inside Live Math Lab.
Convert the user's request into one complete Manim Community Python source file.

Output only executable Python source. No markdown fences, prose, JSON, shell, or commentary.
The source must:
- begin with "from manim import *";
- define exactly one public scene class named GeneratedScene;
- use regular Manim Community APIs, including Scene, ThreeDScene, MovingCameraScene,
  Mobjects, animations, updaters, ValueTracker, MathTex, MarkupText, SVG, graphs,
  vector fields, surfaces, and composition helpers when appropriate;
- construct the requested animation faithfully rather than substituting an unrelated
  canned wave or demo;
- use independently addressable Mobjects and clear visual staging;
- keep duration between 0.25 and 20 seconds unless the user explicitly asks otherwise;
- avoid network access, subprocesses, environment inspection, filesystem reads/writes,
  dynamic imports, eval, exec, and introspection;
- generate data procedurally in memory.

Use Cairo/default Manim unless the user specifically requests OpenGL behavior. For
OpenGL-only Mobjects, set config.renderer = RendererType.OPENGL, import the exact
class from manim.mobject.opengl when it is not re-exported, and otherwise keep using
normal Scene/ThreeDScene animation semantics.

When the request lists uploaded assets, those files are the sole allowed filesystem
inputs. Reference them exactly as "assets/<safe-name>". Use ImageMobject for raster
images, SVGMobject for SVG, Scene.add_sound for audio, and register_font for TTF/OTF.

The Python is executed with Manim's own semantics in a network-denied, write-isolated
macOS sandbox. Its resulting cubic vector states are compiled into the Rust/WebGPU
runtime at explicit frame times.`;

const MIME_TYPES = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".json", "application/json; charset=utf-8"],
  [".svg", "image/svg+xml"],
  [".png", "image/png"],
]);

function securityHeaders(response) {
  response.setHeader("X-Content-Type-Options", "nosniff");
  response.setHeader("Referrer-Policy", "no-referrer");
  response.setHeader("X-Frame-Options", "DENY");
  response.setHeader(
    "Content-Security-Policy",
    "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self' data: blob:; media-src 'self' blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
  );
}

function json(response, status, payload, headers = {}) {
  securityHeaders(response);
  response.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
    "Cache-Control": "no-store",
    ...headers,
  });
  response.end(JSON.stringify(payload));
}

async function readJson(request, maximumBytes = MAX_BODY_BYTES) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > maximumBytes) throw new HttpError(413, "Request is too large.");
    chunks.push(chunk);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new HttpError(400, "Request body must be valid JSON.");
  }
}

class HttpError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

function runProcess(
  command,
  args,
  {
    cwd,
    input = "",
    timeoutMs = 45_000,
    environment = {},
    killProcessGroup = false,
  } = {},
) {
  return new Promise((resolveProcess, rejectProcess) => {
    const child = spawn(command, args, {
      cwd,
      stdio: ["pipe", "ignore", "pipe"],
      detached: killProcessGroup,
      env: {
        ...process.env,
        PATH: `/opt/homebrew/bin:/usr/local/bin:${process.env.PATH || ""}`,
        ...environment,
      },
    });
    let stderr = "";
    const timeout = setTimeout(() => {
      if (killProcessGroup && child.pid) {
        try {
          process.kill(-child.pid, "SIGKILL");
        } catch {
          child.kill("SIGKILL");
        }
      } else {
        child.kill("SIGKILL");
      }
      rejectProcess(new Error(`${command} timed out.`));
    }, timeoutMs);
    child.stderr.on("data", (chunk) => {
      stderr = `${stderr}${chunk}`.slice(-12_000);
    });
    child.on("error", (error) => {
      clearTimeout(timeout);
      rejectProcess(error);
    });
    child.on("close", (code, signal) => {
      clearTimeout(timeout);
      if (code === 0) {
        resolveProcess();
      } else {
        rejectProcess(
          new Error(
            `${command} failed${signal ? ` (${signal})` : ""}: ${stderr.trim() || `exit ${code}`}`,
          ),
        );
      }
    });
    child.stdin.end(input);
  });
}

async function typesetLatex(source) {
  const cacheKey = createHash("sha256").update(source).digest("hex");
  const cached = typesetCache.get(cacheKey);
  if (cached) return { ...cached, cached: true };

  const startedAt = performance.now();
  const workDirectory = await mkdtemp(join(tmpdir(), "realtime-manim-tex-"));
  try {
    const document = String.raw`\documentclass[preview,border=0pt]{standalone}
\usepackage{amsmath,amssymb,mathtools}
\begin{document}
\(\displaystyle ${source}\)
\end{document}`;
    await runProcess(
      TECTONIC_BIN,
      ["--untrusted", "--chatter", "minimal", "--outdir", workDirectory, "-"],
      { cwd: workDirectory, input: document },
    );
    const pdfPath = join(workDirectory, "texput.pdf");
    const svgPath = join(workDirectory, "texput.svg");
    await runProcess(PDF2SVG_BIN, [pdfPath, svgPath], { cwd: workDirectory, timeoutMs: 20_000 });
    const svg = await readFile(svgPath, "utf8");
    if (!svg.includes("<svg") || svg.length > 4_000_000) {
      throw new Error("LaTeX produced an invalid or oversized SVG.");
    }
    const result = {
      svg,
      typesetMs: Math.round(performance.now() - startedAt),
      sourceHash: cacheKey.slice(0, 16),
    };
    if (typesetCache.size >= MAX_TEX_CACHE_ENTRIES) {
      typesetCache.delete(typesetCache.keys().next().value);
    }
    typesetCache.set(cacheKey, result);
    return { ...result, cached: false };
  } finally {
    await rm(workDirectory, { recursive: true, force: true });
  }
}

function sandboxLiteral(value) {
  return `"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}

function manimSandboxProfile(workDirectory) {
  const readableSubpaths = [
    resolve(MANIM_PYTHON_BIN, "..", ".."),
    join(USER_HOME, "Library", "Fonts"),
  ];
  const readableFiles = [MANIM_COMPILER];
  return `(version 1)
(deny default)
(allow process*)
(allow signal (target self))
(allow sysctl-read)
(allow mach-lookup)
(allow file-read*)
(deny file-read* (subpath ${sandboxLiteral(USER_HOME)}) (subpath "/Volumes") (subpath "/private/var/folders"))
(allow file-read-metadata)
(allow file-read* ${readableSubpaths.map((path) => `(subpath ${sandboxLiteral(path)})`).join(" ")})
(allow file-read* ${readableFiles.map((path) => `(literal ${sandboxLiteral(path)})`).join(" ")})
(allow file-read* (subpath ${sandboxLiteral(workDirectory)}))
(allow file-write* (subpath ${sandboxLiteral(workDirectory)}) (literal "/dev/null"))`;
}

function normalizePythonSource(source) {
  let code = source.trim();
  code = code.replace(/^```(?:python|py)?\s*/i, "").replace(/\s*```$/i, "").trim();
  if (!code.startsWith("from manim import *")) {
    throw new Error("Manim source must begin with `from manim import *`.");
  }
  const sceneClasses = [...code.matchAll(/^class\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/gm)]
    .map((match) => match[1])
    .filter((name) => !name.startsWith("_"));
  if (sceneClasses.length !== 1 || sceneClasses[0] !== "GeneratedScene") {
    throw new Error("Manim source must define exactly one public GeneratedScene class.");
  }
  if (code.length > 100_000) {
    throw new Error("Generated Manim source is too large.");
  }
  return `${code}\n`;
}

async function compileManimSource(source, fps = 30, assets = new Map()) {
  const code = normalizePythonSource(source);
  const compilerStat = await stat(MANIM_COMPILER);
  const cacheHash = createHash("sha256")
    .update(code)
    .update(`\0fps:${fps}\0compiler:${compilerStat.size}:${compilerStat.mtimeMs}`);
  for (const [name, asset] of [...assets.entries()].sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    cacheHash.update(`\0asset:${name}:`);
    cacheHash.update(asset.data);
  }
  const cacheKey = cacheHash.digest("hex");
  const cached = manimCompileCache.get(cacheKey);
  if (cached) {
    manimCompileCache.delete(cacheKey);
    manimCompileCache.set(cacheKey, cached);
    return {
      ...cached,
      compileMs: 0,
      originalCompileMs: cached.compileMs,
      cached: true,
    };
  }
  const startedAt = performance.now();
  const workDirectory = await mkdtemp(join(tmpdir(), "realtime-manim-compile-"));
  const sourcePath = join(workDirectory, "generated_scene.py");
  const outputPath = join(workDirectory, "scene.json");
  const receiptPath = join(workDirectory, "scene.receipt.json");
  try {
    const canonicalWorkDirectory = await realpath(workDirectory);
    if (assets.size > 0) {
      const assetDirectory = join(workDirectory, "assets");
      await mkdir(assetDirectory, { mode: 0o700 });
      for (const [name, asset] of assets) {
        await writeFile(join(assetDirectory, name), asset.data, { mode: 0o600 });
      }
    }
    await writeFile(sourcePath, code, { encoding: "utf8", mode: 0o600 });
    await runProcess(
      SANDBOX_EXEC_BIN,
      [
        "-p",
        manimSandboxProfile(canonicalWorkDirectory),
        MANIM_PYTHON_BIN,
        MANIM_COMPILER,
        sourcePath,
        "GeneratedScene",
        "--fps",
        String(fps),
        "--output",
        outputPath,
      ],
      {
        cwd: workDirectory,
        timeoutMs: 120_000,
        killProcessGroup: true,
        environment: {
          XDG_CACHE_HOME: join(workDirectory, ".cache"),
          TEXMFROOT: "/opt/homebrew/opt/texlive/share",
          TEXMFCNF: "/opt/homebrew/opt/texlive/share/texmf-dist/web2c",
          MPLCONFIGDIR: join(workDirectory, ".matplotlib"),
        },
      },
    );
    const outputStat = await stat(outputPath);
    if (!outputStat.isFile() || outputStat.size > 64 * 1024 * 1024) {
      throw new Error("Compiled Manim scene is missing or exceeds 64 MB.");
    }
    const [rawScene, rawReceipt] = await Promise.all([
      readFile(outputPath, "utf8"),
      readFile(receiptPath, "utf8"),
    ]);
    const scene = parseSceneCode(`scene(${rawScene});`);
    const receipt = JSON.parse(rawReceipt);
    const result = {
      code,
      scene,
      receipt,
      compileMs: Math.round(performance.now() - startedAt),
      cached: false,
    };
    if (manimCompileCache.size >= MAX_MANIM_CACHE_ENTRIES) {
      manimCompileCache.delete(manimCompileCache.keys().next().value);
    }
    manimCompileCache.set(cacheKey, result);
    return result;
  } finally {
    await rm(workDirectory, { recursive: true, force: true });
  }
}

function parseCookies(request) {
  const cookies = new Map();
  for (const part of (request.headers.cookie || "").split(";")) {
    const separator = part.indexOf("=");
    if (separator <= 0) continue;
    cookies.set(part.slice(0, separator).trim(), decodeURIComponent(part.slice(separator + 1).trim()));
  }
  return cookies;
}

function getSession(request) {
  const id = parseCookies(request).get(SESSION_COOKIE);
  const session = id ? sessions.get(id) : undefined;
  if (!session) return undefined;
  if (Date.now() - session.touchedAt > SESSION_TTL_MS) {
    sessions.delete(id);
    return undefined;
  }
  session.touchedAt = Date.now();
  return { id, record: session };
}

function sessionCookie(id, maxAgeSeconds = SESSION_TTL_MS / 1000) {
  return `${SESSION_COOKIE}=${encodeURIComponent(id)}; HttpOnly; SameSite=Strict; Path=/; Max-Age=${Math.floor(maxAgeSeconds)}`;
}

function safeMessage(error) {
  const message = error instanceof Error ? error.message : String(error);
  return message
    .replace(/sk-or-[A-Za-z0-9_-]+/g, "[redacted]")
    .replace(/Bearer\s+[A-Za-z0-9._-]+/gi, "Bearer [redacted]")
    .slice(0, 500);
}

async function validateKey(apiKey) {
  const client = new OpenRouter({ apiKey });
  const result = client.callModel({
    model: MODEL,
    input: "Reply with exactly CONNECTED.",
    instructions: "Return only the requested word.",
    maxOutputTokens: 8,
  });
  const text = await result.getText();
  if (text.trim() !== "CONNECTED") {
    throw new Error("Terra responded unexpectedly while checking the key.");
  }
}

function conversationInput(session, prompt) {
  const recent = session.history.slice(-4);
  const assetContext =
    session.assets.size === 0
      ? ""
      : `\n\nUploaded assets available inside the sandbox:\n${[...session.assets.keys()]
          .map((name) => `- assets/${name}`)
          .join("\n")}`;
  if (recent.length === 0) return `${prompt}${assetContext}`;
  const context = recent
    .map((turn) => `${turn.role === "user" ? "User" : "Previous scene code"}:\n${turn.content}`)
    .join("\n\n");
  return `${context}\n\nCurrent user request:\n${prompt}${assetContext}`;
}

async function streamScene(response, session, prompt) {
  securityHeaders(response);
  response.writeHead(200, {
    "Content-Type": "application/x-ndjson; charset=utf-8",
    "Cache-Control": "no-store",
    Connection: "keep-alive",
    "X-Accel-Buffering": "no",
  });

  let cancelled = false;
  response.on("close", () => {
    cancelled = true;
  });

  const startedAt = performance.now();
  let code = "";
  let repaired = false;
  let lastError;

  for (let attempt = 0; attempt < 2; attempt += 1) {
    const client = new OpenRouter({ apiKey: session.apiKey });
    const input =
      attempt === 0
        ? conversationInput(session, prompt)
        : `Repair this invalid Manim Community Python source. Return the complete corrected Python file only.

Compilation or compatibility error: ${lastError}

Invalid code:
${code}`;
    const result = client.callModel({
      model: MODEL,
      input,
      instructions: MANIM_INSTRUCTIONS,
      maxOutputTokens: 20_000,
      temperature: 0.25,
    });

    if (attempt > 0) {
      code = "";
      repaired = true;
      response.write(`${JSON.stringify({ type: "reset", reason: "Repairing scene code…" })}\n`);
    }

    for await (const delta of result.getTextStream()) {
      if (cancelled) {
        await result.cancel();
        return;
      }
      code += delta;
      response.write(`${JSON.stringify({ type: "delta", delta })}\n`);
    }

    try {
      const agentMs = Math.round(performance.now() - startedAt);
      const compiled = await compileManimSource(code, 30, session.assets);
      session.history.push(
        { role: "user", content: prompt },
        { role: "assistant", content: compiled.code },
      );
      session.history = session.history.slice(-8);
      response.write(
        `${JSON.stringify({
          type: "done",
          code: compiled.code,
          scene: compiled.scene,
          receipt: compiled.receipt,
          model: MODEL,
          modelMs: agentMs,
          compileMs: compiled.compileMs,
          compileCached: compiled.cached,
          originalCompileMs: compiled.originalCompileMs,
          repaired,
        })}\n`,
      );
      response.end();
      return;
    } catch (error) {
      lastError = safeMessage(error);
    }
  }

  response.write(
    `${JSON.stringify({
      type: "error",
      message: `Terra could not produce compilable Manim code: ${lastError}`,
    })}\n`,
  );
  response.end();
}

async function handleApi(request, response, url) {
  if (request.method === "GET" && url.pathname === "/api/health") {
    return json(response, 200, {
      ok: true,
      model: MODEL,
      sdk: "@openrouter/agent",
      build: createHash("sha256")
        .update(`${SCENE_INSTRUCTIONS}\n${MANIM_INSTRUCTIONS}`)
        .digest("hex")
        .slice(0, 10),
    });
  }

  if (request.method === "GET" && url.pathname === "/api/session") {
    const session = getSession(request);
    return json(response, 200, { connected: Boolean(session), model: MODEL });
  }

  if (request.method === "POST" && url.pathname === "/api/session") {
    const body = await readJson(request);
    const apiKey = typeof body.apiKey === "string" ? body.apiKey.trim() : "";
    if (!apiKey || apiKey.length > 512) throw new HttpError(400, "Enter a valid OpenRouter key.");
    await validateKey(apiKey);
    const id = randomBytes(24).toString("base64url");
    sessions.set(id, { apiKey, touchedAt: Date.now(), history: [], assets: new Map() });
    return json(
      response,
      200,
      { connected: true, model: MODEL },
      { "Set-Cookie": sessionCookie(id) },
    );
  }

  if (request.method === "DELETE" && url.pathname === "/api/session") {
    const session = getSession(request);
    if (session) sessions.delete(session.id);
    return json(
      response,
      200,
      { connected: false },
      { "Set-Cookie": sessionCookie("", 0) },
    );
  }

  if (request.method === "POST" && url.pathname === "/api/animate") {
    const session = getSession(request);
    if (!session) throw new HttpError(401, "Connect an OpenRouter key first.");
    const body = await readJson(request);
    const prompt = typeof body.prompt === "string" ? body.prompt.trim() : "";
    if (!prompt) throw new HttpError(400, "Describe the animation you want.");
    if (prompt.length > 4000) throw new HttpError(400, "Prompt must be under 4,000 characters.");
    await streamScene(response, session.record, prompt);
    return;
  }

  if (request.method === "POST" && url.pathname === "/api/assets") {
    const session = getSession(request);
    if (!session) throw new HttpError(401, "Connect an OpenRouter key first.");
    const body = await readJson(request, MAX_ASSET_BODY_BYTES);
    const originalName = typeof body.name === "string" ? basename(body.name.trim()) : "";
    const extension = extname(originalName).toLowerCase();
    if (!originalName || originalName.length > 180 || !ASSET_EXTENSIONS.has(extension)) {
      throw new HttpError(400, "Use a PNG, JPEG, WebP, GIF, SVG, audio, TTF, or OTF asset.");
    }
    if (typeof body.data !== "string" || body.data.length === 0 || body.data.length > 17_000_000) {
      throw new HttpError(400, "Asset data is missing or too large.");
    }
    const data = Buffer.from(body.data, "base64");
    if (data.length === 0 || data.length > 12 * 1024 * 1024) {
      throw new HttpError(400, "Each asset must be 12 MB or smaller.");
    }
    const safeStem = originalName
      .slice(0, -extension.length)
      .normalize("NFKD")
      .replace(/[^A-Za-z0-9._-]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 120) || "asset";
    let safeName = `${safeStem}${extension}`;
    let suffix = 2;
    while (session.record.assets.has(safeName)) {
      safeName = `${safeStem}-${suffix}${extension}`;
      suffix += 1;
    }
    const existingBytes = [...session.record.assets.values()].reduce(
      (total, asset) => total + asset.data.length,
      0,
    );
    if (existingBytes + data.length > MAX_SESSION_ASSET_BYTES) {
      throw new HttpError(413, "Uploaded assets exceed the 32 MB session limit.");
    }
    session.record.assets.set(safeName, { data });
    return json(response, 200, {
      name: safeName,
      path: `assets/${safeName}`,
      bytes: data.length,
      count: session.record.assets.size,
    });
  }

  if (request.method === "DELETE" && url.pathname === "/api/assets") {
    const session = getSession(request);
    if (!session) throw new HttpError(401, "Connect an OpenRouter key first.");
    session.record.assets.clear();
    return json(response, 200, { count: 0 });
  }

  if (request.method === "POST" && url.pathname === "/api/typeset") {
    const body = await readJson(request);
    const source = typeof body.tex === "string" ? body.tex.trim() : "";
    if (!source || source.length > 4_000) {
      throw new HttpError(400, "LaTeX source must contain 1–4,000 characters.");
    }
    return json(response, 200, await typesetLatex(source));
  }

  throw new HttpError(404, "API route not found.");
}

async function serveStatic(response, pathname) {
  const vendorPath = VENDOR_FILES.get(pathname);
  if (vendorPath) {
    const vendorStat = await stat(vendorPath).catch(() => undefined);
    if (!vendorStat?.isFile()) throw new HttpError(404, "Vendor file not found.");
    securityHeaders(response);
    response.writeHead(200, {
      "Content-Type": "text/javascript; charset=utf-8",
      "Content-Length": vendorStat.size,
      "Cache-Control": "no-cache",
    });
    createReadStream(vendorPath).pipe(response);
    return;
  }
  const relative = pathname === "/" ? "index.html" : pathname.replace(/^\/+/, "");
  const normalized = normalize(relative);
  const filePath = resolve(join(ROOT, normalized));
  if (!filePath.startsWith(`${ROOT}/`) && filePath !== join(ROOT, "index.html")) {
    throw new HttpError(403, "Forbidden.");
  }
  const fileStat = await stat(filePath).catch(() => undefined);
  if (!fileStat?.isFile()) throw new HttpError(404, "File not found.");
  securityHeaders(response);
  response.writeHead(200, {
    "Content-Type": MIME_TYPES.get(extname(filePath)) || "application/octet-stream",
    "Content-Length": fileStat.size,
    "Cache-Control": "no-cache",
  });
  createReadStream(filePath).pipe(response);
}

const server = createServer(async (request, response) => {
  const url = new URL(request.url || "/", `http://${request.headers.host || `${HOST}:${PORT}`}`);
  try {
    if (url.pathname.startsWith("/api/")) {
      await handleApi(request, response, url);
    } else {
      await serveStatic(response, url.pathname);
    }
  } catch (error) {
    if (response.headersSent) {
      response.end();
      return;
    }
    const status = error instanceof HttpError ? error.status : 500;
    json(response, status, { error: safeMessage(error) });
  }
});

server.listen(PORT, HOST, () => {
  console.log(`Live Math Lab agent server listening on http://${HOST}:${PORT}`);
});

const cleanup = setInterval(() => {
  const cutoff = Date.now() - SESSION_TTL_MS;
  for (const [id, session] of sessions) {
    if (session.touchedAt < cutoff) sessions.delete(id);
  }
}, 15 * 60 * 1000);
cleanup.unref();

process.on("SIGTERM", () => server.close(() => process.exit(0)));
process.on("SIGINT", () => server.close(() => process.exit(0)));

export { DEFAULT_SCENE, compileManimSource };
