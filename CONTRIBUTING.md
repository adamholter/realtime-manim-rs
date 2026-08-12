# Contributing

realtime-manim is an experimental Rust/WebGPU runtime with native, browser,
JavaScript-package, and regular-Manim compatibility surfaces. Changes should
preserve deterministic explicit-time evaluation and avoid browser-only scene
semantics in the Rust core.

## Setup

Install Node.js 22 or newer and Rustup. The checked-in
`rust-toolchain.toml` installs the pinned Rust toolchain and WebAssembly target.
The browser build also needs the matching `wasm-bindgen-cli` release:

```sh
npm ci
cargo install wasm-bindgen-cli --version 0.2.126 --locked
```

## Verification

Run the portable checks before opening a pull request:

```sh
cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p realtime-manim-web-preview --target wasm32-unknown-unknown
.venv-manim-reference/bin/python scripts/test-transform-matching-semantics.py
npm run check:web
npm run test:web
npm run build:package
npm run test:package
```

Browser/WebGPU differential tests additionally require a WebGPU-capable browser.
With a repository-root static server on port 8921, run
`npm run test:browser-package` for the rendered package lifecycle, bidi, and
WebGPU pixel gate. The test uses the installed `playwright-core` package and the
system Google Chrome binary; CI runs this gate on macOS.
The regular-Manim corpus tools require a separate Python environment with Manim
Community and its system dependencies.

## Generated files

Do not commit Rust targets, dependency directories, Python environments, package
archives, Manim `media/`, or generated compatibility renders and retained-scene
JSON. The source corpus, schemas, fixtures, shaders, tests, and sanitized benchmark
manifests remain versioned.

The exception is `packages/manim-web/runtime/`: the npm package ships that generated
JavaScript/Wasm runtime, so a runtime change must rebuild it with
`npm run build:package` and include the resulting files.

Never commit API keys, npm tokens, cookies, `.env` files, or real user assets.
