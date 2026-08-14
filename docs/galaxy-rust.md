# Galaxy simulation internals

Cell grid with Newtonian gravity, in-place tick.

## Layout

Struct-of-arrays (parallel `Vec<f32>` / `Vec<u16>`) so the physics inner loop is a tight numeric kernel the optimizer can auto-vectorize. Acceleration accumulates in cartesian (ax, ay). The old polar representation required four trig calls per pair, which dominated tick cost.

## Hot path

`tick()` runs two passes:

- `gravitate_all()`. O(N squared / 2) pair sweep, symmetric per Newton's third law. Skips mass=0 on either side.
- `apply_acceleration()`. Integrate one step, reassign cells to destination grid indices, accumulate mass on collision. Uses a `Vec<u32>` (size N squared) instead of a `HashMap` to coalesce masses.

`tick` returns a new `Galaxy` to preserve the JS API, but internally reuses scratch buffers and moves the resulting arrays.

## Buffers

- `vel_x`, `vel_y`. Persistent per-cell velocity. Without persistence the sim restarts from rest each tick and produces imperceptible motion.
- `frac_x`, `frac_y`. Sub-grid fractional offsets so a cell accumulates toward its next grid cell across ticks rather than snapping. Worker snapshots carry them to the canvas renderer, so visible clouds move continuously between integer cell transfers.
- `xs_i`, `ys_i`. Integer cell positions. Integer diffs let us index an inv-r-cubed lookup with r squared, no `sqrt` in the hot loop.
- `inv_r3`. Precomputed `g * (r squared + soft) ^ (-3/2)` indexed by integer r squared. Populated in `new()`, reused across seeds and ticks.
- `scratch_mass`. Reused across ticks.

## The rest of the model

- [sim-constants.md](sim-constants.md) - every tuned constant and what it holds up.
- [scenarios.md](scenarios.md) - the four `start => end-shape` pairs and their seeders.
- [gas-forces.md](gas-forces.md) - fountain, ring wave, spiral wave.
- [stellar-model.md](stellar-model.md) - star storage, births, associations, the spheroid.
- [black-hole.md](black-hole.md) - accretion, quasar episodes, evaporation.
- [processes-events.md](processes-events.md) - the scheduler and the event queue.
- [integrator.md](integrator.md) - the gas integrator's four load-bearing decisions.
- [seeding.md](seeding.md) - how an initial condition is built.
- [stellar-population.md](stellar-population.md) - the IMF and the resolved-luminosity floor.
- [stellar-heating.md](stellar-heating.md) - birth orbits and rotational support.
- [star-metrics.md](star-metrics.md) - which star metrics to trust.
- [boundary-ridge.md](boundary-ridge.md) - gas confinement and the star halo.
