#!/usr/bin/env node

import { mkdir, stat, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { performance } from "node:perf_hooks";

const ROOT = resolve(import.meta.dirname, "..");
const ENDPOINT = process.env.REALTIME_MANIM_URL || "http://127.0.0.1:8917";
const OUTPUT = resolve(ROOT, "benchmarks", "text-math", "current.json");
const FORMULAS = [
  String.raw`\int_0^\infty e^{-x^2}\,dx=\frac{\sqrt{\pi}}{2}`,
  String.raw`\begin{bmatrix}a&b\\c&d\end{bmatrix}^{-1}=\frac{1}{ad-bc}\begin{bmatrix}d&-b\\-c&a\end{bmatrix}`,
  String.raw`\begin{aligned}\nabla\cdot\mathbf{E}&=\frac{\rho}{\varepsilon_0}\\\nabla\times\mathbf{B}&=\mu_0\mathbf{J}+\mu_0\varepsilon_0\frac{\partial\mathbf{E}}{\partial t}\end{aligned}`,
  String.raw`f(x)=\begin{cases}x^2,&x\ge0\\-\sqrt{-x},&x<0\end{cases}`,
];

async function typeset(tex) {
  const started = performance.now();
  const response = await fetch(`${ENDPOINT}/api/typeset`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ tex }),
  });
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.error || `typeset failed: ${response.status}`);
  return {
    sourceHash: payload.sourceHash,
    serverTypesetMs: payload.typesetMs,
    clientElapsedMs: Math.round((performance.now() - started) * 100) / 100,
    svgBytes: Buffer.byteLength(payload.svg),
    cached: payload.cached,
    pathCount: (payload.svg.match(/<path\b/g) || []).length,
    clipPathCount: (payload.svg.match(/<clipPath\b/g) || []).length,
  };
}

const cold = [];
for (const tex of FORMULAS) cold.push(await typeset(tex));
const warm = [];
for (const tex of FORMULAS) warm.push(await typeset(tex));

const wasm = await stat(resolve(ROOT, "apps", "web-preview", "www", "pkg", "realtime_manim_web_preview_bg.wasm"));
const payload = {
  schemaVersion: 1,
  recordedAt: new Date().toISOString(),
  endpoint: ENDPOINT,
  formulaCount: FORMULAS.length,
  cold,
  warm,
  summary: {
    coldClientMeanMs:
      Math.round((cold.reduce((sum, item) => sum + item.clientElapsedMs, 0) / cold.length) * 100) / 100,
    warmClientMeanMs:
      Math.round((warm.reduce((sum, item) => sum + item.clientElapsedMs, 0) / warm.length) * 100) / 100,
    wasmBytes: wasm.size,
  },
};
await mkdir(resolve(ROOT, "benchmarks", "text-math"), { recursive: true });
await writeFile(OUTPUT, `${JSON.stringify(payload, null, 2)}\n`);
console.log(JSON.stringify(payload.summary));
