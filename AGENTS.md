# Agent instructions

Workspace conventions load globally via `~/.claude/CLAUDE.md` -> `agentic-os-kai/AGENTS.md`. This file covers only what is specific to this repo.

---

Galaxy-gen is a Rust → WASM → JS galaxy generation simulation. Gravitational physics are computed in Rust, compiled to WebAssembly via `wasm-pack`, and visualized with React + D3 in the browser. See `development.md` for architecture details.

Before second-guessing a non-obvious choice (`getrandom` `wasm_js` backend, binaryen flags, the `Galaxy` immutable-style API), check `git log` and recent commit messages for the rationale - there is usually prior context.

## Project Layout

Load-bearing files you will touch most often:

- `src/rust/galaxy.rs` - core simulation (`Galaxy` + `Cell` structs, gravity, seeding, `tick`). All unit tests live in `mod tests_*` blocks at the bottom.
- `src/rust/lib.rs` - crate root; re-exports `galaxy`.
- `src/js/lib/galaxy.ts` - `Frontend` class; the JS ↔ WASM boundary.
- `src/js/lib/application.tsx` - React UI (controls + buttons). Test IDs on inputs/buttons (`data-testid="btn-init"` etc.) are load-bearing for E2E.
- `src/js/lib/dataviz.tsx` - D3 scatter plot into `#dataviz`.
- `src/js/lib/styles.css` - custom styles (dark theme, galaxy-gen palette).
- `e2e/galaxy.spec.ts` - Playwright end-to-end tests.
- `playwright.config.ts` - Playwright config; auto-boots webpack-dev-server.
- `webpack.config.js` - dev server (HMR + live-reload on `pkg/` changes).

## Dev Loop

```bash
make install           # one-time: cargo build, wasm-pack, npm install, playwright browsers
make test-rust         # cargo check + cargo test
make test-e2e          # build WASM + run Playwright headless
make test              # rust + e2e (full suite)
make dev               # rust watcher + JS dev server (auto-reload on both sides)
make dev-js            # JS dev server only (HMR)
make dev-rust          # cargo watch → wasm-pack build --dev
make build-js-prod     # production webpack build
```

Raw commands: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt`, `wasm-pack build` (output `pkg/`, gitignored), `npm run dev` (HMR :8081), `npm run test:e2e[:ui]` (Playwright), `npm run lint` / `format`.

## Conventions

- Rust public API crosses the WASM boundary via `#[wasm_bindgen]`; keep private helpers in plain `impl` blocks.
- `Galaxy` is immutable-style: `seed()` and `tick()` return new instances.
- The grid is a flat `Vec<Cell>` indexed by `row * size + col`.
- Physics is stored as magnitude + degrees, not x/y vectors - convert at computation boundaries.
- React state is plain `useState` - no state library.
- Use `data-testid` on any UI element that an E2E test asserts against.
- Commits that change the WASM surface should mention it in the subject line (e.g. `wasm: expose mass() typed array`) so `git log --grep=wasm` is useful.

## Key References

- wasm-bindgen book: https://rustwasm.github.io/wasm-bindgen/
- wasm-pack: https://rustwasm.github.io/wasm-pack/
- `getrandom` `wasm_js` backend (why 0.3 needs explicit config): see https://docs.rs/getrandom/0.3/getrandom/#webassembly-support
- Playwright: https://playwright.dev/docs/intro

## CI

GitHub Actions (`.github/workflows/action.yml`) runs three jobs on PR to `main`:

- `rust` - `cargo build` / `check` / `test` / `wasm-pack build`
- `js` - `wasm-pack build` / `npm ci` / `npm run build`
- `e2e` - `wasm-pack build` / `npm ci` / `playwright test` (uploads HTML report artifact on failure)

## Deploy

The deploy surface lives in the deploy monorepo - [coilyco-bridge/deploy](https://forgejo.coilysiren.me/coilyco-bridge/deploy) `services/galaxy-gen/` (chart, namespace, values, rollout, public ingress; deploy#173). This repo carries **no manifests and no publish CI**: the deploy repo's `rollout.sh` builds this repo's `Dockerfile` over the git context (`galaxy-gen.git#main`) on kai-server and pushes it to the in-cluster registry. `Dockerfile` stage 2 is **unprivileged nginx** (`nginxinc/nginx-unprivileged` + `nginx.conf`), which retired the busybox-data-bundle + initContainer + stock-caddy shape (galaxy-gen#22) and the legacy `coilysiren-galaxy-gen` namespace. Push to `main` here runs the in-cluster Forgejo pipeline (`.forgejo/workflows/build-publish-deploy.yml`): `cargo test` only; browser e2e + tsc stay on GitHub PR CI (the in-cluster runner can't reach the Playwright browser CDN). A deploy-repo push under `services/galaxy-gen/**` auto-rolls the site; never add deploy manifests back here (deploy-repo boundary rule).

---

## Commands

Route every dev command through ward, which reads [`.ward/ward.yaml`](.ward/ward.yaml) (run verbs with `ward exec <verb>`). The lockdown denies bare invocations of the underlying tools (`make`, `cargo`, `wasm-pack`, `npx`, etc.). Add new verbs to that file before invoking them.

## See also

- [README.md](README.md) - human-facing intro.
- [docs/FEATURES.md](docs/FEATURES.md) - inventory of what ships today.
- [.ward/ward.yaml](.ward/ward.yaml) - allowlisted commands (`ward exec`).

Cross-reference convention from agentic-os#59.
