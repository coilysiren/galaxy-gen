# Tick worker message protocol

Web Worker that owns its own `Galaxy` WASM instance and runs `tick` off the main thread.

## Main to worker

- `init` with `size, mass, velX, velY, fracX, fracY, stars, field, meta`. Hydrate a new Galaxy from transferred state. The `stars` (Float32Array), `field` (Float32Array), and `meta` (Uint32Array) buffers are opaque sim state - the worker restores them verbatim (stars, then field, then meta - order matters) and never interprets them.
- `start` with `timeModifier`. Begin looping. Tick, post snapshot, schedule next. Tick rate capped at 30/s.
- `setTimeModifier` with `timeModifier`. Live-update dt without stopping.
- `stop`. Halt the loop. Worker replies with final state for rehydration.

## Worker to main

- `snapshot` with `mass, fracX, fracY, tickMs, tickId, stars, transients, radiation, counters, stellarHaloMass, bhMass, gasColdFraction, lensScale`. Per-tick render snapshot: gas mass and sub-cell offsets, star render packing ([x, y, luminosity, colorIndex, stage, clusterId] per star), event transients ([kind, x, y, ticksAgo, magnitude] per recent Supernova/StarBirth/GammaRayBurst), the coarse radiation field, and live UI values including the cumulative association count. Typed arrays are transferred.
- `stopped` with `mass, velX, velY, fracX, fracY, stars, field, meta`. Final state after stop. The opaque buffers ride back so the main thread rehydrates the full sim (star population, coarse fields, scheduler tick count, RNG master seed, pending events) - the round-trip is byte-exact, guarded by a unit test.
