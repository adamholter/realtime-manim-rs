import assert from "node:assert/strict";
import { readFile, stat } from "node:fs/promises";
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
