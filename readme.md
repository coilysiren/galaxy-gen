# galaxy-gen

`{ rust → wasm → js }` galaxy generation simulation. Gravitational physics
(Newton's law on a cell grid) computed in Rust, compiled to WebAssembly via
[wasm-pack](https://github.com/rustwasm/wasm-pack), rendered in the browser
with React + [D3](https://d3js.org/).

Previous version (Python): [coilysiren/galaxySim](https://github.com/coilysiren/galaxySim).

## Quick start

```bash
make install     # cargo build + wasm-pack + npm install + playwright browsers
make dev         # rust/wasm watcher + webpack-dev-server (dual auto-reload)
make test        # rust unit tests + Playwright E2E
```

See the [makefile](makefile) for the full set of targets and
[AGENTS.md](AGENTS.md) for the conventions.

## Architecture

- `src/rust/galaxy.rs` — core simulation (`Galaxy` + `Cell` structs, cell
  types Gas / Star / Planet / White Hole, gravity, seeding, `tick`). Unit
  tests live in `mod tests_*` blocks at the bottom of the file.
- `src/rust/lib.rs` — crate root; re-exports `galaxy`.
- `pkg/` — `wasm-pack` output: `.wasm` + `.js` + `.d.ts`. Gitignored; linked
  into `node_modules/galaxy_gen_backend` by `npm install ./pkg`.
- `src/js/lib/galaxy.ts` — `Frontend` class; the JS ↔ WASM boundary.
- `src/js/lib/application.tsx` — React UI (inputs, buttons, `data-testid`s).
- `src/js/lib/dataviz.tsx` — D3 scatter plot into `#dataviz`.
- `src/js/lib/styles.css` — Tailwind v4 + custom coilysiren palette.
- `e2e/galaxy.spec.ts` — Playwright end-to-end tests.
- `dist/` — production webpack build output (gitignored).

## Tooling

- **Rust**: `cargo check` / `cargo test` / `cargo fmt` / `cargo clippy`.
- **WASM**: `wasm-pack build` compiles Rust to WebAssembly. The dev server
  watches `pkg/**/*` and hot-reloads on rebuild (via `cargo watch`).
- **JS**: webpack 5 + babel (React + TypeScript presets). Tailwind v4 via
  PostCSS.
- **Lint/format**: ESLint flat config (`eslint.config.mjs`) + Prettier.
- **Tests**: Rust unit tests via `cargo test`; browser end-to-end via
  [Playwright](https://playwright.dev/) (`npm run test:e2e`).
- **CI**: GitHub Actions runs `rust`, `js`, and `e2e` jobs on every push/PR
  to `main`.

## Similar projects

Other open-source galaxy / n-body / WASM-sim projects worth a look:

- [andrewdcampbell/galaxy-sim](https://github.com/andrewdcampbell/galaxy-sim)
  — browser N-body galaxy formation with 20k gas particles (JS/WebGL).
- [magwo/fullofstars](https://github.com/magwo/fullofstars) — the
  progenitor of `galaxy-sim`; real-time N-body galaxy toy (JS/WebGL).
- [simbleau/nbody-wasm-sim](https://github.com/simbleau/nbody-wasm-sim) —
  2D N-body in Rust compiled to WASM with WebGPU rendering.
- [MichaelJCole/n-body-wasm-webvr](https://github.com/MichaelJCole/n-body-wasm-webvr)
  — N-body in a WASM worker thread with WebVR/A-Frame.
- [someguynamedmatt/gravity](https://github.com/someguynamedmatt/gravity) —
  gravity sim built with Rust + wasm-bindgen (same toolchain as galaxy-gen).
- [zotho/rust_n_body](https://github.com/zotho/rust_n_body) — Rust N-body
  with a WebAssembly browser demo.
- [aestuans/blob](https://github.com/aestuans/blob) — Rust→WASM 2D fluid +
  gravity sim via WebGL.
- [DrA1ex/JS_ParticleSystem](https://github.com/DrA1ex/JS_ParticleSystem) —
  "galaxy birth" browser sim using Barnes-Hut spatial tree + WebGL2.
- [davrempe/webgl-nbody-sim](https://github.com/davrempe/webgl-nbody-sim)
  — 3D gravitational N-body in vanilla JS/WebGL.
- [holmgr/gemini](https://github.com/holmgr/gemini) — sci-fi galaxy
  simulation in Rust (non-browser), heavy procedural generation.

## Deployment

Deployed to [galaxy-gen.coilysiren.me](https://galaxy-gen.coilysiren.me).
Docker image published to GitHub Container Registry, served through Caddy
on k3s on `kai-server` via Tailscale. See the `deploy` GitHub Actions
workflow for the pipeline.
