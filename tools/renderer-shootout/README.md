# Experimental renderer shootout

Ticket P-10 compares Vello 0.9 with the current retained lyon + wgpu renderer. It does not select or freeze the production architecture before D-20.

Native Metal evidence:

```bash
cargo run --release -p realtime-manim-renderer-shootout -- \
  --output benchmarks/renderer-shootout/latest \
  --width 1280 --height 720 --warmup 12 --iterations 60 \
  --background-load interactive-apps-open
```

The command renders five equivalent vector workloads, waits for GPU completion on every measured frame, reads back both targets, writes PNGs and image-diff metrics, and emits one schema-valid receipt per backend/workload.

Browser Vello proof:

```bash
cargo build --release -p realtime-manim-renderer-shootout --lib \
  --target wasm32-unknown-unknown --no-default-features --features browser-vello
wasm-bindgen target/wasm32-unknown-unknown/release/realtime_manim_renderer_shootout.wasm \
  --target web --out-dir tools/renderer-shootout/web/pkg --out-name vello_shootout
cargo build --release -p realtime-manim-renderer-shootout --lib \
  --target wasm32-unknown-unknown --no-default-features --features browser-lyon
wasm-bindgen target/wasm32-unknown-unknown/release/realtime_manim_renderer_shootout.wasm \
  --target web --out-dir tools/renderer-shootout/web/pkg --out-name lyon_shootout
python3 -m http.server 8932 --directory tools/renderer-shootout/web
```

Open `http://127.0.0.1:8932/` in a WebGPU browser. The page exposes `window.__SHOOTOUT_RESULT__` only after a non-empty GPU-rendered frame is read back.

Limitations are deliberate and explicit: the native workload suite covers retained solid vector fill/stroke/blend pressure, not every feature in the Manim corpus; Vello and lyon use their recommended Area and 4× MSAA modes respectively; and the native processes use different wgpu major versions because Vello 0.9 pins wgpu 29 while realtime-manim uses wgpu 30.
