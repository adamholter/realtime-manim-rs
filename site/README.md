# realtime-manim documentation site

The public documentation and editable 3D demos live here. This is a vinext and
React site hosted on ChatGPT Sites. It uses the repository's Rust/Wasm browser
package for playback.

## Local development

From the repository root, with Node.js 22.13 or newer:

```sh
npm ci
npm ci --prefix site
npm run docs:dev
```

The dev command prints its URL. Every dev or production build copies the
versioned runtime from `packages/manim-web/` into ignored `public/playground/`
and `public/runtime/` directories. Edit the package source, not those copies.
After Rust runtime changes, run `npm run build:package` at the repository root
before rebuilding this site.

## Where to work

| Path | Purpose |
| --- | --- |
| `app/page.tsx` | Documentation homepage |
| `app/` | Playground components, scene examples, and styles |
| `public/agent-guide.md`, `public/llms.txt` | Agent documentation |
| `scripts/sync-playground.mjs` | Copies package files into build assets |
| `tests/` | Rendered HTML and desktop/mobile WebGPU checks |
| `.openai/hosting.json` | Existing Sites project and optional bindings |
| `worker/`, `build/` | Hosting entry point and build integration |

## Verification

```sh
npm run validate:docs
npm run start --prefix site -- --hostname 127.0.0.1 --port 3001
```

With that production server running, in a second terminal:

```sh
npm run test:browser --prefix site
```

The browser gate uses isolated headless Chrome. It checks all three 3D demos,
slider and editor updates, the favicon, console/network errors, and mobile
overflow. It uses the root `playwright-core` dependency, so install root
dependencies before running it. `REALTIME_MANIM_SITE_URL` overrides the default
`http://localhost:3001` target.

The site has no configured D1 or R2 binding. Optional database and sign-in
template code is retained for future use. See
[Sites template reference](docs/sites-template.md) for those integration notes.
