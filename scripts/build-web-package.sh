#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

"$repo_root/scripts/build-web.sh"
mkdir -p "$repo_root/packages/manim-web/runtime"
cp "$repo_root/apps/web-preview/www/pkg/realtime_manim_web_preview.js" "$repo_root/packages/manim-web/runtime/"
cp "$repo_root/apps/web-preview/www/pkg/realtime_manim_web_preview_bg.wasm" "$repo_root/packages/manim-web/runtime/"

if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz \
    --strip-debug \
    --strip-producers \
    --strip-toolchain-annotations \
    --enable-bulk-memory \
    --enable-nontrapping-float-to-int \
    --enable-sign-ext \
    "$repo_root/packages/manim-web/runtime/realtime_manim_web_preview_bg.wasm" \
    -o "$repo_root/packages/manim-web/runtime/realtime_manim_web_preview_bg.wasm.optimized"
  mv "$repo_root/packages/manim-web/runtime/realtime_manim_web_preview_bg.wasm.optimized" \
    "$repo_root/packages/manim-web/runtime/realtime_manim_web_preview_bg.wasm"
fi
