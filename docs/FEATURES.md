# galaxy-gen feature inventory

Coarse inventory of what ships. Pairs with `README.md` (pitch) and
`development.md` (architecture). Each entry links its walkthrough.

## Simulation core (Rust, `src/rust/galaxy.rs`)

- Cell-grid N-body sim on a flat `size×size` Struct-of-Arrays grid. Newtonian
  O(N²/2) symmetric pair sweep, `inv_r3` lookup table, no `sqrt` in the hot path.
- Sub-grid fractional offsets, per-tick step cap, softening length, and a
  scratch-buffer mass merge on collision.
- Immutable-style API: `seed()` and `tick()` return a new `Galaxy`.
  `from_state(...)` rebuilds across the worker boundary and
  `tick_with_accel(...)` accepts an external force field.
- Reproducible ChaCha `StdRng` seeding, so the same `(additional, seed)` gives
  byte-identical galaxies. Zero-copy typed-array exports.
  See [galaxy-rust.md](galaxy-rust.md).
- Four scenarios pairing a start with an end shape: `bang => ring`,
  `bang => spiral`, `irregular => spiral`, `irregular => elliptical`. See
  [ring-density-waves.md](ring-density-waves.md),
  [spiral-density-waves.md](spiral-density-waves.md), and
  [elliptical-relaxation.md](elliptical-relaxation.md).

## Living-galaxy loop (`src/rust/process.rs`, `src/rust/events.rs`, `src/rust/stars.rs`)

Static process registry with declared reads and writes, per-process cadence, a
deterministic event queue, and per-(process, tick) RNG streams derived from the
`?seed=` master. See [processes-events.md](processes-events.md).

- Closed galactic fountain and a conserved metal ledger across gas, stars,
  remnants, and radiation. See [chemical-enrichment.md](chemical-enrichment.md).
- Bounded stellar lifecycles through red giants, white dwarfs, Type Ia and
  core-collapse supernovae, neutron-star mergers, and a phase-mixed halo.
  See [stellar-evolution.md](stellar-evolution.md).
- Star formation into temporary bound associations that tidally release into
  streams. See [stellar-associations.md](stellar-associations.md).
- Central black hole with nuclear viscosity, slow accretion, and brief bipolar
  quasar episodes. See [quasar-feedback.md](quasar-feedback.md).

## Frontend

- `Frontend` wraps the WASM `Galaxy` behind a stable JS surface with a
  runtime-selected `"cpu" | "webgpu"` backend (`src/js/lib/galaxy.ts`).
- Physics runs off the main thread with zero-copy state transfer and a 20/s tick
  cap. See [tick-worker.md](tick-worker.md).
- WGSL direct-sum force kernel with feature detection and clean CPU fallback
  (`src/js/lib/webgpu.ts`).
- Controls, URL round-trip, and the chrome toggle.
  See [ui-controls.md](ui-controls.md).
- Layered canvas renderer with a gravitational-lens post-process.
  See [rendering.md](rendering.md), [starfield.md](starfield.md),
  [co-rotating-frame.md](co-rotating-frame.md), and
  [perf-rewrite.md](perf-rewrite.md).
- Client-side GIF and MP4 capture of a reproducible run.
  See [recording.md](recording.md).

## Build, test, deploy

- `wasm-pack build` outputs `pkg/`. Webpack 5, Babel, and Tailwind v4 build the
  client. HMR and dual auto-reload via `cargo watch` and `webpack-dev-server`.
- ESLint, Prettier, TS noEmit, `clippy -D warnings`, `cargo fmt`, and the Ward
  `debug-sim` seeded structure probe.
- Playwright E2E plus the `perf-profile` and `test-perf` GPU specs.
- Served on k3s at `galaxy-gen.coilysiren.me` by unprivileged nginx. Forgejo CI
  tests the Rust core, then the trusted deploy lane publishes a sha-tagged image
  the deploy repo pulls read-only.

## Known scope-shape signals

README lists nine inspirational sibling projects. Consult it when evaluating
scope adds. `docs/perf-rewrite.md` is load-bearing for the inner loop.

## See also

- [README.md](../README.md) - human-facing intro.
- [AGENTS.md](../AGENTS.md) - agent-facing operating rules.
- [.ward/ward.yaml](../.ward/ward.yaml) - allowlisted commands.

Cross-reference convention from agentic-os#59.
