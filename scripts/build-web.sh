#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
web_build_rustflags=${RUSTFLAGS-}
if [ -n "$web_build_rustflags" ]; then
  web_build_rustflags="$web_build_rustflags "
fi
export RUSTFLAGS="${web_build_rustflags}--remap-path-prefix=$HOME=/workspace"

cd "$repo_root"
cargo build --release -p realtime-manim-glsl-to-wgsl
cargo build -p realtime-manim-web-preview --target wasm32-unknown-unknown --release
wasm-bindgen \
  --target web \
  --out-dir apps/web-preview/www/pkg \
  --out-name realtime_manim_web_preview \
  target/wasm32-unknown-unknown/release/realtime_manim_web_preview.wasm
