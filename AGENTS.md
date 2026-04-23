# Agent instructions

See `../AGENTS.md` for workspace-level conventions (git workflow, test/lint autonomy, readonly ops, writing voice, deploy knowledge). This file covers only what's specific to this repo.

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
- `src/js/lib/styles.css` - custom styles (dark theme, coilysiren palette).
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

Raw commands if you need them:

- `cargo test` - Rust unit tests (fast).
- `cargo clippy -- -D warnings` - Rust lint.
- `cargo fmt` - Rust formatter.
- `wasm-pack build` - compile to WASM, output in `pkg/` (gitignored).
- `npm run dev` - webpack-dev-server with HMR on port 8080.
- `npm run test:e2e` - Playwright (builds WASM first via `pretest:e2e`).
- `npm run test:e2e:ui` - Playwright UI mode (time-travel debugger).
- `npm run lint` / `npm run format` - ESLint / Prettier over `src/` + `e2e/`.

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

GitHub Actions (`.github/workflows/action.yml`) runs three jobs on push/PR to `main`:

- `rust` - `cargo build` / `check` / `test` / `wasm-pack build`
- `js` - `wasm-pack build` / `npm ci` / `npm run build`
- `e2e` - `wasm-pack build` / `npm ci` / `playwright test` (uploads HTML report artifact on failure)
