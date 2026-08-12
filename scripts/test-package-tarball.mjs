#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageRoot = join(root, "packages", "manim-web");
const temporaryRoot = await mkdtemp(join(tmpdir(), "realtime-manim-consumer-"));

try {
  const pack = JSON.parse(execFileSync(
    "npm",
    ["pack", "--json", "--pack-destination", temporaryRoot],
    { cwd: packageRoot, encoding: "utf8" },
  ));
  if (!Array.isArray(pack) || pack.length !== 1) throw new Error("npm pack returned an unexpected manifest.");
  const tarball = join(temporaryRoot, pack[0].filename);
  await writeFile(join(temporaryRoot, "package.json"), JSON.stringify({ private: true, type: "module" }));
  execFileSync("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", tarball], {
    cwd: temporaryRoot,
    stdio: "ignore",
  });

  await writeFile(join(temporaryRoot, "smoke.mjs"), `
    import * as manim from "realtime-manim";
    const line = new manim.Line([1, -1], [5, 3]).moveTo([0, 0]);
    const retained = new manim.Scene().add(line).play(manim.Create(line)).toJSON();
    if (retained.nodes.length !== 1 || retained.tracks.length !== 1) throw new Error("retained scene smoke failed");
    if (line.getCenter()[0] !== 0 || line.getCenter()[1] !== 0) throw new Error("moveTo smoke failed");
    console.log(JSON.stringify({ exports: Object.keys(manim).length, nodes: retained.nodes.length, tracks: retained.tracks.length }));
  `);
  const smoke = JSON.parse(execFileSync(process.execPath, ["smoke.mjs"], {
    cwd: temporaryRoot,
    encoding: "utf8",
  }));

  await writeFile(join(temporaryRoot, "smoke.ts"), `
    import { Create, Line, Scene, type Point } from "realtime-manim";
    const target: Point = [0, 0];
    const line = new Line([1, -1], [5, 3]).moveTo(target);
    new Scene().add(line).play(Create(line));
  `);
  execFileSync(join(root, "node_modules", ".bin", "tsc"), [
    "--noEmit", "--strict", "--skipLibCheck", "--target", "ES2022", "--module", "ESNext",
    "--moduleResolution", "Bundler", "--lib", "ES2022,DOM", "smoke.ts",
  ], { cwd: temporaryRoot, stdio: "ignore" });

  const declaration = await readFile(
    join(temporaryRoot, "node_modules", "realtime-manim", "src", "index.d.ts"),
    "utf8",
  );
  if (!declaration.includes("createManimPlayer") || !declaration.includes("moveTo(target: Mobject | Point")) {
    throw new Error("Packed package declarations are missing public APIs.");
  }

  process.stdout.write(`${JSON.stringify({
    name: pack[0].name,
    version: pack[0].version,
    files: pack[0].files.length,
    shasum: pack[0].shasum,
    exports: smoke.exports,
    nodes: smoke.nodes,
    tracks: smoke.tracks,
    declarations: true,
  })}\n`);
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}
