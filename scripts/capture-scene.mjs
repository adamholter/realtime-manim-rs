#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { chromium } from "playwright";

const [scenePath, outputPath, requestedTime = "0"] = process.argv.slice(2);
if (!scenePath || !outputPath) {
  throw new Error("usage: capture-scene.mjs <scene.json> <output.png> [time]");
}

const scene = JSON.parse(await readFile(resolve(scenePath), "utf8"));
const time = Number(requestedTime);
if (!Number.isFinite(time) || time < 0 || time > scene.duration) {
  throw new Error(`time must be within 0–${scene.duration}`);
}

const browser = await chromium.launch({
  headless: true,
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan,UseSkiaRenderer"],
});
const page = await browser.newPage({ viewport: { width: 854, height: 480 } });
const errors = [];
page.on("console", (message) => {
  if (message.type() === "warning" || message.type() === "error") {
    errors.push(`${message.type()}: ${message.text()}`);
  }
});
page.on("pageerror", (error) => errors.push(`pageerror: ${error.message}`));

try {
  await page.goto("http://127.0.0.1:8917/", { waitUntil: "networkidle" });
  await page.waitForFunction(
    () => document.querySelector("#renderer-status")?.textContent.includes("engine running"),
    null,
    { timeout: 20_000 },
  );
  await page.evaluate(async (candidate) => {
    const app = await import("/app.js");
    await app.runScene(candidate, { record: false });
  }, scene);
  await page.evaluate(async (at) => {
    const renderer = await import("/pkg/realtime_manim_web_preview.js");
    renderer.seek_scene(at);
    const canvas = document.querySelector("#viewport");
    canvas.style.position = "fixed";
    canvas.style.left = "0";
    canvas.style.top = "0";
    canvas.style.width = "854px";
    canvas.style.height = "480px";
    canvas.style.zIndex = "99999";
    document.querySelector(".preview-label").hidden = true;
  }, time);
  await page.waitForTimeout(250);
  await page.locator("#viewport").screenshot({ path: resolve(outputPath) });
  if (errors.length) throw new Error(errors.join("\n"));
  process.stdout.write(
    `${JSON.stringify({ scene: resolve(scenePath), output: resolve(outputPath), time, errors })}\n`,
  );
} finally {
  await browser.close();
}
