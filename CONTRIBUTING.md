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
npm ci --prefix site
cargo install wasm-bindgen-cli --version 0.2.126 --locked
```

The Python bridge targets Manim Community 0.20.1 on Python 3.13. The checked-in
requirements preserve the validated environment, including numerical library
versions. On macOS:

```sh
brew install python@3.13 pkg-config cairo pango
"$(brew --prefix python@3.13)/bin/python3.13" -m venv .venv-manim-reference
.venv-manim-reference/bin/python -m pip install -r scripts/requirements-manim.txt
```

Text/TeX scenes also need a TeX distribution with `latex` and `dvisvgm`, such as
Homebrew `texlive`. The core Python checks run without TeX. Set `MANIM_PYTHON`
to use an existing Python executable instead of the default environment.

## Repository map

| Path | Work here for |
| --- | --- |
| `crates/` | Shared scene evaluation, text, SVG, and benchmark schemas |
| `apps/native-preview/`, `apps/web-preview/` | Native and WebGPU renderers |
| `packages/manim-web/` | Public JS API, types, and distributable Wasm |
| `scripts/compile-manim.py` | Python-to-retained-scene bridge |
| `server/` | Local animation studio and generation API |
| `site/` | Public docs and editable 3D demos |
| `benchmarks/` | Corpus inputs, reproducible fixtures, and measurements |
| `docs/receipts/` | Dated evidence and known limitations |

## Verification

Run the local checks before opening a pull request:

```sh
npm run validate
```

This runs Rust format/check/clippy/tests, strict native GPU smoke, Wasm checks,
Python regressions, JS server/package tests, and site lint/typecheck/build/tests.
It fails on the first failed check. It does not rebuild the versioned Wasm
package, start browser servers, or publish anything.
The Rust runner reads `rust-toolchain.toml` and invokes Rustup explicitly, so
a Homebrew Rust installation earlier on `PATH` cannot select a different compiler.

For focused work, use `npm run validate:rust`, `npm run test:python`,
`npm run test:python -- --core`, `npm run test:package`, or
`npm run validate:docs`. The core Python subset covers the corpus manifest,
stroke semantics, hull geometry, and process determinism without TeX. CI runs
that subset in its own pinned Python environment; the full TeX regression
remains a local gate.

Browser/WebGPU differential tests additionally require a WebGPU-capable browser.
With a repository-root static server on port 8921, run
`npm run test:browser-package` for the rendered package lifecycle, bidi, and
WebGPU pixel gate. The test uses the installed `playwright-core` package and the
system Google Chrome binary; CI runs this gate on macOS.
See [`site/README.md`](site/README.md) for production docs browser checks.
Run `npm run build:package` before package/browser tests when changing Rust
runtime code, so they exercise the newly compiled Wasm.

## Generated files

Do not commit Rust targets, dependency directories, Python environments, package
archives, Manim `media/`, or generated compatibility renders and retained-scene
JSON. The source corpus, schemas, fixtures, shaders, tests, and sanitized benchmark
manifests remain versioned.

The exception is `packages/manim-web/runtime/`: the npm package ships that generated
JavaScript/Wasm runtime, so a runtime change must rebuild it with
`npm run build:package` and include the resulting files.

Never commit API keys, npm tokens, cookies, `.env` files, or real user assets.
