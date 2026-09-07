import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);
  return worker.fetch(
    new Request("http://localhost/", { headers: { accept: "text/html" } }),
    { ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) } },
    { waitUntil() {}, passThroughOnException() {} },
  );
}

test("server-renders the realtime-manim documentation", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);
  const html = await response.text();
  assert.match(html, /realtime-manim — Rust-speed Manim for the browser/);
  assert.match(html, /Manim-style animation/);
  assert.match(html, /npm install realtime-manim/);
  assert.match(html, /Interactive 3D lab/);
  assert.match(html, /Möbius light field/);
  assert.match(html, /href="\/favicon\.svg"/);
  assert.match(html, /Give an agent the contract/);
  assert.match(html, /href="\/llms\.txt"/);
  assert.match(html, /compatibility matrix/i);
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape|react-loading-skeleton/i);
});

test("ships the self-hosted WebGPU playground runtime", async () => {
  const [api, glue, wasm] = await Promise.all([
    stat(new URL("../public/playground/realtime-manim.js", import.meta.url)),
    stat(new URL("../public/runtime/realtime_manim_web_preview.js", import.meta.url)),
    stat(new URL("../public/runtime/realtime_manim_web_preview_bg.wasm", import.meta.url)),
  ]);
  assert.ok(api.size > 50_000);
  assert.ok(glue.size > 50_000);
  assert.ok(wasm.size > 1_000_000);
});

test("ships machine-readable agent documentation and a social card", async () => {
  const [llms, guide, image] = await Promise.all([
    readFile(new URL("../public/llms.txt", import.meta.url), "utf8"),
    readFile(new URL("../public/agent-guide.md", import.meta.url), "utf8"),
    stat(new URL("../public/og.png", import.meta.url)),
  ]);
  assert.match(llms, /npm install realtime-manim/);
  assert.match(llms, /TypeScript declarations/);
  assert.match(guide, /Keep the frame loop.*Rust\/Wasm/);
  assert.match(guide, /compatibility matrix/);
  assert.ok(image.size > 100_000);
});

test("standalone Sites source verifies its runtime snapshot", async () => {
  const directory = await mkdtemp(join(tmpdir(), "manim-site-snapshot-"));
  try {
    await Promise.all(["scripts", "public/playground", "public/runtime"].map(
      (path) => mkdir(join(directory, path), { recursive: true }),
    ));
    await copyFile(new URL("../scripts/sync-playground.mjs", import.meta.url), join(directory, "scripts/sync-playground.mjs"));
    const manifest = JSON.parse(await readFile(new URL("../public/playground/snapshot.json", import.meta.url), "utf8"));
    const hashes = {};
    for (const file of Object.keys(manifest.sha256)) {
      const bytes = await readFile(new URL(`../public/${file}`, import.meta.url));
      hashes[file] = createHash("sha256").update(bytes).digest("hex");
      assert.equal(hashes[file], manifest.sha256[file]);
      await writeFile(join(directory, "public", file), bytes);
    }
    await writeFile(join(directory, "public/playground/snapshot.json"), JSON.stringify(manifest));
    await writeFile(join(directory, "package.json"), JSON.stringify({ realtimeManimVersion: manifest.version }));
    const run = () => spawnSync(process.execPath, ["scripts/sync-playground.mjs"], { cwd: directory, encoding: "utf8" });
    assert.equal(run().status, 0);
    await writeFile(join(directory, "package.json"), JSON.stringify({ realtimeManimVersion: "wrong-version" }));
    assert.match(run().stderr, /different release/);
    await writeFile(join(directory, "package.json"), JSON.stringify({ realtimeManimVersion: manifest.version }));
    await writeFile(join(directory, "public/playground/realtime-manim.js"), "corrupt");
    assert.match(run().stderr, /snapshot is corrupt/);
  } finally {
    execFileSync("rm", ["-rf", directory]);
  }
});
