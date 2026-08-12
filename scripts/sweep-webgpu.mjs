#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { chromium } from "playwright";

const root = resolve(import.meta.dirname, "..");
const endpoint = process.env.REALTIME_MANIM_URL || "http://127.0.0.1:8917";
const chromePath =
  process.env.CHROME_PATH ||
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const outputPath = resolve(
  root,
  process.env.SWEEP_OUTPUT || "benchmarks/compat/full-corpus/browser-sweep.json",
);
const manifest = JSON.parse(
  await readFile(resolve(root, "benchmarks/corpus/manifest.json"), "utf8"),
);

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
      const sample = await page.evaluate(() => {
        const source = document.querySelector("#viewport");
        const target = document.createElement("canvas");
        target.width = 128;
        target.height = 72;
        const context = target.getContext("2d", { willReadFrequently: true });
        context.drawImage(source, 0, 0, target.width, target.height);
        const pixels = context.getImageData(0, 0, target.width, target.height).data;
        let occupied = 0;
        for (let offset = 0; offset < pixels.length; offset += 4) {
          if (
            Math.abs(pixels[offset] - 9) > 5 ||
            Math.abs(pixels[offset + 1] - 9) > 5 ||
            Math.abs(pixels[offset + 2] - 11) > 5
          ) {
            occupied += 1;
          }
        }
        return occupied;
      });
      const png = await page.locator("#viewport").screenshot();
      frames.push({
        at,
        occupiedSamplePixels: sample,
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

  const report = {
    at: new Date().toISOString(),
    endpoint,
    scenes: results.length,
    seeks: results.reduce((sum, result) => sum + result.frames.length, 0),
    errors,
    results,
  };
  await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(
    `WebGPU sweep: ${report.scenes} scenes, ${report.seeks} seeks, ${errors.length} errors`,
  );
  if (errors.length > 0) {
    console.error(JSON.stringify(errors, null, 2));
    process.exitCode = 1;
  }
} finally {
  await browser.close();
}
