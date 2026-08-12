#!/usr/bin/env node

import { resolve } from "node:path";
import { chromium } from "playwright";

const [requestedUrl = "http://127.0.0.1:8921/benchmarks/compat/public-cdn-v040.html", output = "benchmarks/compat/public-cdn-v040.png"] = process.argv.slice(2);
const outputPath = resolve(output);
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
  await page.goto(requestedUrl, { waitUntil: "domcontentloaded" });
  await page.waitForFunction(
    () => globalThis.__realtimeManimPublicReady || globalThis.__realtimeManimPublicError,
    null,
    { timeout: 30_000 },
  );
  const state = await page.evaluate(() => ({
    ready: globalThis.__realtimeManimPublicReady === true,
    error: globalThis.__realtimeManimPublicError ?? null,
    status: document.querySelector("#status")?.textContent ?? "",
    gpu: Boolean(navigator.gpu),
  }));
  if (!state.ready || state.error) throw new Error(state.error ?? "public package did not become ready");
  await page.waitForTimeout(300);
  await page.locator("#manim").screenshot({ path: outputPath });
  if (errors.length) throw new Error(errors.join("\n"));
  process.stdout.write(`${JSON.stringify({ url: requestedUrl, output: outputPath, ...state, errors })}\n`);
} finally {
  await browser.close();
}
