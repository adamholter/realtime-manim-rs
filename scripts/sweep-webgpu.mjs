#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { chromium } from "playwright-core";

const root = resolve(import.meta.dirname, "..");
const endpoint = process.env.REALTIME_MANIM_URL || "http://127.0.0.1:8917";
const chromePath =
  process.env.CHROME_PATH ||
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const outputArtifact =
  process.env.SWEEP_OUTPUT || "benchmarks/compat/full-corpus/browser-sweep.json";
const outputPath = resolve(root, outputArtifact);
const manifest = JSON.parse(
  await readFile(resolve(root, "benchmarks/corpus/manifest.json"), "utf8"),
);
const environmentManifest = JSON.parse(
  await readFile(resolve(root, "benchmarks/environment/current.json"), "utf8"),
);
const revision = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
const dirty = execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim().length > 0;

let activeScene = "startup";
const errors = [];
const browser = await chromium.launch({
  headless: true,
  executablePath: chromePath,
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan,UseSkiaRenderer"],
});

try {
  const page = await browser.newPage({ viewport: { width: 900, height: 540 } });
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      errors.push({ scene: activeScene, source: `console:${message.type()}`, message: message.text() });
    }
  });
  page.on("pageerror", (error) => {
    errors.push({ scene: activeScene, source: "pageerror", message: error.message });
  });

  await page.goto(endpoint, { waitUntil: "networkidle" });
  await page.waitForFunction(
    () => document.querySelector("#render-status")?.textContent?.includes("Running live preview"),
    null,
    { timeout: 30_000 },
  );
  await page.evaluate(() => {
    const canvas = document.querySelector("#viewport");
    canvas.style.cssText =
      "position:fixed;left:0;top:0;width:854px;height:480px;z-index:99999";
    document.querySelector(".preview-label").style.display = "none";
  });
  await page.waitForFunction(() => {
    const canvas = document.querySelector("#viewport");
    return canvas.width === 854 && canvas.height === 480;
  });

  const sweepStartedAt = performance.now();
  const results = [];
  for (const entry of manifest.scenes) {
    activeScene = entry.class;
    const errorStart = errors.length;
    const scene = JSON.parse(
      await readFile(
        resolve(root, "benchmarks/compat/full-corpus", `${entry.class}.json`),
        "utf8",
      ),
    );
    await page.evaluate(async (candidate) => {
      const app = await import("/app.js");
      await app.runScene(candidate, { record: false });
    }, scene);

    const frames = [];
    for (const fraction of [0.17, 0.53, 0.89]) {
      const at = scene.duration * fraction;
      await page.evaluate(async (time) => {
        const timeline = document.querySelector("#timeline-control");
        if (!(timeline instanceof HTMLInputElement)) throw new Error("Timeline control is unavailable.");
        timeline.value = String(time);
        timeline.dispatchEvent(new Event("input", { bubbles: true }));
        await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
        await new Promise((resolveFrame) => requestAnimationFrame(resolveFrame));
      }, at);
      const presentedCanvasReadbackCostMs = await page.evaluate(async () => {
        const source = document.querySelector("#viewport");
        if (!(source instanceof HTMLCanvasElement)) throw new Error("Preview canvas is unavailable.");
        const target = document.createElement("canvas");
        target.width = source.width;
        target.height = source.height;
        const context = target.getContext("2d", { willReadFrequently: true });
        const samples = [];
        for (let index = 0; index < 5; index += 1) {
          const started = performance.now();
          context.drawImage(source, 0, 0);
          context.getImageData(0, 0, 1, 1);
          samples.push(performance.now() - started);
        }
        return samples;
      });
      const png = await page.locator("#viewport").screenshot();
      const sample = await page.evaluate(async (pngUrl) => {
        const source = new Image();
        source.src = pngUrl;
        await source.decode();
        const target = document.createElement("canvas");
        target.width = 128;
        target.height = 72;
        const context = target.getContext("2d", { willReadFrequently: true });
        context.drawImage(source, 0, 0, target.width, target.height);
        const pixels = context.getImageData(0, 0, target.width, target.height).data;
        const background = [pixels[0], pixels[1], pixels[2], pixels[3]];
        let occupied = 0;
        const colorBuckets = new Set();
        for (let offset = 0; offset < pixels.length; offset += 4) {
          colorBuckets.add(
            `${pixels[offset] >> 4},${pixels[offset + 1] >> 4},${pixels[offset + 2] >> 4},${pixels[offset + 3] >> 4}`,
          );
          if (
            Math.abs(pixels[offset] - background[0]) > 8 ||
            Math.abs(pixels[offset + 1] - background[1]) > 8 ||
            Math.abs(pixels[offset + 2] - background[2]) > 8 ||
            Math.abs(pixels[offset + 3] - background[3]) > 8
          ) {
            occupied += 1;
          }
        }
        return {
          occupiedSamplePixels: occupied,
          sampleBackground: background,
          distinctColorBuckets: colorBuckets.size,
        };
      }, `data:image/png;base64,${png.toString("base64")}`);
      frames.push({
        at,
        presentedCanvasReadbackCostMs,
        ...sample,
        pngBytes: png.byteLength,
        sha256: createHash("sha256").update(png).digest("hex"),
      });
    }
    if (frames.every((frame) => frame.occupiedSamplePixels === 0)) {
      errors.push({ scene: activeScene, source: "blank-frame", message: "All three sampled frames matched the background." });
    }
    results.push({
      scene: entry.class,
      errors: errors.length - errorStart,
      frames,
    });
  }

  const readbackCosts = results.flatMap((result) => result.frames.flatMap((frame) => frame.presentedCanvasReadbackCostMs));
  const sortedReadbackCosts = [...readbackCosts].sort((left, right) => left - right);
  const percentile = (fraction) => sortedReadbackCosts[Math.min(
    sortedReadbackCosts.length - 1,
    Math.max(0, Math.ceil(sortedReadbackCosts.length * fraction) - 1),
  )];
  const sweepDurationSeconds = (performance.now() - sweepStartedAt) / 1000;
  const report = {
    at: new Date().toISOString(),
    endpoint,
    scenes: results.length,
    seeks: results.reduce((sum, result) => sum + result.frames.length, 0),
    durationSeconds: sweepDurationSeconds,
    presentedCanvasReadbackCostMs: {
      mean: readbackCosts.reduce((sum, value) => sum + value, 0) / readbackCosts.length,
      p50: percentile(0.5),
      p95: percentile(0.95),
      max: Math.max(...readbackCosts),
    },
    errors,
    results,
  };
  await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  const receipt = {
    schema_version: 1,
    benchmark_id: "full-corpus-webgpu-seek-v1",
    recorded_at_utc: report.at,
    environment: {
      os: environmentManifest.os.family,
      os_version: environmentManifest.os.version,
      architecture: environmentManifest.os.architecture,
      processor: environmentManifest.hardware.chip,
      memory_bytes: environmentManifest.hardware.memory_bytes,
      display: {
        width: 900,
        height: 540,
        refresh_hz: 60,
        device_pixel_ratio_milli: 1000,
      },
      background_load_profile: environmentManifest.background_load_profile,
    },
    workload: {
      name: "Full Manim compatibility corpus random seeks",
      scene: "benchmarks/compat/full-corpus/*.json",
      resolution: [854, 480],
      duration_seconds: sweepDurationSeconds,
      notes: [
        `${report.scenes} scenes and ${report.seeks} explicit-time seeks`,
        "Each readback-cost sample draws the presented WebGPU canvas and synchronously reads one pixel.",
        "Headless Chrome WebGPU; background work was not suspended.",
      ],
    },
    samples: [
      { name: "presented_canvas_readback_cost", unit: "ms", statistic: "mean", value: report.presentedCanvasReadbackCostMs.mean },
      { name: "presented_canvas_readback_cost", unit: "ms", statistic: "p50", value: report.presentedCanvasReadbackCostMs.p50 },
      { name: "presented_canvas_readback_cost", unit: "ms", statistic: "p95", value: report.presentedCanvasReadbackCostMs.p95 },
      { name: "presented_canvas_readback_cost", unit: "ms", statistic: "max", value: report.presentedCanvasReadbackCostMs.max },
      { name: "browser_errors", unit: "count", statistic: "total", value: errors.length },
      { name: "distinct_frame_hashes", unit: "count", statistic: "total", value: new Set(results.flatMap((result) => result.frames.map((frame) => frame.sha256))).size },
    ],
    provenance: {
      revision,
      dirty,
      commands: ["npm run sweep:webgpu"],
      artifacts: [outputArtifact, `${outputArtifact}.receipt.json`],
    },
  };
  await writeFile(`${outputPath}.receipt.json`, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(
    `WebGPU sweep: ${report.scenes} scenes, ${report.seeks} seeks, ${errors.length} errors, ` +
      `${report.presentedCanvasReadbackCostMs.p50.toFixed(1)} ms p50 / ${report.presentedCanvasReadbackCostMs.p95.toFixed(1)} ms p95 readback`,
  );
  if (errors.length > 0) {
    console.error(JSON.stringify(errors, null, 2));
    process.exitCode = 1;
  }
} finally {
  await browser.close();
}
