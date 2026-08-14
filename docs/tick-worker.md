# Tick worker message protocol

Web Worker that owns its own `Galaxy` WASM instance and runs `tick` off the main thread.

## Main to worker

- `init` with `size, mass, velX, velY, fracX, fracY, stars, field, meta`. Hydrate a new Galaxy from transferred state. The `stars` (Float32Array), `field` (Float32Array), and `meta` (Uint32Array) buffers are opaque sim state - the worker restores them verbatim (stars, then field, then meta - order matters) and never interprets them.
- `start` with `timeModifier`. Begin looping. Tick, post snapshot, schedule next. Tick rate capped at 20/s.
- `setTimeModifier` with `timeModifier`. Live-update dt without stopping.
- `stop`. Halt the loop. Worker replies with final state for rehydration.

## Worker to main

- `snapshot` with `mass, fracX, fracY, tickMs, tickId, stars, transients, radiation, metallicity, counters, stellarHaloMass, bhMass, gasColdFraction, lensScale`. Per-tick render snapshot: gas mass, sub-cell offsets, per-cell heavy-element fraction, star render packing ([x, y, luminosity, colorIndex, stage, clusterId, age] per star), recent event transients, the coarse radiation field, live stellar-stage populations, and cumulative event counts. Typed arrays are transferred.
- `stopped` with `mass, velX, velY, fracX, fracY, stars, field, meta`. Final state after stop. The opaque buffers ride back so the main thread rehydrates the full sim (star population, coarse fields, scheduler tick count, RNG master seed, pending events) - the round-trip is byte-exact, guarded by a unit test.

## Flat layouts

The opaque buffers are flat arrays with a fixed field order. Changing any of these is a serialization change and breaks the round-trip test.

- Stars, f32 per star: `[x, y, vx, vy, mass, age, lifetime, stage, luminosity, color_index, cluster_id, binary_id, halo_dwell, id, metal_mass]`. Integer ids survive f32 because live ids stay far below 2^24, and the `u32::MAX` sentinels round-trip through Rust's saturating float cast.
- Star render packing, f32 per star: `[x, y, luminosity, color_index, stage, cluster_id, age]`. The renderer derives size, birth reveal, and association glow from these. Nothing flows back into the simulation.
- Events, u32: `[next_id lo/hi, seq_tick lo/hi, seq_in_tick, n_pending]` then 12 u32 per pending event. The instrumentation ring and counters are dropped on purpose - they are diagnostics, not sim state.
