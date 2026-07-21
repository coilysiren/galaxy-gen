# Galaxy simulation internals

Cell grid with Newtonian gravity, in-place tick.

## Layout

Struct-of-arrays (parallel `Vec<f32>` / `Vec<u16>`) so the physics inner loop is a tight numeric kernel the optimizer can auto-vectorize. Acceleration accumulates in cartesian (ax, ay). The old polar representation required four trig calls per pair, which dominated tick cost.

## Hot path

`tick()` runs two passes:

- `gravitate_all()`. O(N squared / 2) pair sweep, symmetric per Newton's third law. Skips mass=0 on either side.
- `apply_acceleration()`. Integrate one step, reassign cells to destination grid indices, accumulate mass on collision. Uses a `Vec<u32>` (size N squared) instead of a `HashMap` to coalesce masses.

`tick` returns a new `Galaxy` to preserve the JS API, but internally reuses scratch buffers and moves the resulting arrays.

## Constants

- `GRAVATIONAL_CONSTANT` is 5.0e-4. Newton's G of 6.67e-11 is numerically invisible at this grid scale. Tuned so circular-orbit speeds fit under `MAX_SUBGRID_STEP` at default dt - the old 5.0e-2 demanded orbital speeds ~10x the movement cap, so every initial condition free-fell to the center at terminal speed.
- `SOFTENING_SQ` is 1.0. Avoids division by ~0 when cells share a grid cell.
- `MAX_SUBGRID_STEP` is 0.5. Caps per-tick position delta so we don't teleport across the grid on a tight mass concentration.
- `DRAG_COEFF` is 0.001, applied as `v *= exp(-DRAG_COEFF * dt)` per tick. Keeps the grid-quantized sim from overheating at large dt while staying weak enough that rotation holds for minutes of wall-clock. The old flat `0.995`/tick damping halved velocity every second.
- `REPULSE_R2` is 2.0. Gravity flips repulsive at integer r-squared at or below it - a crude contact-pressure proxy, mirrored in the WGSL kernel via a params field. Placeholder until a real equation of state.
- `CELL_MASS_CAP` is 128. Transfers into a cell never pack it past this (incompressibility floor). A full destination rejects the mover, which parks at its cell edge with velocity intact (`BLOCKED_FRICTION` = 1.0, traffic-jam model - reflecting or damping thermalizes disk rotation). Cells above the cap shed the excess to their four neighbors each tick (pressure overflow), so capped cores breathe instead of gridlocking.
- `CONFINE_STIFFNESS` is 0.02. Gas boundary spring: past the disk radius (size/2 - 1, the soft clip) cells feel a linear pull back toward the center. The toroidal wrap remains as a backstop only.
- Stars use a two-tier halo instead: between the soft clip and the hard clip (`HARD_CLIP_FACTOR` 3.0 x soft) a repulsive gradient `HALO_STIFFNESS x (r - soft)/(hard - r)` (clamped at `HALO_ACCEL_MAX`) diverges at the hard clip, so no finite speed reaches it. `STAR_HALO_DRAG` bleeds velocity only inside the band - the halo spring is conservative, and without dissipation ejecta would oscillate forever instead of rejoining the disk. The renderer fades matter from the soft clip to invisible by 1.5 x soft; the deep halo exists but never renders.

## Stars and the causal loop

Stars live in `src/rust/stars.rs`: struct-of-arrays, continuous f32 positions, stable u32 ids (indices reorder on swap-remove). They are collisionless - they bilinear-sample a coarse 64x64 acceleration field rebuilt every 4 ticks from gas + stars + a central black hole (5% of seeded mass), so they never jam and the star population costs O(N). Lifecycle: lifetime = 40000/mass; heavy stars (mass >= 60) supernova, returning 80% of their mass to nearby gas with an outward kick and leaving a dim remnant; light stars fade to remnants. Collapse, birth, radiation, and shock tuning constants are documented inline in galaxy.rs; the loop walkthrough lives in [processes-events.md](processes-events.md).

## Seeding

Every mode seeds inside the disk radius and then adds circular-orbit support on top: v += sqrt(G * M_enc / r) tangentially, with M_enc prefix-summed over cells sorted by radius. `seed_with_mode_seeded` gives byte-identical output for the same `(additional, mode, seed)` - the `?seed=` URL invariant covers every mode.

## Buffers

- `vel_x`, `vel_y`. Persistent per-cell velocity. Without persistence the sim restarts from rest each tick and produces imperceptible motion.
- `frac_x`, `frac_y`. Sub-grid fractional offsets so a cell accumulates toward its next grid cell across ticks rather than snapping.
- `xs_i`, `ys_i`. Integer cell positions. Integer diffs let us index an inv-r-cubed lookup with r squared, no `sqrt` in the hot loop.
- `inv_r3`. Precomputed `g * (r squared + soft) ^ (-3/2)` indexed by integer r squared. Populated in `new()`, reused across seeds and ticks.
- `scratch_mass`. Reused across ticks.
