#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { arch, cpus, platform, totalmem } from "node:os";
import { dirname, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { gunzipSync } from "node:zlib";

import { chromium } from "playwright-core";

const ROOT = resolve(import.meta.dirname, "..");
const DEFAULT_SCENES = [
  "VectorFieldAndStreamLines",
  "TextAndMath",
  "ThreeDSurface",
  "PolyhedraAndFixedLabels",
];

async function readSceneFixture(name) {
  const compressed = await readFile(
    resolve(ROOT, "benchmarks", "runtime", "fixtures", `${name}.json.gz`),
  );
  return JSON.parse(gunzipSync(compressed).toString("utf8"));
}

function parseArgs(argv) {
  const options = {
    endpoint: process.env.REALTIME_MANIM_URL || "http://127.0.0.1:8917",
    output: null,
    receipt: null,
    phase: "measurement",
    sampleMs: 3_000,
    memorySweeps: 1_000,
    minPlaybackFps: 55,
    minLargeSceneSeekFps: 30,
    maxMemoryGrowthPercent: 5,
    backgroundLoad: "interactive-apps-open-isolated-headless-chrome",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const value = () => argv[++index] ?? (() => { throw new Error(`${argument} requires a value.`); })();
    if (argument === "--endpoint") options.endpoint = value();
    else if (argument === "--output") options.output = resolve(value());
    else if (argument === "--receipt") options.receipt = resolve(value());
    else if (argument === "--phase") options.phase = value();
    else if (argument === "--sample-ms") options.sampleMs = Number(value());
    else if (argument === "--memory-sweeps") options.memorySweeps = Number(value());
    else if (argument === "--min-playback-fps") options.minPlaybackFps = Number(value());
    else if (argument === "--min-large-scene-seek-fps") options.minLargeSceneSeekFps = Number(value());
    else if (argument === "--max-memory-growth-percent") options.maxMemoryGrowthPercent = Number(value());
    else if (argument === "--background-load") options.backgroundLoad = value();
    else throw new Error(`Unknown argument: ${argument}`);
  }
  if (!Number.isInteger(options.sampleMs) || options.sampleMs < 500 || options.sampleMs > 60_000) {
    throw new Error("--sample-ms must be an integer from 500 to 60000.");
  }
  if (!Number.isInteger(options.memorySweeps) || options.memorySweeps < 0 || options.memorySweeps > 10_000) {
    throw new Error("--memory-sweeps must be an integer from 0 to 10000.");
  }
  for (const [name, number] of [
    ["--min-playback-fps", options.minPlaybackFps],
    ["--min-large-scene-seek-fps", options.minLargeSceneSeekFps],
    ["--max-memory-growth-percent", options.maxMemoryGrowthPercent],
  ]) {
    if (!Number.isFinite(number) || number < 0) throw new Error(`${name} must be a finite nonnegative number.`);
  }
  return options;
}

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)];
}

function cdpMetric(payload, name) {
  const value = payload.metrics.find((entry) => entry.name === name)?.value;
  if (!Number.isFinite(value)) throw new Error(`Chrome did not expose ${name}.`);
  return value;
}

function command(command, args) {
  return execFileSync(command, args, { cwd: ROOT, encoding: "utf8" }).trim();
}

function shellQuote(value) {
  return /^[A-Za-z0-9_./:=+-]+$/.test(value)
    ? value
    : `'${value.replaceAll("'", `'"'"'`)}'`;
}

function osVersion() {
  return platform() === "darwin" ? command("sw_vers", ["-productVersion"]) : command("uname", ["-r"]);
}

async function waitForFrames(page, count) {
  await page.evaluate(async (frameCount) => {
    for (let index = 0; index < frameCount; index += 1) {
      await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
    }
  }, count);
}

const options = parseArgs(process.argv.slice(2));
const chromePath = process.env.CHROME_PATH || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const browser = await chromium.launch({
  headless: true,
  executablePath: chromePath,
  args: [
    "--enable-unsafe-webgpu",
    "--enable-features=Vulkan,UseSkiaRenderer",
    "--js-flags=--expose-gc",
  ],
});
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const cdp = await page.context().newCDPSession(page);
await cdp.send("Performance.enable");
const errors = [];
page.on("pageerror", (error) => errors.push(`page:${error.message}`));
page.on("console", (message) => {
  if (message.type() === "error") errors.push(`console:${message.text()}`);
});

const benchmarkStarted = performance.now();
try {
  await page.goto(options.endpoint, { waitUntil: "networkidle" });
  await page.waitForFunction(
    () => document.querySelector("#render-status")?.textContent?.includes("Running live preview"),
    null,
    { timeout: 30_000 },
  );

  const results = [];
  for (const sceneName of DEFAULT_SCENES) {
    const scene = await readSceneFixture(sceneName);
    const errorStart = errors.length;
    const firstFrameMs = await page.evaluate(async (candidate) => {
      const app = await import("/app.js");
      const started = performance.now();
      await app.runScene(candidate, { record: false });
      await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      return performance.now() - started;
    }, scene);

    const playbackDiagnosticsBefore = await page.evaluate(async () => (await import("/app.js")).previewDiagnostics());
    const playbackMetricsBefore = await cdp.send("Performance.getMetrics");
    await page.waitForTimeout(options.sampleMs);
    const playbackMetricsAfter = await cdp.send("Performance.getMetrics");
    const playbackDiagnosticsAfter = await page.evaluate(async () => (await import("/app.js")).previewDiagnostics());
    const playbackPresentedFrames = playbackDiagnosticsAfter.presentedFrames - playbackDiagnosticsBefore.presentedFrames;

    await page.evaluate(async () => {
      const app = await import("/app.js");
      app.seekPreview(1.25);
      await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
    });
    const pausedDiagnosticsBefore = await page.evaluate(async () => (await import("/app.js")).previewDiagnostics());
    const pausedMetricsBefore = await cdp.send("Performance.getMetrics");
    await page.waitForTimeout(options.sampleMs);
    const pausedMetricsAfter = await cdp.send("Performance.getMetrics");
    const pausedDiagnosticsAfter = await page.evaluate(async () => (await import("/app.js")).previewDiagnostics());

    const seekStarted = performance.now();
    for (let index = 0; index < 120; index += 1) {
      await page.evaluate(async (time) => {
        const app = await import("/app.js");
        app.seekPreview(time);
        await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      }, (index % 60) / 20);
    }
    const seek120ElapsedMs = performance.now() - seekStarted;

    const captureAt = async (time) => {
      await page.evaluate(async (at) => {
        const app = await import("/app.js");
        app.seekPreview(at);
        await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
        await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      }, time);
      return page.locator("#viewport").screenshot();
    };
    const first = await captureAt(1.1);
    await captureAt(2.2);
    await captureAt(0.4);
    const repeat = await captureAt(1.1);
    const hashA = createHash("sha256").update(first).digest("hex");
    const hashB = createHash("sha256").update(repeat).digest("hex");

    const presentedReadbackMs = await page.evaluate(async () => {
      const source = document.querySelector("#viewport");
      const target = document.createElement("canvas");
      target.width = source.width;
      target.height = source.height;
      const context = target.getContext("2d", { willReadFrequently: true });
      const samples = [];
      for (let index = 0; index < 20; index += 1) {
        const started = performance.now();
        context.drawImage(source, 0, 0);
        context.getImageData(0, 0, 1, 1);
        samples.push(performance.now() - started);
        await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      }
      return samples;
    });

    results.push({
      scene: sceneName,
      sourceBytes: Buffer.byteLength(JSON.stringify(scene)),
      nodes: scene.nodes.length,
      tracks: scene.tracks.length,
      firstFrameMs,
      playbackFps: playbackPresentedFrames * 1_000 / options.sampleMs,
      playbackPresentedFrames,
      playbackCpuMs: (
        cdpMetric(playbackMetricsAfter, "TaskDuration") - cdpMetric(playbackMetricsBefore, "TaskDuration")
      ) * 1_000,
      lastCpuFrameMs: playbackDiagnosticsAfter.lastCpuFrameMs,
      pausedPresentedFrames: pausedDiagnosticsAfter.presentedFrames - pausedDiagnosticsBefore.presentedFrames,
      pausedSkippedFrames: pausedDiagnosticsAfter.skippedFrames - pausedDiagnosticsBefore.skippedFrames,
      pausedCpuMs: (
        cdpMetric(pausedMetricsAfter, "TaskDuration") - cdpMetric(pausedMetricsBefore, "TaskDuration")
      ) * 1_000,
      seek120ElapsedMs,
      seekThroughputPerSecond: 120_000 / seek120ElapsedMs,
      deterministic: hashA === hashB,
      deterministicHash: hashA,
      presentedReadbackMs,
      presentedReadbackP50Ms: percentile(presentedReadbackMs, 0.5),
      presentedReadbackP95Ms: percentile(presentedReadbackMs, 0.95),
      jsHeapUsedBytes: cdpMetric(await cdp.send("Performance.getMetrics"), "JSHeapUsedSize"),
      errors: errors.slice(errorStart),
    });
  }

  let memory = null;
  if (options.memorySweeps > 0) {
    const memoryScene = await readSceneFixture("ThreeDSurfaceCompact2");
    await page.evaluate(async (candidate) => {
      const app = await import("/app.js");
      await app.runScene(candidate, { record: false });
      app.seekPreview(0.5);
      await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      globalThis.gc?.();
      globalThis.gc?.();
    }, memoryScene);
    const warmupSweeps = Math.min(120, options.memorySweeps);
    for (let index = 0; index < warmupSweeps; index += 1) {
      await page.evaluate(async (time) => {
        const app = await import("/app.js");
        app.seekPreview(time);
        await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      }, (index % 120) / 30);
    }
    await page.evaluate(() => { globalThis.gc?.(); globalThis.gc?.(); });
    const before = await cdp.send("Performance.getMetrics");
    const beforeDom = await cdp.send("Memory.getDOMCounters");
    const beforeDiagnostics = await page.evaluate(async () => (await import("/app.js")).previewDiagnostics());
    const sweepStarted = performance.now();
    for (let index = 0; index < options.memorySweeps; index += 1) {
      await page.evaluate(async (time) => {
        const app = await import("/app.js");
        app.seekPreview(time);
        await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      }, (index % 120) / 30);
    }
    const sweepElapsedMs = performance.now() - sweepStarted;
    await waitForFrames(page, 2);
    await page.evaluate(() => { globalThis.gc?.(); globalThis.gc?.(); });
    const after = await cdp.send("Performance.getMetrics");
    const afterDom = await cdp.send("Memory.getDOMCounters");
    const afterDiagnostics = await page.evaluate(async () => (await import("/app.js")).previewDiagnostics());
    const beforeBytes = cdpMetric(before, "JSHeapUsedSize");
    const afterBytes = cdpMetric(after, "JSHeapUsedSize");
    memory = {
      scene: "benchmarks/runtime/fixtures/ThreeDSurfaceCompact2.json.gz",
      sourceBytes: Buffer.byteLength(JSON.stringify(memoryScene)),
      nodes: memoryScene.nodes.length,
      tracks: memoryScene.tracks.length,
      warmupSweeps,
      sweeps: options.memorySweeps,
      elapsedMs: sweepElapsedMs,
      throughputPerSecond: options.memorySweeps * 1_000 / sweepElapsedMs,
      presentedFrames: afterDiagnostics.presentedFrames - beforeDiagnostics.presentedFrames,
      jsHeapBeforeBytes: beforeBytes,
      jsHeapAfterBytes: afterBytes,
      jsHeapGrowthBytes: afterBytes - beforeBytes,
      jsHeapGrowthPercent: (afterBytes - beforeBytes) * 100 / beforeBytes,
      domNodeGrowth: afterDom.nodes - beforeDom.nodes,
      eventListenerGrowth: afterDom.jsEventListeners - beforeDom.jsEventListeners,
      note: "Chrome exposes JS heap and DOM counters here; WebAssembly linear memory and GPU-resident bytes are not separately observable from this page.",
    };
  }

  const revision = command("git", ["rev-parse", "HEAD"]);
  const dirty = command("git", ["status", "--porcelain"]).length > 0;
  const elapsedSeconds = (performance.now() - benchmarkStarted) / 1_000;
  const failures = [];
  for (const result of results) {
    if (result.playbackFps < options.minPlaybackFps) {
      failures.push(`${result.scene} playback ${result.playbackFps.toFixed(2)} fps < ${options.minPlaybackFps} fps`);
    }
    if (result.pausedPresentedFrames !== 0) {
      failures.push(`${result.scene} presented ${result.pausedPresentedFrames} settled paused frames`);
    }
    if (!result.deterministic) failures.push(`${result.scene} fixed-time SHA-256 mismatch`);
  }
  if (memory?.throughputPerSecond < options.minLargeSceneSeekFps) {
    failures.push(`large-scene seek ${memory.throughputPerSecond.toFixed(2)} fps < ${options.minLargeSceneSeekFps} fps`);
  }
  if (memory?.jsHeapGrowthPercent > options.maxMemoryGrowthPercent) {
    failures.push(`large-scene JS heap grew ${memory.jsHeapGrowthPercent.toFixed(3)}% > ${options.maxMemoryGrowthPercent}%`);
  }
  const report = {
    schemaVersion: 1,
    phase: options.phase,
    recordedAt: new Date().toISOString(),
    endpoint: options.endpoint,
    browser: browser.version(),
    viewport: [1440, 900],
    sampleMs: options.sampleMs,
    results,
    memory,
    limitations: [
      "Browser WebGPU timestamp queries are not enabled, so GPU-only frame time is not reported.",
      "Presented-canvas readback includes synchronization and CPU copy cost; it is not labeled as GPU-only time.",
      "Chrome exposes JS heap here, not total WebAssembly or GPU-resident memory.",
    ],
    acceptance: {
      minPlaybackFps: options.minPlaybackFps,
      minLargeSceneSeekFps: options.minLargeSceneSeekFps,
      maxMemoryGrowthPercent: options.maxMemoryGrowthPercent,
      failures,
      passed: failures.length === 0,
    },
    errors,
  };

  const samples = [];
  for (const result of results) {
    const prefix = result.scene;
    samples.push(
      { name: `${prefix}.first_frame_time`, unit: "ms", statistic: "single", value: result.firstFrameMs },
      { name: `${prefix}.playback_frame_rate`, unit: "fps", statistic: "mean", value: result.playbackFps },
      { name: `${prefix}.playback_cpu_time`, unit: "ms", statistic: `total_${options.sampleMs}ms`, value: result.playbackCpuMs },
      { name: `${prefix}.paused_cpu_time`, unit: "ms", statistic: `total_${options.sampleMs}ms`, value: result.pausedCpuMs },
      { name: `${prefix}.paused_presented_frames`, unit: "frames", statistic: `total_${options.sampleMs}ms`, value: result.pausedPresentedFrames },
      { name: `${prefix}.seek_throughput`, unit: "frames/s", statistic: "120_seeks", value: result.seekThroughputPerSecond },
      { name: `${prefix}.presented_readback_time`, unit: "ms", statistic: "p50", value: result.presentedReadbackP50Ms },
      { name: `${prefix}.presented_readback_time`, unit: "ms", statistic: "p95", value: result.presentedReadbackP95Ms },
      { name: `${prefix}.fixed_time_determinism`, unit: "match", statistic: "sha256_repeat", value: result.deterministic ? 1 : 0 },
    );
  }
  if (memory) {
    samples.push(
      { name: "large_scene.seek_throughput", unit: "frames/s", statistic: `${memory.sweeps}_seeks`, value: memory.throughputPerSecond },
      { name: "large_scene.js_heap_growth", unit: "bytes", statistic: `${memory.sweeps}_seeks`, value: memory.jsHeapGrowthBytes },
      { name: "large_scene.dom_node_growth", unit: "nodes", statistic: `${memory.sweeps}_seeks`, value: memory.domNodeGrowth },
      { name: "large_scene.event_listener_growth", unit: "listeners", statistic: `${memory.sweeps}_seeks`, value: memory.eventListenerGrowth },
    );
  }
  const receipt = {
    schema_version: 1,
    benchmark_id: `P-12-heavy-scenes-${options.phase}`,
    recorded_at_utc: report.recordedAt,
    environment: {
      os: platform() === "darwin" ? "macOS" : platform(),
      os_version: osVersion(),
      architecture: arch(),
      processor: cpus()[0]?.model || "unknown",
      memory_bytes: totalmem(),
      display: { width: 1440, height: 900, refresh_hz: 60, device_pixel_ratio_milli: 1_000 },
      background_load_profile: options.backgroundLoad,
    },
    workload: {
      name: `WebGPU heavy-scene runtime ${options.phase}`,
      scene: "benchmarks/runtime/fixtures/*.json.gz",
      resolution: [1440, 900],
      duration_seconds: elapsedSeconds,
      notes: [...report.limitations, `Browser: ${report.browser}`],
    },
    samples,
    provenance: {
      revision,
      dirty,
      commands: [
        ["node", "scripts/benchmark-heavy-scenes.mjs", ...process.argv.slice(2)]
          .map(shellQuote)
          .join(" "),
      ],
      artifacts: [options.output, options.receipt]
        .filter(Boolean)
        .map((path) => path.startsWith(ROOT) ? path.slice(ROOT.length + 1) : path),
    },
  };

  if (options.output) {
    await mkdir(dirname(options.output), { recursive: true });
    await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (options.receipt) {
    await mkdir(dirname(options.receipt), { recursive: true });
    await writeFile(options.receipt, `${JSON.stringify(receipt, null, 2)}\n`);
  }
  process.stdout.write(`${JSON.stringify({
    phase: options.phase,
    results: results.map(({ scene, firstFrameMs, playbackFps, playbackCpuMs, pausedCpuMs, pausedPresentedFrames, seekThroughputPerSecond, deterministic }) => ({
      scene, firstFrameMs, playbackFps, playbackCpuMs, pausedCpuMs, pausedPresentedFrames, seekThroughputPerSecond, deterministic,
    })),
    memory,
    failures,
    errors,
  })}\n`);
  if (errors.length || failures.length) {
    process.exitCode = 1;
  }
} finally {
  await browser.close();
}
