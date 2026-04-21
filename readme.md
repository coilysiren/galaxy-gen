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

A few open-source galaxy / n-body / WASM-sim projects worth studying.
Feature bullets below are drawn from each project's readme and public
metadata, surfaced here with admiration:

- [andrewdcampbell/galaxy-sim](https://github.com/andrewdcampbell/galaxy-sim)
  — real-time N-body galaxy-formation sim in the browser.
  - Three.js / WebGL rendering with cubemap skybox backgrounds and both
    free-orbit and guided camera modes.
  - Runtime controls for gravitational strength, particle opacity, and
    HiDPI pixel ratio — tweakable during the sim without a rebuild.
  - Keyboard shortcuts for play/pause, rotation, and UI toggles, plus
    the usual stars-coalescing-around-a-central-black-hole narrative.
- [magwo/fullofstars](https://github.com/magwo/fullofstars) — the
  original real-time N-body galaxy toy this project's art direction
  takes after.
  - Pure JavaScript + WebGL rendering, no build step.
  - Served as the architectural reference for the newer `galaxy-sim`
    fork above — a nice example of an idea worth forking.
  - Tight, low-dependency footprint that runs as a single static page.
- [simbleau/nbody-wasm-sim](https://github.com/simbleau/nbody-wasm-sim)
  — 2D N-body simulation in Rust, compiled to WebAssembly.
  - Uses `wgpu` so the N-body force kernel runs as a WebGPU compute
    shader on the GPU.
  - Same Rust source drives both native and web builds — one of the
    cleaner examples of the wgpu cross-platform story.
  - Explicit, documented toolchain (wasm + wgpu-rs) tagged via GitHub
    topics, which makes the project easy to learn from.
- [MichaelJCole/n-body-wasm-webvr](https://github.com/MichaelJCole/n-body-wasm-webvr)
  — a browser universe rendered in WebVR.
  - Runs the physics in a dedicated Web Worker so the main thread stays
    free for A-Frame's scene graph.
  - Core integrator is written in AssemblyScript and compiled to WASM.
  - Scene is viewable in a headset via A-Frame / WebVR — a nice example
    of how to stitch WASM + Workers + WebVR together.
- [someguynamedmatt/gravity](https://github.com/someguynamedmatt/gravity)
  — compact gravity sim on the same Rust + wasm-bindgen toolchain as
  this project.
  - Useful baseline for what the bare wasm-bindgen Rust→JS surface
    looks like before you layer on SoA / lookup-tables / canvas.
  - Rust-first with a browser front-end — a gentle entry point for
    folks new to wasm-bindgen.
  - Repo's small enough to read end-to-end in one sitting.
- [zotho/rust_n_body](https://github.com/zotho/rust_n_body) — Rust
  N-body with a WASM browser demo.
  - Ships both a native Rust binary and a WASM build from the same
    crate.
  - Focuses on the integrator rather than flashy rendering — a good
    reference for the physics layer in isolation.
  - Tagged `rust` / `wasm` / `gravity`, which signals the intent
    clearly.
- [aestuans/blob](https://github.com/aestuans/blob) — showcase of
  Rust→WASM through a 2D fluid-and-gravity simulation.
  - Combines fluid dynamics *and* gravity in one sim rather than
    treating them as separate demos.
  - Renders through WebGL directly from the WASM side, so the browser
    path stays tight.
  - Intentionally pitched as a *showcase* of the toolchain, which
    makes the code a friendly reference.
- [DrA1ex/JS_ParticleSystem](https://github.com/DrA1ex/JS_ParticleSystem)
  — galaxy-birth simulation handling up to 1,000,000 particles in
  real time.
  - Hierarchical spatial-tree approximation drops the per-step cost
    from O(N²) to O(N log N), as documented in the readme with clear
    diagrams.
  - Pluggable CPU and GPGPU backends — the same sim can fall back to
    CPU or trade accuracy for a GPU-accelerated run.
  - Record-and-replay simulation player, live query-string parameter
    tuning, and particle-collision mode — a lot of dials for someone
    who wants to explore the parameter space.
- [davrempe/webgl-nbody-sim](https://github.com/davrempe/webgl-nbody-sim)
  — 3D N-body simulation written in vanilla WebGL.
  - Keeps the full simulation *and* rendering in a browser tab
    without a framework wrapper, which is impressive on its own.
  - 3D (not 2D) N-body rendering — the extra dimension is noticeably
    harder to tune visually.
  - No build system or transpile step — cloneable and runnable as-is.
- [holmgr/gemini](https://github.com/holmgr/gemini) — sci-fi
  trading-and-smuggling game built around a procedurally generated
  galaxy, in Rust.
  - Topics include `parallelism` and `machine-learning`, hinting at
    more depth in the generation pipeline than a typical procgen toy.
  - Terminal UI front-end over a Rust core — a tasteful alternative
    to a browser build.
  - Crossed the line from "simulation" into "game" with trading and
    smuggling mechanics layered on top of the physics.

## Deployment

Deployed to [galaxy-gen.coilysiren.me](https://galaxy-gen.coilysiren.me).
Docker image published to GitHub Container Registry, served through Caddy
on k3s on `kai-server` via Tailscale. See the `deploy` GitHub Actions
workflow for the pipeline.
