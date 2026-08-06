# galaxy-gen feature inventory

Baseline of what ships. Pairs with `README.md` (pitch) and `development.md` (architecture).

## Simulation core (Rust, `src/rust/galaxy.rs`)

- Cell-grid N-body sim. Flat `size×size` grid, Struct-of-Arrays storage (`mass`, `vel_x/y`, `frac_x/y`, `acc_x/y`, `xs_i/ys_i`) for auto-vectorization.
- Newtonian gravity, O(N²/2) symmetric pair sweep. Skips zero-mass cells, accumulates cartesian acceleration. Polar form removed to drop trig per pair.
- Precomputed `inv_r3` lookup table indexed by integer r² so the hot path has no `sqrt`.
- Sub-grid fractional offsets across ticks, per-tick step cap (`MAX_SUBGRID_STEP = 1.0`), and softening length (`SOFTENING_SQ = 1.0`).
- Mass-merge on collision via a `Vec<u32>` scratch buffer instead of a HashMap.
- Immutable-style API: `seed()` and `tick()` return new `Galaxy`. Reuses scratch internally.
- Four scenarios (`Scenario` enum, exposed to JS), each a hardcoded `start => end-shape` pair whose physics constants steer the run toward its promised shape: `bang => ring`, `bang => spiral`, `irregular => spiral`, `irregular => elliptical`. A static halo rotation curve plus flow-relaxation dissipation keeps each disk rotating. The ring scenario adds an axisymmetric annular potential, conservative radial gas transport, and annulus-scoped collapse. Spiral scenarios add a rotating logarithmic density wave and cold-gas transport into broad compression lanes. The elliptical scenario instead uses resolved pressure, distributed collapse, and collisionless phase mixing to relax an irregular cloud into a pressure-supported stellar spheroid. See [ring-density-waves.md](ring-density-waves.md), [spiral-density-waves.md](spiral-density-waves.md), [elliptical-relaxation.md](elliptical-relaxation.md), and [galaxy-rust.md](galaxy-rust.md).
- Reproducible seeding via ChaCha `StdRng`. Same `(additional, seed)` -> byte-identical galaxies. Powers `?seed=...` URL sharing.
- `from_state(...)` rebuild from raw arrays. Used to ship state across the Web Worker boundary without re-seeding.
- `tick_with_accel(time, acc_x, acc_y)` external-gravity tick path so a WebGPU backend can supply the N-body force field and reuse Rust scenario forces, gas integration, and collision handling.
- Zero-copy typed-array exports (`mass_ptr` / `mass_len` plus `mass` / `x` / `y` / `vel_x` / `vel_y` / `frac_x` / `frac_y`).
- Rust unit tests in-file under `mod tests_*`. Benches at `benches/{tick_bench,debug_sim}.rs`.

## Living-galaxy loop (`src/rust/process.rs`, `src/rust/events.rs`, `src/rust/stars.rs`)

Static process registry with declared reads/writes, freshness requirements, and per-process cadence. `tick` runs due processes in registry order, then executes the tick's due events. The deterministic event queue emits at N and executes at N+1 with stable ordering, causal parent ids, and a bounded instrumentation ring. Stateless per-(process, tick) RNG streams derive from the `?seed=` master. On top: a sparse collisionless star population reads a coarse Barnes-Hut gravity field with a central black hole, and the full causal loop runs unattended with a closed baryonic mass ledger. Walkthrough: [processes-events.md](processes-events.md).

The gas lifecycle includes a closed galactic fountain. Radiation lifts cold disk gas into a serialized hot halo reservoir, then cooling returns small moving parcels to the evolved disk. The visible cold share follows a deterministic 40-60% cycle while the combined reservoir remains in the baryonic ledger.

Chemical enrichment is conserved across cold gas, the hot halo, resolved and phase-mixed stars, the black hole, radiation, and in-flight births. Stars inherit their birth-cloud composition, core-collapse supernovae synthesize an explicit heavy-element yield, and the worker round-trip preserves the full composition ledger. The renderer derives dust opacity and shock-swept [OIII] intensity from local metallicity. See [chemical-enrichment.md](chemical-enrichment.md).

The stellar lifecycle is bounded rather than immortal. Lower-mass stars become red giants, return their envelopes through `PlanetaryNebula`, and leave white dwarfs. Intermediate-mass birth draws form deterministic binaries whose seed-derived delay ends in a fully disruptive `TypeIaSupernova`, a composition yield, and a causally linked shock. Massive stars supernova into neutron stars, and core-collapse-scale binaries later emit `NeutronStarMerger`, combine into one remnant, account for radiated mass, then emit a short `GammaRayBurst`. Stars that remain beyond the luminous disk and old compact remnants phase-mix into a serialized diffuse stellar-halo reservoir. See [stellar-evolution.md](stellar-evolution.md).

Star formation produces temporary stellar associations rather than unrelated point sprays. Nearby young collapses join one association, batches inherit a shared prograde center-of-mass orbit, and a momentum-neutral internal potential keeps the group legible. Age and the local galactic tide release members into streams without deleting or teleporting them. The renderer derives a restrained shared glow from the still-bound members. See [stellar-associations.md](stellar-associations.md).

The central black hole now acts directly on gas as well as stars. Weak nuclear viscosity removes angular momentum across the inner disk while a compact low-angular-momentum sink accretes slowly, allowing stable nuclear rings to remain visible while feeding the hole.

## JS / WASM boundary (`src/js/lib/galaxy.ts`)

- `Frontend` class wraps the WASM `Galaxy`, stable JS surface.
- Pluggable compute backend (`"cpu" | "webgpu"`, runtime-selected, WebGPU falls back to CPU on tick failure).
- Snapshot / restore helpers for main-thread <-> worker state transfer.

## Web Worker tick loop (`src/js/lib/tick-worker.ts`)

Physics off the main thread; worker owns its own `Galaxy` WASM instance. Zero-copy state transfer in/out. Render snapshots include gas sub-cell offsets, so the main-thread canvas preserves continuous physical motion instead of showing only integer cell hops. Live `dt` updates mid-run. Tick rate capped at 30/s. Graceful degradation when `Worker` is unavailable.

## WebGPU backend (`src/js/lib/webgpu.ts`)

WGSL compute shader for direct-sum O(N²) N-body force kernel. Bodies as `(pos.xy, mass, _pad)`, params as `(n, g, soft_sq, _pad)`. Feature detection + clean fallback via `isWebGPUAvailable()`. Hands acceleration to `tick_with_accel`, keeps collision + integration in WASM.

## React UI (`src/js/lib/application.tsx`)

Plain `useState`. Sidebar layout on desktop (sticky controls left, viz right), stacked on mobile. Controls: galaxy size (default 250), scenario dropdown (the four start => end pairs), generate / play-pause / step. Seed mass and dt are fixed constants - both retired as config surfaces. Seven default statistics keep the panel compact: sim tick, resolved stars, total supernovae, planetary nebulae, phase-mixed stars, black-hole growth, and the visible cold gas share. Detailed lifecycle counters, tick-ms, and FPS appear only under `?debug=1`. No keyboard surface - generate / play / step buttons are the whole interface. URL param round-trip (`?seed=&size=&scenario=&lock=&t=`) uses `history.replaceState`. Generate cycles to a fresh seed each press unless `lock=1` pins it, while a URL-provided seed is honored for the first generate either way. `t=` makes any moment addressable: pausing or stepping stamps the current sim tick into the URL, and loading a seed+t link auto-generates and fast-forwards to that exact tick. Determinism guarantees the identical frame. u64 seed: `crypto.getRandomValues` for fresh, `BigInt` for paste/validate. `data-wasm-ready` gate. Every E2E-touched element has `data-testid` (load-bearing).

## Visualization (`src/js/lib/dataviz.tsx`)

Canvas (not SVG) renderer: single `<canvas>` per frame. SVG `setAttribute` was a bottleneck. DPR-aware (clamped 2× for HiDPI). The complete world renders in a deterministic stellar co-rotating frame, so faster nebular gas sweeps through star-forming regions and leaves newborn stars behind instead of appearing to fire them outward. New main-sequence stars begin dimly beneath both gas passes and ease into the exposed stellar layer with age, while association glow follows the same reveal. Pan + zoom camera is a dev utility gated behind `?debug=1` (pointer-drag pan, wheel zoom, zoom clamp `[1, 50]`, double-click reset, `data-cam-{tx,ty,zoom}` observability). Without the flag the view is locked and interaction-free. Layers: soft nebular gas sprites at fractional physical positions in four tiers - cold blue-violet, warm magenta, hot H-alpha pink, and shock-swept [OIII] teal - split into under- and over-star passes, multiply-composited brown dust lanes, restrained bound-association glows, sharp stellar-class points, cyan compact remnants, a diffuse phase-mixed stellar halo, and event transients for expanding supernova shells, planetary-nebula shells, and short gamma-ray jets. The 1.42x view leaves black sky beyond a broad radial fade that starts inside the nominal disk and reaches zero before the canvas, hiding the finite simulation boundary. A gravitational-lens post-process around the central black hole adds point-mass deflection, Einstein-ring arclets, an inverted inner image, event-horizon shadow, and photon ring over the finished frame. See [co-rotating-frame.md](co-rotating-frame.md).

## Build, test, deploy

- `wasm-pack build` outputs `pkg/`, linked via `npm install ./pkg`.
- Webpack 5 + Babel (React/TS), Tailwind v4 via PostCSS.
- HMR + dual auto-reload via `cargo watch` + `webpack-dev-server`. Dev server live-reloads on `pkg/` changes. `GALAXY_DEV_PORT` overrides the default port when another local service already owns it.
- The Ward `debug-sim` verb runs the native seeded structure probe and accepts ticks, size, seed count, and an optional starting seed.
- ESLint flat + Prettier over `src/` + `e2e/`. TS noEmit typecheck. Rust `clippy -D warnings`, `cargo fmt`.
- Playwright E2E boots dev server, asserts UI shell, init, seed cell count, tick advancement, mass redistribution, WebGPU path when `navigator.gpu` is present.
- CI: GH Actions `rust` / `js` / `e2e` jobs on push/PR to `main`. E2E uploads HTML report on failure.
- Served on k3s at `galaxy-gen.coilysiren.me` by **unprivileged nginx** (`Dockerfile` stage 2: `nginxinc/nginx-unprivileged` + `nginx.conf`, wasm MIME + immutable caching for hashed bundles). Source Forgejo CI tests the Rust core, then the trusted deploy lane publishes one private `forgejo.coilysiren.me/coilyco-gaming/galaxy-gen:<full-source-sha>` Linux image and proves its remote manifest. The deploy repo owns only the read-only pull credential, chart, namespace, rollout, and public ingress. This retired the deploy-owned remote-context build, busybox data bundle, initContainer, stock-caddy shape, and old envsubst manifest.

## Known scope-shape signals

README lists nine inspirational sibling projects; consult when evaluating scope adds. Already pulled in: WebGPU compute kernel, worker-based physics, reproducible seeding + URL params. `docs/perf-rewrite.md` documents the SoA + cartesian + lookup-table rewrite; treat as load-bearing for the inner loop.

## See also

- [README.md](../README.md) - human-facing intro.
- [AGENTS.md](../AGENTS.md) - agent-facing operating rules.
- [.ward/ward.yaml](../.ward/ward.yaml) - allowlisted commands.

Cross-reference convention from agentic-os#59.
