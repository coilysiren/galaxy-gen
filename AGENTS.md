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
ward exec install
ward exec test-rust
ward exec test-e2e
ward exec test
ward exec dev
ward exec dev-js
ward exec dev-rust
ward exec build-js-prod
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

## Workflow

The resolved workflow for this repo is `direct-to-main`. Commit and push
finished work straight to `main` on Forgejo, then close the issue. Do not park
a finished change on a task branch waiting for a human to merge it, and do not
open a pull request for the default case.

Pushing to `main` publishes the image and rolls the public site, so the gate is
the test suite rather than a review. Land only with `ward exec test-rust`,
`ward exec check-js`, and `ward exec test-e2e` green, and never with
`--no-verify`.

Use a branch only when the work is genuinely unfinished, when a human has to
choose between paths first, or when Kai asks for one. A branch is also the
right home for a checkpoint an agent cannot carry to completion - push it
rather than leaving the only copy local.

## Deploy

Source CI owns the image build. A push to `main` first runs the Rust test job,
then the trusted `deploy` runner publishes the private image as
`forgejo.coilysiren.me/coilyco-gaming/galaxy-gen:<full-source-sha>`. The runner
supplies package write authority as `REGISTRY_TOKEN`, and the publisher proves
the remote immutable manifest after its single-architecture push. Browser e2e
and tsc stay on GitHub PR CI because the in-cluster runner cannot reach the
Playwright browser CDN.

The deploy surface remains in
[coilyco-bridge/deploy](https://forgejo.coilysiren.me/coilyco-bridge/deploy)
under `services/galaxy-gen/`. That repo owns the chart, namespace, rollout,
public ingress, and separate read-only `forgejo-registry` pull credential. It
does not build this source. Never add deploy manifests back here.

---

## Commands

Route every dev command through Ward, which reads [`.ward/ward.yaml`](.ward/ward.yaml). Run verbs with `ward exec <verb>`. The lockdown denies bare invocations of the underlying tools (`cargo`, `wasm-pack`, `npx`, etc.). Add new verbs to that file before invoking them.

Run `ward exec image-publish-check` and `ward exec build-docker` when changing
the Forgejo OCI publisher.

## See also

- [README.md](README.md) - human-facing intro.
- [docs/FEATURES.md](docs/FEATURES.md) - inventory of what ships today.
- [.ward/ward.yaml](.ward/ward.yaml) - allowlisted commands (`ward exec`).

Cross-reference convention from agentic-os#59.
