# galaxy-gen feature inventory

Baseline of what ships. Pairs with `README.md` (pitch) and `development.md` (architecture).

## Simulation core (Rust, `src/rust/galaxy.rs`)

- Cell-grid N-body sim. Flat `size×size` grid, Struct-of-Arrays storage (`mass`, `vel_x/y`, `frac_x/y`, `acc_x/y`, `xs_i/ys_i`) for auto-vectorization.
- Newtonian gravity, O(N²/2) symmetric pair sweep. Skips zero-mass cells, accumulates cartesian acceleration. Polar form removed to drop trig per pair.
- Precomputed `inv_r3` lookup table indexed by integer r² so the hot path has no `sqrt`.
- Sub-grid fractional offsets across ticks; per-tick step cap (`MAX_SUBGRID_STEP = 0.5`); softening length (`SOFTENING_SQ = 1.0`).
- Mass-merge on collision via a `Vec<u32>` scratch buffer instead of a HashMap.
- Immutable-style API: `seed()` and `tick()` return new `Galaxy`. Reuses scratch internally.
- Four initial-condition presets (`InitialCondition` enum, exposed to JS): `Uniform`, `Rotation`, `Bang`, `Collision`.
- Reproducible seeding via ChaCha `StdRng`. Same `(additional, seed)` -> byte-identical galaxies. Powers `?seed=...` URL sharing.
- `from_state(...)` rebuild from raw arrays. Used to ship state across the Web Worker boundary without re-seeding.
- `tick_with_accel(time, acc_x, acc_y)` external-acceleration tick path so a WebGPU backend can supply the force field and reuse the CPU integrator + collision step.
- Zero-copy typed-array exports (`mass_ptr` / `mass_len` plus `mass` / `x` / `y` / `vel_x` / `vel_y` / `frac_x` / `frac_y`).
- Rust unit tests in-file under `mod tests_*`. Benches at `benches/{tick_bench,debug_sim}.rs`.

## JS / WASM boundary (`src/js/lib/galaxy.ts`)

- `Frontend` class wraps the WASM `Galaxy`, stable JS surface.
- Pluggable compute backend (`"cpu" | "webgpu"`, runtime-selected, WebGPU falls back to CPU on tick failure).
- Snapshot / restore helpers for main-thread <-> worker state transfer.

## Web Worker tick loop (`src/js/lib/tick-worker.ts`)

Physics off the main thread; worker owns its own `Galaxy` WASM instance. Zero-copy state transfer in/out. Live `dt` updates mid-run. Graceful degradation when `Worker` is unavailable.

## WebGPU backend (`src/js/lib/webgpu.ts`)

WGSL compute shader for direct-sum O(N²) N-body force kernel. Bodies as `(pos.xy, mass, _pad)`, params as `(n, g, soft_sq, _pad)`. Feature detection + clean fallback via `isWebGPUAvailable()`. Hands acceleration to `tick_with_accel`, keeps collision + integration in WASM.

## React UI (`src/js/lib/application.tsx`)

Plain `useState`. Controls: galaxy size, seed mass, init-condition dropdown, init / tick / run-pause / reset-view. Live stats: dt, tick count, tick ms, FPS. Keyboard: `space` play/pause, `↑/↓` scale dt by 1.25×, `r` reset dt. URL param round-trip (`?seed=&size=&mass=&dt=`) via `history.replaceState`. u64 seed: `crypto.getRandomValues` for fresh, `BigInt` for paste/validate. `data-wasm-ready` gate. Every E2E-touched element has `data-testid` (load-bearing).

## Visualization (`src/js/lib/dataviz.tsx`)

Canvas (not SVG) renderer: single `<canvas>` per frame; SVG `setAttribute` was a bottleneck. DPR-aware (clamped 2× for HiDPI). Pan + zoom camera: pointer-drag pan, wheel zoom (with ctrl-wheel pinch), zoom clamp `[1, 50]`, pan clamp so world rect intersects viewport. Camera state observable via `data-cam-{tx,ty,zoom}` for E2E. Reset-view button.

## Build, test, deploy

- `wasm-pack build` outputs `pkg/`, linked via `npm install ./pkg`.
- Webpack 5 + Babel (React/TS), Tailwind v4 via PostCSS.
- HMR + dual auto-reload via `cargo watch` + `webpack-dev-server`. Dev server live-reloads on `pkg/` changes.
- ESLint flat + Prettier over `src/` + `e2e/`. TS noEmit typecheck. Rust `clippy -D warnings`, `cargo fmt`.
- Playwright E2E boots dev server, asserts UI shell, init, seed cell count, tick advancement, mass redistribution, WebGPU path when `navigator.gpu` is present.
- CI: GH Actions `rust` / `js` / `e2e` jobs on push/PR to `main`. E2E uploads HTML report on failure.
- Sentry browser SDK in `src/js/index.js` (`SENTRY_DSN`-driven). Served on k3s on `kai-server` at `galaxy-gen.coilysiren.me` by **unprivileged nginx** (`Dockerfile` stage 2: `nginxinc/nginx-unprivileged` + `nginx.conf`, wasm MIME + immutable caching for hashed bundles). The deploy surface lives in coilyco-bridge/deploy `services/galaxy-gen/` (chart, namespace, rollout, public ingress), which builds this repo's Dockerfile over the git context at rollout - no in-repo manifests and no publish CI. This retired the busybox-data-bundle + initContainer + stock-caddy shape (galaxy-gen#22) and the old envsubst manifest under `deploy/`. CI here runs tests on push and PR.

## Known scope-shape signals

README lists nine inspirational sibling projects; consult when evaluating scope adds. Already pulled in: WebGPU compute kernel, worker-based physics, reproducible seeding + URL params. `docs/perf-rewrite.md` documents the SoA + cartesian + lookup-table rewrite; treat as load-bearing for the inner loop.

## See also

- [README.md](../README.md) - human-facing intro.
- [AGENTS.md](../AGENTS.md) - agent-facing operating rules.
- [.ward/ward.yaml](../.ward/ward.yaml) - allowlisted commands.

Cross-reference convention from agentic-os#59.
