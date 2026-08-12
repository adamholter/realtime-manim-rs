#!/usr/bin/env node

import { chromium } from "playwright-core";
import { createHash } from "node:crypto";
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
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
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
  await page.goto("http://127.0.0.1:8921/packages/manim-web/test/browser.html");
  await page.waitForFunction(() => globalThis.__MANIM_BROWSER_TEST__, null, { timeout: 30_000 });
  const state = await page.evaluate(() => globalThis.__MANIM_BROWSER_TEST__);
  if (!state.ok) throw new Error(state.error ?? JSON.stringify(state));
  const bidiCanvases = [
    "bidi-mixed",
    "bidi-joined",
    "bidi-markup-same",
    "bidi-markup-colored",
    "bidi-rtl-spans",
    "bidi-isolated",
  ];
  const bidiPixels = {};
  for (const id of bidiCanvases) {
    const png = await page.locator(`#${id}`).screenshot();
    const pixels = await page.evaluate(async (encoded) => {
      const bytes = Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0));
      const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      const context = canvas.getContext("2d", { willReadFrequently: true });
      context.drawImage(bitmap, 0, 0);
      const data = context.getImageData(0, 0, bitmap.width, bitmap.height).data;
      let occupiedPixels = 0;
      const foregroundMask = new Uint8Array(bitmap.width * bitmap.height);
      const palette = {
        red: { rgb: [252, 98, 85], count: 0, xTotal: 0 },
        green: { rgb: [131, 193, 103], count: 0, xTotal: 0 },
        blue: { rgb: [88, 196, 221], count: 0, xTotal: 0 },
      };
      for (let index = 0; index < data.length; index += 4) {
        const distance = Math.abs(data[index] - 7)
          + Math.abs(data[index + 1] - 9)
          + Math.abs(data[index + 2] - 16);
        if (data[index + 3] > 0 && distance > 24) {
          occupiedPixels += 1;
          foregroundMask[index / 4] = 1;
          let nearest = null;
          let nearestDistance = Number.POSITIVE_INFINITY;
          for (const [name, sample] of Object.entries(palette)) {
            const colorDistance = (data[index] - sample.rgb[0]) ** 2
              + (data[index + 1] - sample.rgb[1]) ** 2
              + (data[index + 2] - sample.rgb[2]) ** 2;
            if (colorDistance < nearestDistance) {
              nearest = name;
              nearestDistance = colorDistance;
            }
          }
          if (nearest !== null && nearestDistance < 2500) {
            palette[nearest].count += 1;
            palette[nearest].xTotal += (index / 4) % bitmap.width;
          }
        }
      }
      const maskDigest = new Uint8Array(await crypto.subtle.digest("SHA-256", foregroundMask));
      const foregroundMaskSha256 = [...maskDigest]
        .map((byte) => byte.toString(16).padStart(2, "0"))
        .join("");
      bitmap.close();
      const paletteStats = Object.fromEntries(Object.entries(palette).map(([name, sample]) => [
        name,
        {
          count: sample.count,
          meanX: sample.count === 0 ? null : sample.xTotal / sample.count,
        },
      ]));
      return {
        width: canvas.width,
        height: canvas.height,
        occupiedPixels,
        foregroundMaskSha256,
        paletteStats,
      };
    }, png.toString("base64"));
    bidiPixels[id] = {
      ...pixels,
      sha256: createHash("sha256").update(png).digest("hex"),
    };
  }
  if (Object.values(bidiPixels).some(({ occupiedPixels }) => occupiedPixels < 200)) {
    throw new Error(`Bidi WebGPU canvases did not render enough text pixels: ${JSON.stringify(bidiPixels)}`);
  }
  const joined = bidiPixels["bidi-joined"];
  const sameMarkup = bidiPixels["bidi-markup-same"];
  const coloredMarkup = bidiPixels["bidi-markup-colored"];
  const rtlSpans = bidiPixels["bidi-rtl-spans"];
  const isolated = bidiPixels["bidi-isolated"];
  if (joined.foregroundMaskSha256 !== sameMarkup.foregroundMaskSha256
    || joined.occupiedPixels !== sameMarkup.occupiedPixels) {
    throw new Error("Same-face MarkupText spans changed joined Arabic geometry.");
  }
  if (joined.foregroundMaskSha256 !== coloredMarkup.foregroundMaskSha256
    || joined.occupiedPixels !== coloredMarkup.occupiedPixels) {
    throw new Error("Paint-only MarkupText spans changed joined Arabic geometry.");
  }
  if (coloredMarkup.paletteStats.red.count < 50 || coloredMarkup.paletteStats.blue.count < 50) {
    throw new Error(`Colored joined Arabic did not retain both span paints: ${JSON.stringify(coloredMarkup)}`);
  }
  const { red, green, blue } = rtlSpans.paletteStats;
  if (red.count < 30 || green.count < 30 || blue.count < 30
    || !(blue.meanX < green.meanX && green.meanX < red.meanX)) {
    throw new Error(`RTL span paints are not in visual order: ${JSON.stringify(rtlSpans)}`);
  }
  if (joined.foregroundMaskSha256 === isolated.foregroundMaskSha256) {
    throw new Error("Joined Arabic unexpectedly matched deliberately isolated Text nodes.");
  }
  const transparencyScenes = await page.evaluate(async () => {
    const { createManimPlayer } = await import("/packages/manim-web/src/index.js");
    const canvas = (id) => {
      const element = document.createElement("canvas");
      element.id = id;
      element.width = 64;
      element.height = 64;
      element.style.width = "64px";
      element.style.height = "64px";
      document.body.append(element);
      return element;
    };
    const scene = (title, nodes) => ({
      version: 2,
      title,
      width: 4,
      height: 4,
      pixelWidth: 64,
      pixelHeight: 64,
      duration: 1,
      fps: 60,
      background: "#000000",
      camera3d: {
        position: [0, 0, 0], target: [0, 0, 1], up: [0, 1, 0],
        fovY: 0.9, near: 1, far: 10, ambient: 1,
      },
      nodes,
      tracks: [], signals: [], bindings: [], controls: [], audio: [], captions: [],
    });
    const mesh = (id, z, color, opacity = 1) => ({
      id,
      type: "mesh",
      vertices: [[-z, -z, z], [0, z, z], [z, -z, z]],
      triangles: [[0, 1, 2]],
      colors: [color, color, color],
      unlit: true,
      doubleSided: true,
      style: { fill: "#ffffffff", stroke: null, opacity },
    });
    const behind = await createManimPlayer({
      canvas: canvas("transparent-behind-opaque"),
      scene: scene("transparent behind opaque", [
        mesh("opaque-front", 2, "#ff0000ff"),
        mesh("transparent-behind", 4, "#0000ffff", 0.5),
      ]),
      autoplay: false,
    });
    const ordered = await createManimPlayer({
      canvas: canvas("transparent-global-order"),
      scene: scene("global transparent order", [
        mesh("near-blue-first", 2, "#0000ffff", 0.5),
        mesh("far-red-second", 4, "#ff0000ff", 0.5),
      ]),
      autoplay: false,
    });
    globalThis.__MANIM_TRANSPARENCY_PLAYERS__ = [behind, ordered];
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return true;
  });
  const centerPixel = async (id) => {
    const png = await page.locator(`#${id}`).screenshot();
    return page.evaluate(async (encoded) => {
      const bytes = Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0));
      const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      const context = canvas.getContext("2d", { willReadFrequently: true });
      context.drawImage(bitmap, 0, 0);
      const pixel = [...context.getImageData(
        Math.floor(bitmap.width / 2),
        Math.floor(bitmap.height / 2),
        1,
        1,
      ).data];
      bitmap.close();
      return pixel;
    }, png.toString("base64"));
  };
  const behindOpaqueCenter = await centerPixel("transparent-behind-opaque");
  const globalOrderCenter = await centerPixel("transparent-global-order");
  await page.evaluate(() => {
    globalThis.__MANIM_TRANSPARENCY_PLAYERS__.forEach((player) => player.destroy());
    delete globalThis.__MANIM_TRANSPARENCY_PLAYERS__;
  });
  if (!transparencyScenes || behindOpaqueCenter[0] < 245 || behindOpaqueCenter[2] > 10) {
    throw new Error(`Transparent geometry leaked through opaque depth: ${behindOpaqueCenter}`);
  }
  if (!(globalOrderCenter[2] > globalOrderCenter[0]
    && globalOrderCenter[0] > 40 && globalOrderCenter[2] > 110)) {
    throw new Error(`Transparent 3D nodes were not blended globally far-to-near: ${globalOrderCenter}`);
  }
  if (errors.length) throw new Error(errors.join("\n"));
  process.stdout.write(`${JSON.stringify({
    ...state,
    bidiPixels,
    transparency: { behindOpaqueCenter, globalOrderCenter },
    errors,
  })}\n`);
} finally {
  await browser.close();
}
