# galaxy-gen feature inventory

Baseline of what ships. Pairs with `README.md` (pitch) and `development.md` (architecture).

## Simulation core (Rust, `src/rust/galaxy.rs`)

- Cell-grid N-body sim. Flat `size×size` grid, Struct-of-Arrays storage (`mass`, `vel_x/y`, `frac_x/y`, `acc_x/y`, `xs_i/ys_i`) for auto-vectorization.
- Newtonian gravity, O(N²/2) symmetric pair sweep. Skips zero-mass cells, accumulates cartesian acceleration. Polar form removed to drop trig per pair.
- Precomputed `inv_r3` lookup table indexed by integer r² so the hot path has no `sqrt`.
- Sub-grid fractional offsets across ticks; per-tick step cap (`MAX_SUBGRID_STEP = 0.5`); softening length (`SOFTENING_SQ = 1.0`).
- Mass-merge on collision via a `Vec<u32>` scratch buffer instead of a HashMap.
- Immutable-style API: `seed()` and `tick()` return new `Galaxy`. Reuses scratch internally.
- Four scenarios (`Scenario` enum, exposed to JS), each a hardcoded `start => end-shape` pair whose physics constants steer the run toward its promised shape at t ~= 1000: `bang => ring`, `bang => spiral`, `irregular => spiral`, `irregular => elliptical`. A static halo rotation curve plus flow-relaxation dissipation (drag toward the local circular flow, not toward rest) keeps every scenario visibly rotating at t=1000. See [galaxy-rust.md](galaxy-rust.md).
- Reproducible seeding via ChaCha `StdRng`. Same `(additional, seed)` -> byte-identical galaxies. Powers `?seed=...` URL sharing.
- `from_state(...)` rebuild from raw arrays. Used to ship state across the Web Worker boundary without re-seeding.
- `tick_with_accel(time, acc_x, acc_y)` external-acceleration tick path so a WebGPU backend can supply the force field and reuse the CPU integrator + collision step.
- Zero-copy typed-array exports (`mass_ptr` / `mass_len` plus `mass` / `x` / `y` / `vel_x` / `vel_y` / `frac_x` / `frac_y`).
- Rust unit tests in-file under `mod tests_*`. Benches at `benches/{tick_bench,debug_sim}.rs`.

## Living-galaxy loop (`src/rust/process.rs`, `src/rust/events.rs`, `src/rust/stars.rs`)

Static process registry with declared reads/writes, freshness requirements, and per-process cadence; `tick` runs due processes in registry order then executes the tick's due events. Deterministic event queue (emit at N, execute at N+1, stable ordering, causal parent ids) with a bounded instrumentation ring. Stateless per-(process, tick) RNG streams derived from the `?seed=` master. On top: a sparse collisionless star population reading a coarse Barnes-Hut gravity field (with a central black hole), and the full causal loop - cloud collapse -> star birth -> radiation feedback -> stellar aging -> supernova -> shock-induced collapse - running unattended with a closed baryonic mass ledger. Walkthrough: [processes-events.md](processes-events.md).

## JS / WASM boundary (`src/js/lib/galaxy.ts`)

- `Frontend` class wraps the WASM `Galaxy`, stable JS surface.
- Pluggable compute backend (`"cpu" | "webgpu"`, runtime-selected, WebGPU falls back to CPU on tick failure).
- Snapshot / restore helpers for main-thread <-> worker state transfer.

## Web Worker tick loop (`src/js/lib/tick-worker.ts`)

Physics off the main thread; worker owns its own `Galaxy` WASM instance. Zero-copy state transfer in/out. Live `dt` updates mid-run. Tick rate capped at 30/s. Graceful degradation when `Worker` is unavailable.

## WebGPU backend (`src/js/lib/webgpu.ts`)

WGSL compute shader for direct-sum O(N²) N-body force kernel. Bodies as `(pos.xy, mass, _pad)`, params as `(n, g, soft_sq, _pad)`. Feature detection + clean fallback via `isWebGPUAvailable()`. Hands acceleration to `tick_with_accel`, keeps collision + integration in WASM.

## React UI (`src/js/lib/application.tsx`)

Plain `useState`. Sidebar layout on desktop (sticky controls left, viz right), stacked on mobile. Controls: galaxy size (default 250), scenario dropdown (the four start => end pairs), generate / play-pause / step. Seed mass and dt are fixed constants - both retired as config surfaces. Live stats lean popsci, rendered as a table so plain copy-paste yields label-value lines: sim tick (the frame reference, continuous across pause/resume), star count, supernovae, clusters born, black-hole captures and growth factor, and gas-reservoir percentage; tick-ms and FPS appear only under ?debug=1. No keyboard surface - generate / play / step buttons are the whole interface. URL param round-trip (`?seed=&size=&scenario=&lock=&t=`) via `history.replaceState`; generate cycles to a fresh seed each press unless `lock=1` pins it (a URL-provided seed is honored for the first generate either way). `t=` makes any moment addressable: pausing or stepping stamps the current sim tick into the URL, and loading a seed+t link auto-generates and fast-forwards to that exact tick - determinism guarantees the identical frame. u64 seed: `crypto.getRandomValues` for fresh, `BigInt` for paste/validate. `data-wasm-ready` gate. Every E2E-touched element has `data-testid` (load-bearing).

## Visualization (`src/js/lib/dataviz.tsx`)

Canvas (not SVG) renderer: single `<canvas>` per frame; SVG `setAttribute` was a bottleneck. DPR-aware (clamped 2× for HiDPI). Pan + zoom camera is a dev utility gated behind `?debug=1` (pointer-drag pan, wheel zoom, zoom clamp `[1, 50]`, double-click reset, `data-cam-{tx,ty,zoom}` observability); without the flag the view is locked and interaction-free. Layers: soft nebular gas sprites in four tiers - cold blue-violet, warm magenta, hot H-alpha pink (keyed to the radiation field, dithered at tier edges), and shock-swept [OIII] teal tracking recent supernova fronts - split into under- and over-star passes so clusters sit inside their clouds, plus multiply-composited brown dust lanes in the densest cold cells, sharp warm star points (cream to blue-white by mass, tight glow on the brightest only), and faint event transients (expanding supernova shells, birth glints) from the executed-event ring. The sim owns the full viewport (dynamically sized canvas, disk fitted to the short dimension, live resize) with the control panel floating over it; the view spans 1.1x the grid and the radial fade hides the deep halo. Gravitational lens post-process around the central black hole: true point-mass deflection (r_src = r - thetaE^2/r) with Einstein-ring arclets, inverted inner image, event-horizon shadow, and photon ring, applied in screen space over the finished frame.

## Build, test, deploy

- `wasm-pack build` outputs `pkg/`, linked via `npm install ./pkg`.
- Webpack 5 + Babel (React/TS), Tailwind v4 via PostCSS.
- HMR + dual auto-reload via `cargo watch` + `webpack-dev-server`. Dev server live-reloads on `pkg/` changes.
- ESLint flat + Prettier over `src/` + `e2e/`. TS noEmit typecheck. Rust `clippy -D warnings`, `cargo fmt`.
- Playwright E2E boots dev server, asserts UI shell, init, seed cell count, tick advancement, mass redistribution, WebGPU path when `navigator.gpu` is present.
- CI: GH Actions `rust` / `js` / `e2e` jobs on push/PR to `main`. E2E uploads HTML report on failure.
- Sentry browser SDK in `src/js/index.js` (`SENTRY_DSN`-driven). Served on k3s at `galaxy-gen.coilysiren.me` by **unprivileged nginx** (`Dockerfile` stage 2: `nginxinc/nginx-unprivileged` + `nginx.conf`, wasm MIME + immutable caching for hashed bundles). Source Forgejo CI tests the Rust core, then the trusted deploy lane publishes one private `forgejo.coilysiren.me/coilyco-gaming/galaxy-gen:<full-source-sha>` Linux image and proves its remote manifest. The deploy repo owns only the read-only pull credential, chart, namespace, rollout, and public ingress. This retired the deploy-owned remote-context build, busybox data bundle, initContainer, stock-caddy shape, and old envsubst manifest.

## Known scope-shape signals

README lists nine inspirational sibling projects; consult when evaluating scope adds. Already pulled in: WebGPU compute kernel, worker-based physics, reproducible seeding + URL params. `docs/perf-rewrite.md` documents the SoA + cartesian + lookup-table rewrite; treat as load-bearing for the inner loop.

## See also

- [README.md](../README.md) - human-facing intro.
- [AGENTS.md](../AGENTS.md) - agent-facing operating rules.
- [.ward/ward.yaml](../.ward/ward.yaml) - allowlisted commands.

Cross-reference convention from agentic-os#59.
