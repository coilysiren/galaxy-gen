# galaxy-gen

`{ rust → wasm → js }` galaxy generation simulation. Gravitational physics
(Newton's law on a cell grid) computed in Rust, compiled to WebAssembly via
[wasm-pack](https://github.com/rustwasm/wasm-pack), rendered in the browser
with React + [D3](https://d3js.org/).

## Quick start

```bash
ward exec install
ward exec dev
ward exec test
```

See [.ward/ward.yaml](.ward/ward.yaml) for the full command catalog and
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
- `src/js/lib/styles.css` — Tailwind v4 + custom palette.
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

Open-source galaxy / n-body / WASM-sim projects worth studying. Surfaced with admiration.

- [andrewdcampbell/galaxy-sim](https://github.com/andrewdcampbell/galaxy-sim) - real-time browser N-body, Three.js + WebGL, runtime controls and free-orbit camera.
- [magwo/fullofstars](https://github.com/magwo/fullofstars) - the original real-time N-body galaxy toy this project's art direction takes after. Pure JS + WebGL, no build step.
- [simbleau/nbody-wasm-sim](https://github.com/simbleau/nbody-wasm-sim) - 2D N-body, Rust + wgpu, force kernel runs as a WebGPU compute shader.
- [MichaelJCole/n-body-wasm-webvr](https://github.com/MichaelJCole/n-body-wasm-webvr) - browser universe in WebVR, physics in a Web Worker, AssemblyScript -> WASM.
- [someguynamedmatt/gravity](https://github.com/someguynamedmatt/gravity) - compact gravity sim on the same Rust + wasm-bindgen toolchain. Bare-surface reference.
- [zotho/rust_n_body](https://github.com/zotho/rust_n_body) - Rust N-body with native + WASM builds from one crate. Integrator-focused.
- [aestuans/blob](https://github.com/aestuans/blob) - Rust -> WASM showcase, 2D fluid + gravity in one sim, renders WebGL from the WASM side.
- [DrA1ex/JS_ParticleSystem](https://github.com/DrA1ex/JS_ParticleSystem) - 1M particles in real time. Hierarchical spatial-tree O(N log N) + pluggable CPU/GPGPU backends + record-replay.
- [davrempe/webgl-nbody-sim](https://github.com/davrempe/webgl-nbody-sim) - 3D N-body in vanilla WebGL, no build step.
- [holmgr/gemini](https://github.com/holmgr/gemini) - sci-fi trading-and-smuggling over a procedurally generated galaxy. Terminal UI over a Rust core.

## Contributing

External contributors are welcome. The pre-commit pipeline includes the ward lockdown checks and audit trail hooks that guard this workspace. Run them before your first commit, or the hook will fail.

## Deployment

Deployed to [galaxy-gen.coilysiren.me](https://galaxy-gen.coilysiren.me).
The deploy surface lives in the deploy monorepo
([coilyco-bridge/deploy](https://forgejo.coilysiren.me/coilyco-bridge/deploy),
`services/galaxy-gen/`), which serves the source-owned image with unprivileged
nginx on k3s. A push to `main` first tests the Rust core, then publishes the
private image as
`forgejo.coilysiren.me/coilyco-gaming/galaxy-gen:<full-source-sha>`. The deploy
repo owns the separate read-only `forgejo-registry` pull credential, chart,
rollout, and public ingress.

Validate the local image and publisher through Ward:

```bash
ward exec image-publish-check
ward exec build-docker
```

## Commands

Dev commands are declared in [`.ward/ward.yaml`](.ward/ward.yaml). Run them as `ward exec <verb>`.

## See also

- [AGENTS.md](AGENTS.md) - agent-facing operating rules.
- [docs/FEATURES.md](docs/FEATURES.md) - inventory of what ships today.
- [.ward/ward.yaml](.ward/ward.yaml) - allowlisted commands.

Cross-reference convention from agentic-os#59.
