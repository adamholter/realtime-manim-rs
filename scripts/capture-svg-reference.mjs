#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { chromium } from "playwright";

const [scenePath, outputPath] = process.argv.slice(2);
if (!scenePath || !outputPath) {
  throw new Error("usage: capture-svg-reference.mjs <scene.json> <output.png>");
}

const scene = JSON.parse(await readFile(resolve(scenePath), "utf8"));
const svgNode = scene.nodes.find((node) => node.type === "svg");
if (!svgNode?.svg) throw new Error("scene must contain an inline SVG node");

const sourceSize = svgNode.svg.match(/<svg[^>]*\bwidth="([\d.]+)"[^>]*\bheight="([\d.]+)"/);
if (!sourceSize) throw new Error("inline SVG must declare numeric width and height");
const aspect = Number(sourceSize[1]) / Number(sourceSize[2]);
const cssHeight = (svgNode.height ?? 2) * (480 / (scene.height ?? 9));
const cssWidth = cssHeight * aspect;
const transform = svgNode.transform ?? {};
const centerX = 854 / 2 + (transform.x ?? 0) * (854 / (scene.width ?? 16));
const centerY = 480 / 2 - (transform.y ?? 0) * (480 / (scene.height ?? 9));

const browser = await chromium.launch({
  headless: true,
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
});
const page = await browser.newPage({ viewport: { width: 854, height: 480 } });
try {
  await page.setContent(`<!doctype html><style>
    html,body{margin:0;width:100%;height:100%;overflow:hidden;background:${scene.background ?? "#000"}}
    #reference{position:absolute;left:${centerX}px;top:${centerY}px;width:${cssWidth}px;height:${cssHeight}px;transform:translate(-50%,-50%) rotate(${-(transform.rotation ?? 0)}rad);opacity:${svgNode.style?.opacity ?? 1}}
  </style><div id="reference">${svgNode.svg}</div>`);
  await page.locator("#reference > svg").evaluate((svg) => {
    svg.style.width = "100%";
    svg.style.height = "100%";
    svg.style.display = "block";
  });
  await page.screenshot({ path: resolve(outputPath) });
  process.stdout.write(`${JSON.stringify({ scene: resolve(scenePath), output: resolve(outputPath) })}\n`);
} finally {
  await browser.close();
}
