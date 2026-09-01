import DemoPlayground from "./DemoPlayground";

const install = `npm install realtime-manim`;

const quickstart = `import {
  Scene, Circle, Square, Text,
  Create, Transform, FadeOut,
  createManimPlayer,
} from "realtime-manim";

const title = new Text("Rust-speed Manim", {
  y: 2.6,
  fontSize: 0.72,
  color: "#f5f7ff",
});
const circle = new Circle({ radius: 1.25, color: "#7c8cff" });
const square = new Square({ sideLength: 2.4, color: "#34d399" });

const scene = new Scene()
  .add(title, circle)
  .play(Create(title), Create(circle), { runTime: 1 })
  .play(Transform(circle, square), { runTime: 0.8 })
  .play(FadeOut(title), { runTime: 0.4 });

const player = await createManimPlayer({
  canvas: document.querySelector("canvas"),
  scene,
});`;

const agentPrompt = `Use the realtime-manim npm package to author this animation.

Rules:
- Import public APIs from "realtime-manim".
- Build a retained Scene; do not write a frame-by-frame JavaScript loop.
- Prefer Mobject helpers, Groups, Axes, Transform, AnimationGroup, and signals.
- Keep the render hot path in the Rust/Wasm player.
- Call createManimPlayer({ canvas, scene }) and destroy() on cleanup.
- If a feature is unsupported, preserve intent and report the exact gap.

Primary references:
- Package: https://www.npmjs.com/package/realtime-manim
- Source/schema/compiler: https://github.com/adamholter/realtime-manim-rs
- Agent index: /llms.txt`;

const capabilities = [
  ["Author", "Typed retained JS: primitives, groups, layout, graphs, text, SVG, raster, meshes, surfaces, paths and cameras."],
  ["Animate", "Create, fades, rotation, movement, transforms, replacement transforms, groups, lagged starts and successions."],
  ["Interact", "Independent canvases, seek-safe evaluation, signals, controls, live sliders and deterministic cleanup."],
  ["Render", "Rust-owned WebAssembly/WebGPU browser runtime plus a native Metal preview and headless PNG renderer."],
  ["Typography", "OpenType registration, fallback chains, bidi, Arabic joining, Hebrew and strict nested Pango-style markup."],
  ["Compatibility", "Python Manim compiler, semantic TransformMatching metadata, explicit diagnostics and receipted corpus checks."],
];

export default function Home() {
  return (
    <main>
      <nav className="nav shell" aria-label="Main navigation">
        <a className="brand" href="#top" aria-label="Realtime Manim home">
          <span className="brand-mark" aria-hidden="true">rm</span>
          <span>realtime-manim</span>
        </a>
        <div className="nav-links">
          <a href="#demos">3D demos</a>
          <a href="#quickstart">Quickstart</a>
          <a href="#agents">For agents</a>
          <a href="https://github.com/adamholter/realtime-manim-rs">GitHub</a>
        </div>
      </nav>

      <section className="hero shell" id="top">
        <div className="hero-main">
          <p className="hero-version">realtime-manim / 0.6 / Rust + WebGPU</p>
          <h1>Manim scenes,<br /><span>at interaction speed.</span></h1>
          <p className="hero-copy">
            Write a retained scene in JavaScript. Rust evaluates it, WebGPU draws it,
            and every slider stays live—without a JavaScript frame loop.
          </p>
          <div className="hero-actions">
            <a className="button primary" href="#demos">Open the 3D lab</a>
            <a className="button secondary" href="https://www.npmjs.com/package/realtime-manim">npm package</a>
          </div>
          <div className="install" aria-label="Install command"><span>$</span><code>{install}</code></div>
        </div>
        <aside className="hero-system" aria-label="Rendering architecture">
          <p>One retained scene</p>
          <ol>
            <li><span>01</span><b>JavaScript authoring</b><small>Typed mobjects, animation and signals</small></li>
            <li><span>02</span><b>Rust scene core</b><small>Deterministic evaluation and validation</small></li>
            <li><span>03</span><b>GPU output</b><small>WebGPU in browser · Metal on macOS</small></li>
          </ol>
          <a href="https://github.com/adamholter/realtime-manim-rs">Read the implementation →</a>
        </aside>
      </section>

      <DemoPlayground />

      <section className="section shell" id="quickstart">
        <div className="eyebrow">Quickstart</div>
        <div className="section-heading">
          <h2>One package. One retained scene.</h2>
          <p>Build the scene in JavaScript; let Rust evaluate and render it.</p>
        </div>
        <div className="code-card">
          <div className="code-bar"><span>animation.js</span><span className="dots">● ● ●</span></div>
          <pre><code>{quickstart}</code></pre>
        </div>
        <div className="note">
          Serve from localhost or HTTPS in a WebGPU-capable browser. The package ships
          its Wasm runtime; custom fonts can be preloaded before scene validation.
        </div>
      </section>

      <section className="section shell" id="capabilities">
        <div className="eyebrow">What ships</div>
        <div className="section-heading">
          <h2>A real general-purpose engine.</h2>
          <p>No canned sine-wave demo. The same scene IR drives browser and native paths.</p>
        </div>
        <div className="capability-grid">
          {capabilities.map(([title, copy], index) => (
            <article className="capability" key={title}>
              <span>{String(index + 1).padStart(2, "0")}</span>
              <h3>{title}</h3>
              <p>{copy}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="section shell agent-section" id="agents">
        <div className="agent-copy">
          <div className="eyebrow">Agent handoff</div>
          <h2>Give an agent the contract, not a scavenger hunt.</h2>
          <p>
            Point coding agents at this site’s machine-readable index. It links the public API,
            package, schema, compiler, examples, compatibility matrix and benchmark receipts.
          </p>
          <div className="agent-links">
            <a href="/llms.txt">Open llms.txt →</a>
            <a href="/agent-guide.md">Open agent guide →</a>
            <a href="https://github.com/adamholter/realtime-manim-rs/blob/main/site/public/agent-guide.md">Repository guide →</a>
          </div>
        </div>
        <div className="prompt-card">
          <div className="code-bar"><span>agent-prompt.txt</span><span>copy-ready</span></div>
          <pre><code>{agentPrompt}</code></pre>
        </div>
      </section>

      <section className="section shell honesty">
        <div>
          <div className="eyebrow">Compatibility posture</div>
          <h2>Fast, broad, and honest about the frontier.</h2>
        </div>
        <p>
          This is not yet arbitrary-Python drop-in parity with every Manim Community extension.
          Unsupported features reject explicitly instead of silently flattening quality. The public
          capability matrix and benchmark receipts show what is native, compiled, partial, or open.
        </p>
        <a className="button secondary" href="https://github.com/adamholter/realtime-manim-rs/blob/main/benchmarks/corpus/capability-matrix.json">Read the matrix</a>
      </section>

      <footer className="footer shell">
        <div><span className="brand-mark" aria-hidden="true">rm</span><b>realtime-manim</b></div>
        <p>MIT OR Apache-2.0 · Built in Rust · Published for agents and humans.</p>
        <div><a href="https://github.com/adamholter/realtime-manim-rs">GitHub</a><a href="https://www.npmjs.com/package/realtime-manim">npm</a></div>
      </footer>
    </main>
  );
}
