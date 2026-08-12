#!/usr/bin/env node

import { chromium } from "playwright-core";
import { homedir } from "node:os";
import { join } from "node:path";
import { readFile, readdir } from "node:fs/promises";

async function bundledTestFont() {
  const registryRoot = join(homedir(), ".cargo", "registry", "src");
  for (const registry of await readdir(registryRoot)) {
    const crateRoot = join(registryRoot, registry);
    for (const name of await readdir(crateRoot)) {
      if (name === "ttf-noto-sans-0.1.2") {
        return readFile(join(crateRoot, name, "assets", "NotoSans-Bold.ttf"));
      }
    }
  }
  throw new Error("The ttf-noto-sans test dependency is not downloaded. Build the Rust workspace first.");
}

const browser = await chromium.launch({
  headless: true,
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan,UseSkiaRenderer"],
});
const page = await browser.newPage({ viewport: { width: 1280, height: 360 } });
const errors = [];
const testFontBase64 = (await bundledTestFont()).toString("base64");
await page.addInitScript((encoded) => {
  globalThis.__MANIM_TEST_FONT_BASE64__ = encoded;
}, testFontBase64);
page.on("console", (message) => {
  if (message.type() === "warning" || message.type() === "error") errors.push(`${message.type()}: ${message.text()}`);
});
page.on("pageerror", (error) => errors.push(`pageerror: ${error.message}`));

try {
  await page.goto("http://127.0.0.1:8921/packages/manim-web/test/multi-player.html");
  await page.waitForFunction(() => globalThis.__multiPlayerState, null, { timeout: 30_000 });
  const state = await page.evaluate(() => globalThis.__multiPlayerState);
  if (!state.ok) throw new Error(state.error);
  if (errors.length) throw new Error(errors.join("\n"));
  process.stdout.write(`${JSON.stringify({ ...state, errors })}\n`);
} finally {
  await browser.close();
}
