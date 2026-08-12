#!/usr/bin/env node

import { chromium } from "playwright-core";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { arch, cpus, platform, release, totalmem } from "node:os";
import { dirname, relative, resolve } from "node:path";

const url = process.env.RENDERER_SHOOTOUT_URL ?? "http://127.0.0.1:8932/";
const output = resolve(
  process.env.RENDERER_SHOOTOUT_OUTPUT
    ?? "benchmarks/renderer-shootout/2026-08-12-metal/browser-comparison-proof.png",
);
const evidencePath = output.replace(/\.png$/u, ".json");
const publicOutput = relative(process.cwd(), output);
const browser = await chromium.launch({
  headless: true,
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan,UseSkiaRenderer"],
});
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
const errors = [];
page.on("console", (message) => {
  if (message.type() === "warning" || message.type() === "error") {
    errors.push(`${message.type()}: ${message.text()}`);
  }
});
page.on("pageerror", (error) => errors.push(`pageerror: ${error.message}`));

try {
  await page.goto(url, { waitUntil: "networkidle", timeout: 30_000 });
  await page.waitForFunction(
    () => globalThis.__SHOOTOUT_RESULT__ || globalThis.__SHOOTOUT_ERROR__,
    null,
    { timeout: 120_000 },
  );
  const state = await page.evaluate(() => ({
    result: globalThis.__SHOOTOUT_RESULT__ ?? null,
    error: globalThis.__SHOOTOUT_ERROR__ ?? null,
  }));
  if (state.error) throw new Error(state.error);
  if (!state.result) throw new Error("Browser probe produced no result");
  const expectedBackends = {
    vello: "vello-0.9/WebGPU",
    lyon: "lyon-wgpu-30/WebGPU",
  };
  for (const [name, expectedBackend] of Object.entries(expectedBackends)) {
    const backend = state.result[name];
    if (backend?.backend !== expectedBackend) {
      throw new Error(`Unexpected ${name} backend: ${backend?.backend}`);
    }
    if (backend.adapter_available !== true) {
      throw new Error(`${name} WebGPU adapter was not acquired`);
    }
    if (backend.shapes !== 1_248 || backend.iterations !== 40) {
      throw new Error(`Unexpected ${name} workload: ${JSON.stringify(backend)}`);
    }
    if (!(backend.content_pixels > 10_000)) {
      throw new Error(`${name} frame is blank or incomplete: ${backend.content_pixels}`);
    }
    for (const metric of ["frame_p50_ms", "frame_p95_ms", "frame_p99_ms", "frame_mean_ms"]) {
      if (!(Number.isFinite(backend[metric]) && backend[metric] > 0)) {
        throw new Error(`Invalid ${name} ${metric}: ${backend[metric]}`);
      }
    }
  }
  const contentRatio = state.result.vello.content_pixels / state.result.lyon.content_pixels;
  if (!(contentRatio > 0.85 && contentRatio < 1.15)) {
    throw new Error(`Backends did not render equivalent geometry footprints: ratio=${contentRatio}`);
  }
  const boundsDelta = state.result.vello.content_bounds.map(
    (value, index) => Math.abs(value - state.result.lyon.content_bounds[index]),
  );
  if (Math.max(...boundsDelta) > 2) {
    throw new Error(`Backends rendered displaced bounds: ${JSON.stringify(boundsDelta)}`);
  }
  const velloOccupancy = state.result.vello.occupancy_64x36;
  const lyonOccupancy = state.result.lyon.occupancy_64x36;
  if (velloOccupancy.length !== 64 * 36 || lyonOccupancy.length !== 64 * 36) {
    throw new Error("Backends did not return the 64x36 occupancy proof");
  }
  const occupancyDifference = velloOccupancy.reduce(
    (total, value, index) => total + Math.abs(value - lyonOccupancy[index]),
    0,
  );
  const occupancyMass = Math.max(
    velloOccupancy.reduce((total, value) => total + value, 0),
    lyonOccupancy.reduce((total, value) => total + value, 0),
  );
  const occupancyNormalizedError = occupancyDifference / occupancyMass;
  if (!(occupancyNormalizedError < 0.20)) {
    throw new Error(`Backends rendered different spatial occupancy: error=${occupancyNormalizedError}`);
  }
  if (errors.length > 0) {
    throw new Error(`Browser emitted errors: ${JSON.stringify(errors)}`);
  }
  await mkdir(dirname(output), { recursive: true });
  await page.screenshot({ path: output, fullPage: true });
  const evidence = {
    ok: true,
    url,
    result: state.result,
    consoleErrors: errors,
    screenshot: publicOutput,
  };
  await writeFile(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
  const revision = `${commandOutput("git", ["rev-parse", "--short=12", "HEAD"], "uncommitted")}+tree-${await sourceTreeHash()}`;
  const dirty = commandOutput("git", ["status", "--porcelain"], "dirty").length > 0;
  const environment = {
    os: platform() === "darwin" ? "macOS" : platform(),
    os_version: platform() === "darwin"
      ? commandOutput("sw_vers", ["-productVersion"], release())
      : release(),
    architecture: arch(),
    processor: cpus()[0]?.model || "unknown processor",
    memory_bytes: totalmem(),
    display: {
      width: 1280,
      height: 720,
      refresh_hz: 60,
      device_pixel_ratio_milli: 1000,
    },
    background_load_profile: "isolated-headless-chrome-with-interactive-apps-open",
  };
  for (const [name, backend] of Object.entries(state.result)) {
    const retainedBytes = backend.retained_encoding_bytes ?? backend.retained_geometry_bytes;
    const receipt = {
      schema_version: 1,
      benchmark_id: `P-10-browser-${name}-primitive-grid`,
      recorded_at_utc: new Date().toISOString(),
      environment,
      workload: {
        name: `${backend.backend} / primitive-grid`,
        scene: "tools/renderer-shootout/generated/primitive-grid",
        resolution: [backend.width, backend.height],
        duration_seconds: backend.frame_mean_ms * backend.iterations / 1000,
        notes: [
          "1,248 independent filled primitives in isolated headless Chrome WebGPU.",
          "Experimental architecture evidence; not a renderer selection.",
          "Frame samples include CPU encoding, GPU submission, and completion callback.",
        ],
      },
      samples: [
        { name: "frame_time", unit: "ms", statistic: "p50", value: backend.frame_p50_ms },
        { name: "frame_time", unit: "ms", statistic: "p95", value: backend.frame_p95_ms },
        { name: "frame_time", unit: "ms", statistic: "p99", value: backend.frame_p99_ms },
        { name: "frame_time", unit: "ms", statistic: "mean", value: backend.frame_mean_ms },
        { name: "retained_geometry", unit: "bytes", statistic: "single", value: retainedBytes },
        { name: "content_pixels", unit: "pixels", statistic: "single", value: backend.content_pixels },
        { name: "occupancy_normalized_error", unit: "ratio", statistic: "single", value: occupancyNormalizedError },
      ],
      provenance: {
        revision,
        dirty,
        commands: [
          "npm run test:renderer-shootout-browser",
        ],
        artifacts: [publicOutput, relative(process.cwd(), evidencePath)],
      },
    };
    const receiptPath = resolve(dirname(output), `browser-${name}.receipt.json`);
    await writeFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`);
  }
  console.log(JSON.stringify(evidence));
} finally {
  await browser.close();
}

function commandOutput(program, commandArguments, fallback) {
  try {
    return execFileSync(program, commandArguments, { encoding: "utf8" }).trim();
  } catch {
    return fallback;
  }
}

async function sourceTreeHash() {
  const files = [
    "Cargo.toml",
    "Cargo.lock",
    "crates/benchmark-schema/Cargo.toml",
    "crates/benchmark-schema/src/lib.rs",
    "tools/renderer-shootout/Cargo.toml",
    "tools/renderer-shootout/src/lib.rs",
    "tools/renderer-shootout/src/main.rs",
    "tools/renderer-shootout/src/shootout.wgsl",
    "tools/renderer-shootout/web/index.html",
    "scripts/test-renderer-shootout-browser.mjs",
  ];
  const digest = createHash("sha256");
  for (const file of files) {
    digest.update(file);
    digest.update("\0");
    digest.update(await readFile(file));
    digest.update("\0");
  }
  return digest.digest("hex").slice(0, 16);
}
