import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const siteRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(siteRoot, "..");
const packageRoot = resolve(repoRoot, "packages/manim-web");
const playgroundRoot = resolve(siteRoot, "public/playground");
const runtimeRoot = resolve(siteRoot, "public/runtime");

await Promise.all([mkdir(playgroundRoot, { recursive: true }), mkdir(runtimeRoot, { recursive: true })]);

const packageApi = await readFile(resolve(packageRoot, "src/index.js"), "utf8");
const browserApi = packageApi.replace(
  "initRuntime(wasmUrl).catch((error) => {",
  `(async () => {
      if (wasmUrl === undefined) return initRuntime();
      const response = await fetch(wasmUrl);
      if (!response.ok) {
        throw new Error("Failed to fetch the docs Wasm runtime: " + response.status + " " + response.statusText);
      }
      return initRuntime({ module_or_path: await response.arrayBuffer() });
    })().catch((error) => {`,
);
if (browserApi === packageApi) throw new Error("Could not adapt the package Wasm initializer for the docs playground.");

await Promise.all([
  writeFile(resolve(playgroundRoot, "realtime-manim.js"), browserApi),
  copyFile(
    resolve(packageRoot, "runtime/realtime_manim_web_preview.js"),
    resolve(runtimeRoot, "realtime_manim_web_preview.js"),
  ),
  copyFile(
    resolve(packageRoot, "runtime/realtime_manim_web_preview_bg.wasm"),
    resolve(runtimeRoot, "realtime_manim_web_preview_bg.wasm"),
  ),
]);
