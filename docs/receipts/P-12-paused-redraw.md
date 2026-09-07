# P-12 tranche — measured paused-frame suppression

Recorded: 2026-08-30

Measurement note, 2026-09-07: the historical playback FPS figures below used the
requested sample duration as their denominator. The benchmark now uses measured
elapsed time and records CPU sample durations separately. Historical numbers are
preserved here; see `2026-09-07-review.md` for the correction and fresh evidence.

Revision measured before the change: `116fc1c`

## Decision

The dominant verified runtime defect was identical-frame work while paused or at an
explicit seek time. Both browser and native previews reevaluated the scene, rebuilt
geometry, uploaded buffers, submitted GPU work, and presented again even though scene
time had not changed.

The committed-package probe made the defect unambiguous on the 9.5 MiB compact 3D
surface: a paused player presented 301 unchanged frames in 5.0065 seconds and spent
1,060.267 ms of renderer-main CPU time, or 3.5225 ms per animation callback.

The Python compiler's 3D surface capture is also expensive and remains the next
measured optimization target. It was not mixed into this renderer tranche.

## Change

- Browser players keep a lightweight animation callback for resize and device-loss
  detection, but skip evaluation, tessellation, upload, submission, and presentation
  when paused/manual-time state is clean.
- Scene load, seek, signal, font registration, resize, reset, pause transition, and
  device recovery each dirty the frame and therefore still present once.
- Native preview uses an event-driven wait loop. It schedules continuously only while
  playing and redraws once for startup, resize, seek, restart, or pause/resume input.
- Package diagnostics add cumulative presented/skipped frame counts and the last CPU
  frame time. Existing methods and scene semantics are unchanged.

## Browser before and after

Each CPU value is Chrome renderer-main task time during the same three-second paused
sample. The remaining after value is browser/application idle work, not scene renders;
the presented-frame counter stayed at zero for every settled paused sample.

| Scene | Paused CPU before | Paused CPU after | Reduction | Paused presents after | Playback after |
| --- | ---: | ---: | ---: | ---: | ---: |
| VectorFieldAndStreamLines | 175.727 ms | 101.649 ms | 42.2% | 0 | 60.67 fps |
| TextAndMath | 163.407 ms | 116.969 ms | 28.4% | 0 | 61.00 fps |
| ThreeDSurface | 523.460 ms | 78.078 ms | 85.1% | 0 | 60.33 fps |
| PolyhedraAndFixedLabels | 1,355.844 ms | 85.588 ms | 93.7% | 0 | 60.33 fps |

Interactive seek throughput on `PolyhedraAndFixedLabels` rose from 25.40 to 54.52
frames/s. The other three scenes remained at approximately 60 seeks/s.

All four fixed-time canvas SHA-256 hashes matched their pre-change values exactly:

- `VectorFieldAndStreamLines`: `a2e47949dddba0f70bfdc947b1d32f1570091238232c59033a3c73226e12c807`
- `TextAndMath`: `50d31bea740f32b5a963f50a396a8140ad203cb8b1d30b7d436f5ff52427a3bf`
- `ThreeDSurface`: `719066b27dcc599fb5d7cb48fcb37b453ba9ffad858dc8b1356d1046d49e8f6b`
- `PolyhedraAndFixedLabels`: `27a991625d10b8a613d8e91746ef341f7e73e3ad814d29af2cc1977be8661bc6`

## Large-scene memory and throughput

The 9,816,950-byte `ThreeDSurfaceCompact2.json.gz` workload expands to a scene with 669 nodes and
2,983 tracks. After 120 warm-up seeks, 1,000 measured explicit-time seeks produced
exactly 1,000 frames in 16.730 seconds (59.77 frames/s).

- garbage-collected JS heap: 83,458,904 → 83,462,540 bytes
- growth: 3,636 bytes, or 0.0044%
- DOM node growth: 0
- event-listener growth: 0

Chrome did not expose the page's WebAssembly linear-memory or GPU-resident byte totals.
An independent alternating-load probe observed the compact runtime's Wasm memory stay
at exactly 9,043,968 bytes across 80 heavy-scene loads.

## Compile, first-frame, native, and GPU baselines

- Rust native-preview build: 67.89 s stale-target rebuild; 0.24 s no-op rebuild.
- `VectorFieldAndStreamLines` Python compile at 15 fps: 8.51 s then 5.71 s;
  500,139-byte outputs were byte-identical.
- `ThreeDSurface` Python compile at 15 fps: 20.96 s and 386,891,776-byte max RSS.
- Strict native 1280×720 first render process: 1.76 s cold driver-cache process,
  0.21 s warm process; both PNGs were byte-identical.
- Compact browser 3D surface: 8.6 ms load, 16.9 ms first animation frame, 59.97 fps.
- Experimental Lyon/Metal p95 frame completion, including CPU encoding, GPU submit,
  and queue-completion wait: 0.4074–0.9217 ms across the five existing workloads.

The production WebGPU adapters request no timestamp-query feature and use no timestamp
writes, so separate GPU-only frame time is not observable. Presented-canvas readback
and queue-completion measurements are labeled as combined measurements instead.

## Reproduction and artifacts

```sh
npm run benchmark:heavy-scenes -- \
  --phase after-paused-redraw \
  --sample-ms 3000 \
  --memory-sweeps 1000 \
  --output benchmarks/runtime/2026-08-30-paused-redraw-after.json \
  --receipt benchmarks/runtime/2026-08-30-paused-redraw-after.receipt.json
```

- `benchmarks/runtime/2026-08-30-paused-redraw-before.json`
- `benchmarks/runtime/2026-08-30-paused-heavy-before.json`
- `benchmarks/runtime/2026-08-30-paused-redraw-before.receipt.json`
- `benchmarks/runtime/2026-08-30-paused-redraw-after.json`
- `benchmarks/runtime/2026-08-30-paused-redraw-after.receipt.json`
- `benchmarks/runtime/2026-08-30-compile-native-baseline.receipt.json`
- `benchmarks/runtime/fixtures/*.json.gz`

The benchmark-schema crate parses and validates all three schema-v1 receipts in its
test suite. The environment manifest was refreshed from the current host instead of
copying the stale July OS snapshot.

## Remaining measured target

`ThreeDSurface` compiler profiling attributed 26.786 seconds to
`_capture_cairo_surface`; repeated NumPy cross products, color conversion, and
all-close checks dominate that path. A later tranche should bypass expanded per-frame
mesh/wire construction when compact retained surface output is valid, with reference
frame and byte-determinism gates.
