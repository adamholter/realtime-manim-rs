#!/usr/bin/env node

import { chromium } from "playwright-core";

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
  const before = await page.evaluate(async () => {
    const app = await import("/app.js");
    app.seekPreview(0.7);
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const time = app.previewDiagnostics().time;
    app.simulatePreviewDeviceLoss();
    return time;
  });
  await page.waitForFunction(
    async () => {
      const app = await import("/app.js");
      return (
        app.previewDiagnostics().recoveryCount >= 1 &&
        document.querySelector("#renderer-status")?.textContent.includes("engine running")
      );
    },
    null,
    { timeout: 20_000 },
  );
  await page.waitForTimeout(250);
  const result = await page.evaluate(async () => {
    const app = await import("/app.js");
    const diagnostics = app.previewDiagnostics();
    return {
      count: diagnostics.recoveryCount,
      time: diagnostics.time,
      status: document.querySelector("#renderer-status")?.textContent,
      backend: document.querySelector("#backend-value")?.textContent,
      pngLength: document.querySelector("#viewport").toDataURL("image/png").length,
    };
  });
  if (errors.length) throw new Error(errors.join("\n"));
  if (result.count !== 1) throw new Error(`expected one recovery, received ${result.count}`);
  if (Math.abs(result.time - before) > 0.000_01) {
    throw new Error(`scene time changed across recovery: ${before} -> ${result.time}`);
  }
  if (result.pngLength < 10_000) throw new Error("recovered canvas is blank");
  process.stdout.write(`${JSON.stringify({ before, ...result, errors })}\n`);
} finally {
  await browser.close();
}
