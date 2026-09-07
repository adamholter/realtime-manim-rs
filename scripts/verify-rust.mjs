import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const config = readFileSync(resolve(root, "rust-toolchain.toml"), "utf8");
const toolchain = config.match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
if (!toolchain) throw new Error("Missing channel in rust-toolchain.toml");

const checks = [
  ["fmt", "--check"],
  ["check", "--workspace", "--all-targets", "--locked"],
  ["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
  ["test", "--workspace", "--locked"],
  ["run", "-p", "realtime-manim-native-preview", "--locked", "--", "smoke", "--strict", "--width", "640", "--height", "360"],
  ["check", "-p", "realtime-manim-web-preview", "--target", "wasm32-unknown-unknown", "--locked"],
  ["clippy", "-p", "realtime-manim-web-preview", "--target", "wasm32-unknown-unknown", "--locked", "--", "-D", "warnings"],
];
for (const args of checks) {
  console.log(`\nRust ${toolchain}: cargo ${args.join(" ")}`);
  // Invoke Rustup explicitly. Homebrew's cargo/rustc can precede Rustup on
  // PATH and ignore the repository toolchain and its installed Wasm target.
  const result = spawnSync("rustup", ["run", toolchain, "cargo", ...args], {
    cwd: root,
    stdio: "inherit",
  });
  if (result.error || result.signal || result.status !== 0) {
    console.error(`Rust check failed: ${result.error?.message || result.signal || result.status}`);
    process.exit(result.status || 1);
  }
}
