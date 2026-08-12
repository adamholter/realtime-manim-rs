"use client";

import { useCallback, useEffect, useRef, useState } from "react";

type Runtime = typeof import("../../packages/manim-web/src/index.js");
type Player = Awaited<ReturnType<Runtime["createManimPlayer"]>>;

type Demo = {
  id: string;
  title: string;
  summary: string;
  code: string;
};

const demos: Demo[] = [
  {
    id: "mobius",
    title: "Möbius light field",
    summary: "A dense, lit non-orientable mesh with a cinematic orbital camera.",
    code: `const { Scene, Mesh } = manim;

const vertices = [];
const triangles = [];
const colors = [];
const palette = ["#8B5CF6", "#3B82F6", "#22D3EE", "#34D399"];
const rings = 96;
const strips = 18;

for (let i = 0; i <= rings; i++) {
  const u = (i / rings) * Math.PI * 2;
  for (let j = 0; j < strips; j++) {
    const v = -0.82 + (j / (strips - 1)) * 1.64;
    vertices.push([
      (2.55 + v * Math.cos(u / 2)) * Math.cos(u),
      (2.55 + v * Math.cos(u / 2)) * Math.sin(u),
      v * Math.sin(u / 2),
    ]);
    colors.push(palette[(i + Math.floor(j / 5)) % palette.length]);
  }
}

for (let i = 0; i < rings; i++) {
  for (let j = 0; j < strips - 1; j++) {
    const a = i * strips + j;
    const b = (i + 1) * strips + j;
    triangles.push([a, b, a + 1], [a + 1, b, b + 1]);
  }
}

const band = new Mesh(vertices, triangles, {
  colors,
  gloss: 0.92,
  shadow: 0.52,
  doubleSided: true,
  lightPosition: [-4, 7, 9],
});

const scene = new Scene({
  title: "Möbius light field",
  width: 16,
  height: 9,
  duration: 12,
  background: "#050711",
}).add(band).setCamera3D({
  position: [8.4, 4.2, 8.4],
  target: [0, 0, 0],
  fovY: 0.72,
  near: 0.1,
  far: 60,
  ambient: 0.26,
});

scene.track("__camera__", "camera3dOrbit", [
  { at: 0, value: [8.4, 4.2, 8.4] },
  { at: 4, value: [-7.4, 5.8, 8.8], easing: "smooth" },
  { at: 8, value: [-8.2, 2.4, -8.2], easing: "smooth" },
  { at: 12, value: [8.4, 4.2, 8.4], easing: "smooth" },
]);
scene.signal("tilt", [{ at: 0, value: -0.55 }]);
scene.signal("depth", [{ at: 0, value: 1 }]);
scene.signal("lens", [{ at: 0, value: 0.72 }]);
scene.bind(band, "rotationX", { op: "signal", id: "tilt" });
scene.bind(band, "scaleZ", { op: "signal", id: "depth" });
scene.bind("__camera__", "camera3dFovY", { op: "signal", id: "lens" });
scene.control("tilt", { label: "Band tilt", min: -1.4, max: 1.4, step: 0.01, default: -0.55 });
scene.control("depth", { label: "Ribbon depth", min: 0.15, max: 2.4, step: 0.01, default: 1 });
scene.control("lens", { label: "Camera lens", min: 0.38, max: 1.25, step: 0.01, default: 0.72 });

return scene;`,
  },
  {
    id: "knot",
    title: "Torus-knot reactor",
    summary: "A 9,000-triangle reactor coil wrapped around a luminous spatial curve.",
    code: `const { Scene, Mesh, Path3D, pathCommand3D } = manim;

const vertices = [];
const triangles = [];
const colors = [];
const curve = [];
const palette = ["#F97316", "#FACC15", "#FB7185", "#C084FC"];
const loops = 150;
const sides = 30;
const p = 2;
const q = 5;

function center(t) {
  const r = 2.5 + 0.78 * Math.cos(q * t);
  return [r * Math.cos(p * t), r * Math.sin(p * t), 0.78 * Math.sin(q * t)];
}

for (let i = 0; i < loops; i++) {
  const t = (i / loops) * Math.PI * 2;
  const c = center(t);
  const next = center(t + 0.001);
  const tangent = next.map((value, axis) => value - c[axis]);
  const length = Math.hypot(...tangent);
  const T = tangent.map((value) => value / length);
  const helper = Math.abs(T[2]) < 0.85 ? [0, 0, 1] : [0, 1, 0];
  const Nraw = [
    T[1] * helper[2] - T[2] * helper[1],
    T[2] * helper[0] - T[0] * helper[2],
    T[0] * helper[1] - T[1] * helper[0],
  ];
  const nLength = Math.hypot(...Nraw);
  const N = Nraw.map((value) => value / nLength);
  const B = [
    T[1] * N[2] - T[2] * N[1],
    T[2] * N[0] - T[0] * N[2],
    T[0] * N[1] - T[1] * N[0],
  ];

  curve.push(i === 0 ? pathCommand3D.moveTo(c) : pathCommand3D.lineTo(c));
  for (let j = 0; j < sides; j++) {
    const angle = (j / sides) * Math.PI * 2;
    const radius = 0.19 + 0.055 * Math.sin(11 * t);
    vertices.push(c.map((value, axis) => value + radius * (N[axis] * Math.cos(angle) + B[axis] * Math.sin(angle))));
    colors.push(palette[Math.floor((i / loops) * palette.length) % palette.length]);
  }
}

for (let i = 0; i < loops; i++) {
  const ni = (i + 1) % loops;
  for (let j = 0; j < sides; j++) {
    const nj = (j + 1) % sides;
    const a = i * sides + j;
    const b = ni * sides + j;
    triangles.push([a, b, i * sides + nj], [i * sides + nj, b, ni * sides + nj]);
  }
}

const coil = new Mesh(vertices, triangles, {
  colors,
  gloss: 1,
  shadow: 0.62,
  doubleSided: true,
  lightPosition: [2, 8, 10],
});
const core = new Path3D(curve, {
  style: { stroke: "#FFF7D6", strokeWidth: 0.075, opacity: 0.9 },
});

const scene = new Scene({
  title: "Torus-knot reactor",
  width: 16,
  height: 9,
  duration: 14,
  background: "#080407",
}).add(coil, core).setCamera3D({
  position: [8.5, 5.2, 9.5], target: [0, 0, 0], fovY: 0.7,
  near: 0.1, far: 70, ambient: 0.2,
});

scene.track("__camera__", "camera3dOrbit", [
  { at: 0, value: [8.5, 5.2, 9.5] },
  { at: 7, value: [-9, 3.2, 7.5], easing: "smooth" },
  { at: 14, value: [8.5, 5.2, 9.5], easing: "smooth" },
]);
scene.signal("pitch", [{ at: 0, value: -0.35 }]);
scene.signal("yaw", [{ at: 0, value: 0.1 }]);
scene.signal("stretch", [{ at: 0, value: 1 }]);
scene.bind(coil, "rotationX", { op: "signal", id: "pitch" });
scene.bind(core, "rotationX", { op: "signal", id: "pitch" });
scene.bind(coil, "rotationY", { op: "signal", id: "yaw" });
scene.bind(core, "rotationY", { op: "signal", id: "yaw" });
scene.bind(coil, "scaleZ", { op: "signal", id: "stretch" });
scene.bind(core, "scaleZ", { op: "signal", id: "stretch" });
scene.control("pitch", { label: "Reactor pitch", min: -1.5, max: 1.5, step: 0.01, default: -0.35 });
scene.control("yaw", { label: "Reactor yaw", min: -3.14, max: 3.14, step: 0.01, default: 0.1 });
scene.control("stretch", { label: "Core stretch", min: 0.25, max: 2.4, step: 0.01, default: 1 });

return scene;`,
  },
  {
    id: "wave",
    title: "Gravitational wave grid",
    summary: "A 3D interference surface with layered curvature and a low-angle flyby.",
    code: `const { Scene, Mesh, Path3D, pathCommand3D } = manim;

const vertices = [];
const triangles = [];
const colors = [];
const rings = [];
const size = 58;
const span = 8.8;
const palette = ["#0EA5E9", "#22D3EE", "#A78BFA", "#F472B6"];

for (let y = 0; y < size; y++) {
  for (let x = 0; x < size; x++) {
    const px = -span / 2 + (x / (size - 1)) * span;
    const py = -span / 2 + (y / (size - 1)) * span;
    const r1 = Math.hypot(px + 1.45, py);
    const r2 = Math.hypot(px - 1.45, py);
    const z = 0.46 * Math.sin(4.4 * r1) / (1 + 0.18 * r1 * r1)
      + 0.46 * Math.sin(4.4 * r2) / (1 + 0.18 * r2 * r2);
    vertices.push([px, py, z]);
    const band = Math.max(0, Math.min(3, Math.floor((z + 0.8) * 2.4)));
    colors.push(palette[band]);
  }
}

for (let y = 0; y < size - 1; y++) {
  for (let x = 0; x < size - 1; x++) {
    const a = y * size + x;
    const b = a + 1;
    const c = a + size;
    const d = c + 1;
    triangles.push([a, c, b], [b, c, d]);
  }
}

for (const radius of [1.05, 2.05, 3.05]) {
  const commands = [];
  for (let i = 0; i <= 120; i++) {
    const t = (i / 120) * Math.PI * 2;
    const point = [
      -1.45 + radius * Math.cos(t),
      radius * Math.sin(t),
      0.58,
    ];
    commands.push(i === 0 ? pathCommand3D.moveTo(point) : pathCommand3D.lineTo(point));
  }
  rings.push(new Path3D(commands, { style: { stroke: "#E0F2FEAA", strokeWidth: 0.025 } }));
}

const field = new Mesh(vertices, triangles, {
  colors,
  gloss: 0.82,
  shadow: 0.44,
  doubleSided: true,
  lightPosition: [-5, 4, 10],
});

const scene = new Scene({
  title: "Gravitational wave grid",
  width: 16,
  height: 9,
  duration: 11,
  background: "#030712",
}).add(field, ...rings).setCamera3D({
  position: [7.8, -9.8, 6.4], target: [0, 0, 0], fovY: 0.72,
  near: 0.1, far: 80, ambient: 0.24,
});

scene.track("__camera__", "camera3dOrbit", [
  { at: 0, value: [7.8, -9.8, 6.4] },
  { at: 5.5, value: [-8.8, -7.4, 4.1], easing: "smooth" },
  { at: 11, value: [7.8, -9.8, 6.4], easing: "smooth" },
]);
scene.signal("amplitude", [{ at: 0, value: 1 }]);
scene.signal("bank", [{ at: 0, value: -0.12 }]);
scene.signal("lens", [{ at: 0, value: 0.72 }]);
scene.bind(field, "scaleZ", { op: "signal", id: "amplitude" });
for (const ring of rings) scene.bind(ring, "scaleZ", { op: "signal", id: "amplitude" });
scene.bind(field, "rotationY", { op: "signal", id: "bank" });
for (const ring of rings) scene.bind(ring, "rotationY", { op: "signal", id: "bank" });
scene.bind("__camera__", "camera3dFovY", { op: "signal", id: "lens" });
scene.control("amplitude", { label: "Wave amplitude", min: 0.08, max: 3, step: 0.01, default: 1 });
scene.control("bank", { label: "Field bank", min: -1, max: 1, step: 0.01, default: -0.12 });
scene.control("lens", { label: "Camera lens", min: 0.38, max: 1.3, step: 0.01, default: 0.72 });

return scene;`,
  },
];

const runtimeUrl = "/playground/realtime-manim.js";

function formatValue(value: number) {
  return Number(value).toFixed(2).replace(/\.00$/, "");
}

export default function DemoPlayground() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const playerRef = useRef<Player | null>(null);
  const runtimeRef = useRef<Runtime | null>(null);
  const [selectedId, setSelectedId] = useState(demos[0].id);
  const [code, setCode] = useState(demos[0].code);
  const [controls, setControls] = useState<Array<{ id: string; label: string; signal: string; min: number; max: number; step: number; default: number }>>([]);
  const [values, setValues] = useState<Record<string, number>>({});
  const [status, setStatus] = useState("Loading Rust + WebGPU…");
  const [error, setError] = useState("");
  const [running, setRunning] = useState(true);

  const renderCode = useCallback(async (source: string) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    setStatus("Compiling scene…");
    setError("");
    const started = performance.now();
    try {
      const importFromBrowser = new Function("url", "return import(url)") as (url: string) => Promise<Runtime>;
      const runtime = runtimeRef.current ?? await importFromBrowser(runtimeUrl);
      runtimeRef.current = runtime;
      const execute = new Function("manim", `"use strict"; return (async () => {\n${source}\n})()`);
      const scene = await execute(runtime);
      if (!(scene instanceof runtime.Scene)) throw new TypeError("Scene code must return a Scene.");
      const sceneData = scene.toJSON();
      playerRef.current?.destroy();
      const player = await runtime.createManimPlayer({
        canvas,
        scene,
        autoplay: true,
        wasmUrl: "/runtime/realtime_manim_web_preview_bg.wasm",
      });
      playerRef.current = player;
      const nextControls = sceneData.controls ?? [];
      setControls(nextControls);
      setValues(Object.fromEntries(nextControls.map((control) => [control.signal, control.default])));
      setRunning(true);
      setStatus(`Live · ${(performance.now() - started).toFixed(0)} ms build`);
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      setError(message);
      setStatus("Scene error");
    }
  }, []);

  useEffect(() => {
    void renderCode(code);
    return () => playerRef.current?.destroy();
    // The initial example owns boot; later edits run explicitly.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const resize = new ResizeObserver(([entry]) => {
      const ratio = Math.min(devicePixelRatio || 1, 2);
      const width = Math.max(1, Math.round(entry.contentRect.width * ratio));
      const height = Math.max(1, Math.round(entry.contentRect.height * ratio));
      try { playerRef.current?.setSize(width, height); } catch { /* Player may be rebuilding. */ }
    });
    resize.observe(canvas);
    return () => resize.disconnect();
  }, []);

  function chooseDemo(demo: Demo) {
    setSelectedId(demo.id);
    setCode(demo.code);
    void renderCode(demo.code);
  }

  function setSignal(signal: string, value: number) {
    setValues((current) => ({ ...current, [signal]: value }));
    try { playerRef.current?.setSignal(signal, value); } catch { /* Player may be rebuilding. */ }
  }

  function togglePlayback() {
    const player = playerRef.current;
    if (!player) return;
    if (running) player.pause(); else player.play();
    setRunning(!running);
  }

  function restart() {
    const player = playerRef.current;
    if (!player) return;
    player.seek(0);
    player.play();
    setRunning(true);
  }

  return (
    <section className="section demo-section shell" id="demos">
      <div className="eyebrow">Interactive 3D lab</div>
      <div className="section-heading demo-heading">
        <h2>Edit the scene. Move the math.</h2>
        <p>Switch examples, change the retained Manim code, or drag a parameter. Rust renders every frame in real time.</p>
      </div>

      <div className="demo-tabs" role="tablist" aria-label="3D demo scenes">
        {demos.map((demo) => (
          <button
            className={selectedId === demo.id ? "demo-tab active" : "demo-tab"}
            key={demo.id}
            onClick={() => chooseDemo(demo)}
            role="tab"
            aria-selected={selectedId === demo.id}
          >
            <span>{demo.title}</span>
            <small>{demo.summary}</small>
          </button>
        ))}
      </div>

      <div className="playground">
        <div className="render-panel">
          <div className="render-toolbar">
            <span className={error ? "live-indicator error" : "live-indicator"}>{status}</span>
            <div>
              <button onClick={togglePlayback} type="button">{running ? "Pause" : "Play"}</button>
              <button onClick={restart} type="button">Restart</button>
            </div>
          </div>
          <div className="canvas-frame">
            <canvas ref={canvasRef} aria-label="Live realtime-manim 3D render" />
            {error && <div className="canvas-error" role="alert"><b>Couldn’t render</b><span>{error}</span></div>}
          </div>
          <div className="parameter-panel" aria-label="Live scene parameters">
            {controls.map((control) => (
              <label className="parameter" key={control.id}>
                <span><b>{control.label}</b><output>{formatValue(values[control.signal] ?? control.default)}</output></span>
                <input
                  aria-label={control.label}
                  type="range"
                  min={control.min}
                  max={control.max}
                  step={control.step}
                  value={values[control.signal] ?? control.default}
                  onChange={(event) => setSignal(control.signal, Number(event.target.value))}
                />
              </label>
            ))}
          </div>
        </div>

        <div className="editor-panel">
          <div className="code-bar"><span>scene.js · live</span><button type="button" onClick={() => void renderCode(code)}>Render code</button></div>
          <textarea
            aria-label="Editable realtime-manim scene code"
            value={code}
            onChange={(event) => setCode(event.target.value)}
            spellCheck={false}
          />
        </div>
      </div>
      <p className="demo-footnote">WebGPU required. The editor runs locally in your browser; no scene code is sent to a server.</p>
    </section>
  );
}
