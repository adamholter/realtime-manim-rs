import { existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const siteRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(siteRoot, "..");
const packageRoot = resolve(repoRoot, "packages/manim-web");
const playgroundRoot = resolve(siteRoot, "public/playground");
const runtimeRoot = resolve(siteRoot, "public/runtime");
const snapshotPath = resolve(playgroundRoot, "snapshot.json");
const snapshotFiles = [
  "playground/realtime-manim.js",
  "runtime/realtime_manim_web_preview.js",
  "runtime/realtime_manim_web_preview_bg.wasm",
];
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

// The Sites source repository is rooted here, without the parent monorepo.
// Its committed runtime snapshot must match the exact validated release bytes.
if (!existsSync(resolve(packageRoot, "src/index.js"))) {
  const snapshot = JSON.parse(await readFile(snapshotPath, "utf8"));
  const sitePackage = JSON.parse(await readFile(resolve(siteRoot, "package.json"), "utf8"));
  if (snapshot.version !== sitePackage.realtimeManimVersion) {
    throw new Error("The standalone playground snapshot is from a different release.");
  }
  for (const file of snapshotFiles) {
    if (sha256(await readFile(resolve(siteRoot, "public", file))) !== snapshot.sha256[file]) {
      throw new Error(`The standalone playground snapshot is corrupt: ${file}`);
    }
  }
  console.log(`Verified standalone realtime-manim ${snapshot.version} snapshot.`);
  process.exit(0);
}

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
const packageJson = JSON.parse(await readFile(resolve(packageRoot, "package.json"), "utf8"));
await writeFile(snapshotPath, JSON.stringify({
  version: packageJson.version,
  sha256: Object.fromEntries(await Promise.all(snapshotFiles.map(async (file) => [
    file, sha256(await readFile(resolve(siteRoot, "public", file))),
  ]))),
}, null, 2) + "\n");
