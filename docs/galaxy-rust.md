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
- `MAX_SUBGRID_STEP` is 1.0 - one cell per tick, the transfer scheme's hard ceiling - clamping the per-tick displacement VECTOR norm. A per-axis clamp let diagonal movers travel sqrt(2) faster than axis-aligned ones, which funneled every fast transit (bang ejecta) into four diagonal sectors and shredded rings into a 4-blob pinwheel.
- Gas dissipation is per-scenario flow relaxation, not plain drag: `v = u + (v - u) * exp(-flow_drag * dt)` where `u = flow_support * v_c(r)` is the local circular flow of a static halo rotation curve `v_c(r) = v_flat * r / sqrt(r^2 + rc^2)`. The halo also pulls `v_c^2 / r` inward (gas in `apply_acceleration`, stars via the coarse field), so circular flow is a force equilibrium. Dissipation therefore circularizes orbits instead of stopping them - a rotating disk is the attractor state, which is what keeps every scenario visibly rotating at t=1000. The old plain drag toward rest (and before it, flat 0.995/tick damping) froze every run into a static blob. `flow_support` below 1.0 leaves the gas chronically under-supported so it inspirals while rotating - the elliptical's concentration knob.
- `REPULSE_R2` is 2.0. Gravity flips repulsive at integer r-squared at or below it - a crude contact-pressure proxy, mirrored in the WGSL kernel via a params field. Placeholder until a real equation of state.
- `CELL_MASS_CAP` is 128. Transfers into a cell never pack it past this (incompressibility floor). A full destination rejects the mover, which parks at its cell edge with velocity intact (`BLOCKED_FRICTION` = 1.0, traffic-jam model - reflecting or damping thermalizes disk rotation). Admission resolves iteratively like a traffic wave: a mover is admitted when its destination's resident is CONFIRMED leaving, so a convoy of full cells unwinds from its free end and dense clouds translate and rotate as bodies instead of freezing solid. (Trusting mere intent is over-permissive in a jam and collapses whole clouds into one mega-blob; the strict single-sweep rule froze all +x/+y bulk motion.) Cells above the cap shed the excess to their four neighbors each tick (pressure overflow), so capped cores breathe instead of gridlocking.
- `CONFINE_STIFFNESS` is 0.02. Gas boundary spring: past the disk radius (size/2 - 1, the soft clip) cells feel a linear pull back toward the center. The toroidal wrap remains as a backstop only.
- `STAR_FIELD_SCALE` is 0.25: the coarse field stars read is built at quarter strength with its halo term on half the gas curve, so star orbits run at half the gas pace - fast pink rivers of gas around a slow drifting star population.
- Stars use a two-tier halo instead: between the soft clip and the hard clip (`HARD_CLIP_FACTOR` 3.0 x soft) a repulsive gradient `HALO_STIFFNESS x (r - soft)/(hard - r)` (clamped at `HALO_ACCEL_MAX`) diverges at the hard clip, so no finite speed reaches it. `STAR_HALO_DRAG` bleeds velocity only inside the band - the halo spring is conservative, and without dissipation ejecta would oscillate forever instead of rejoining the disk. The renderer fades matter from the soft clip to invisible by 1.5 x soft; the deep halo exists but never renders.

## Stars and the causal loop

Stars live in `src/rust/stars.rs`: struct-of-arrays, continuous f32 positions, stable u32 ids (indices reorder on swap-remove). They are collisionless - they bilinear-sample a coarse 64x64 acceleration field rebuilt every 4 ticks from gas + stars + a central black hole (5% of seeded mass), so they never jam and the star population costs O(N). Lifecycle: births sample a Salpeter-flavored IMF (dN/dm ~ m^-2.35, masses 3-120) so most stars are faint red dwarfs and giants are rare. Luminosity ~ m^2, lifetime = 900 x (30/m)^2 sim-time (M-dwarfs outlive the session, O-stars die in minutes), class_index is log-mass normalized 0..1 and the renderer maps it through the stellar-classification (OBAFGKM) color sequence. Heavy stars (mass >= 30) supernova, returning 80% of their mass to nearby gas with an outward kick and leaving a dim remnant; light stars fade to remnants. Collapse, birth, radiation, and shock tuning constants are documented inline in galaxy.rs; the loop walkthrough lives in [processes-events.md](processes-events.md).

## Black hole lifecycle

The central hole is live: `bh_accretion` (cadence 8) eats 1% of the gas within 2 cells of center per run and emits `BlackHoleCapture` for stars that are both inside the capture radius (0.5) and slow (< 0.8) - fast stars slingshot, which is what keeps the hole from eating the galaxy. `bh_evaporation` applies Hawking radiation with the physically-shaped dM/dt = -HAWKING_COEFF/M^2: negligible while fat, runaway once small, ending in a final flash - at a wildly exaggerated rate (a real stellar-mass hole radiates nanokelvins). Radiated mass exits through the `radiated_total` ledger sink and heats the core radiation field. The renderer's lens depth scales with sqrt(bh_mass / seeded mass), so the lens deepens as the hole feeds and vanishes if it evaporates.

## Scenarios and seeding

A `Scenario` is a hardcoded `start => end-shape` pair - not just an initial condition, because the physics constants that steer 1000 ticks of evolution (halo curve, flow relaxation, ejection geometry) belong to it too. Four ship, and the end shape is sturdy across seeds because noise only textures a scenario - the shape drivers are deterministic:

- `BangRing` - central explosion, symmetric ejection tuned to turn around near 0.62 disk_r, strong circularization parks it as a rotating ring with a hollow core.
- `BangSpiral` - explosion with an m=2 lobed ejection tilted 0.6 rad prograde (`eject_swirl`); direction is immune to the per-axis movement clamp, so the arms curl even while speed is capped.
- `IrregularSpiral` - domain-warped smoke noise with a two-arm log-spiral overdensity that differential rotation shears into a pinwheel.
- `IrregularElliptical` - smoke noise under an exponential radial envelope (real ellipticals are light-profile concentrated), weak rotation, high dispersion, sub-circular flow support so it stays compact and smooth.

The bang seeders deliberately fill the core BELOW the collapse density threshold (`core_fill_scale`): a dense core converts to stars before its ejecta travel anywhere, so the wide thin core only reaches star-forming density where mass piles up near the target radius - star formation happens IN the ring/arms. Ejection speed budgets the climb explicitly: self-gravity escape plus the halo potential difference to the target radius (`v_flat^2 * ln((rt^2 + rc^2) / rc^2)`).

The irregular seeder normalizes to a deterministic total gas budget: the fBm draw's mean varies +-35% seed to seed, and thin draws lose their seeded structure to dissipation long before t=1000, so per-seed budgets are what shape sturdiness is made of - noise only textures. The elliptical additionally applies a whisper of in-disk star drag (`star_drag`): stars are collisionless, so without it a young swarm slowly evaporates outward instead of settling into the central glow.

Every scenario seeds inside the disk radius and adds orbital support on top: `v = rotation_boost * sqrt(G * M_enc / r + v_c(r)^2)` tangentially, with M_enc prefix-summed over cells sorted by radius - the combined self-gravity + halo equilibrium speed. `seed_with_mode_seeded` gives byte-identical output for the same `(additional, scenario, seed)` - the `?seed=` URL invariant covers every scenario.

## Buffers

- `vel_x`, `vel_y`. Persistent per-cell velocity. Without persistence the sim restarts from rest each tick and produces imperceptible motion.
- `frac_x`, `frac_y`. Sub-grid fractional offsets so a cell accumulates toward its next grid cell across ticks rather than snapping.
- `xs_i`, `ys_i`. Integer cell positions. Integer diffs let us index an inv-r-cubed lookup with r squared, no `sqrt` in the hot loop.
- `inv_r3`. Precomputed `g * (r squared + soft) ^ (-3/2)` indexed by integer r squared. Populated in `new()`, reused across seeds and ticks.
- `scratch_mass`. Reused across ticks.
