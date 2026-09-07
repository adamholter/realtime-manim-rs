import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const args = process.argv.slice(2);
if (args.some((arg) => arg !== "--core")) {
  console.error("Usage: npm run test:python -- [--core]");
  process.exit(2);
}
const python = process.env.MANIM_PYTHON || resolve(root, ".venv-manim-reference/bin/python");
if (!process.env.MANIM_PYTHON && !existsSync(python)) {
  console.error("Missing Manim environment. Follow CONTRIBUTING.md setup or set MANIM_PYTHON to a Python executable.");
  process.exit(1);
}
const scripts = [
  "audit-corpus.py",
  "test-compile-manim-strokes.py",
  "test-compile-manim-determinism.py",
  ...(!args.includes("--core") ? ["test-transform-matching-semantics.py"] : []),
];
for (const script of scripts) {
  console.log(`\nPython check: ${script}`);
  const result = spawnSync(python, [resolve(root, "scripts", script)], {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, PYTHONHASHSEED: "0" },
  });
  if (result.error || result.signal || result.status !== 0) {
    console.error(`${script} failed: ${result.error?.message || result.signal || result.status}`);
    process.exit(result.status || 1);
  }
}
