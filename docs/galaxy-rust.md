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
- `REPULSE_R2` is 2.0. Gravity flips repulsive at integer r-squared at or below it, a contact-pressure proxy mirrored in the WGSL kernel via a params field. Wave-bearing disks additionally use the conservative gas-pressure model described below.
- `CELL_MASS_CAP` is 128. Transfers into a cell never pack it past this (incompressibility floor). A full destination rejects the mover, which parks at its cell edge with velocity intact (`BLOCKED_FRICTION` = 1.0, traffic-jam model - reflecting or damping thermalizes disk rotation). Admission resolves iteratively like a traffic wave: a mover is admitted when its destination's resident is CONFIRMED leaving, so a convoy of full cells unwinds from its free end and dense clouds translate and rotate as bodies instead of freezing solid. (Trusting mere intent is over-permissive in a jam and collapses whole clouds into one mega-blob. The strict single-sweep rule froze all +x/+y bulk motion.) Cells above the cap shed the excess to their four neighbors each tick (pressure overflow), so capped cores breathe instead of gridlocking.
- `CONFINE_STIFFNESS` is 0.02. Gas boundary spring: past the disk radius (size/2 - 1, the soft clip) cells feel a linear pull back toward the center. The toroidal wrap remains as a backstop only.
- `STAR_FIELD_SCALE` is 0.25: the coarse field stars read is built at quarter strength with its halo term on half the gas curve, so star orbits run at half the gas pace - fast pink rivers of gas around a slow drifting star population.
- Stars use a two-tier halo instead: between the soft clip and the hard clip (`HARD_CLIP_FACTOR` 3.0 x soft) a repulsive gradient `HALO_STIFFNESS x (r - soft)/(hard - r)` (clamped at `HALO_ACCEL_MAX`) diverges at the hard clip, so no finite speed reaches it. `STAR_HALO_DRAG` bleeds velocity only inside the band. A star that remains beyond 1.18 x disk radius for eight lifecycle scans phase-mixes into the unresolved stellar halo before it can advertise the numerical backstop. The renderer starts fading at 0.88 x soft, reaches zero at 1.32 x soft, and leaves black sky before the canvas edge.

## Galactic fountain

Radiation-dissipated gas is not destroyed. It moves into `halo_gas_mass`, a hot circumgalactic reservoir serialized with the rest of the simulation. `gas_fountain` runs every eight ticks and drives the cold share of active gas around a 480-tick 40-60% limit cycle. Feedback lifts irradiated cells first. Cooling returns small parcels to existing moving disk filaments before sparsely seeding empty annular cells, with circular velocity and a slight inward drift. The exchange changes visibility and density without creating baryons, and the UI reports `cold / (cold + halo)`.

## Spiral density waves

The two spiral scenarios carry an ongoing two-arm logarithmic potential, not only a spiral-shaped seed. `spiral_density_wave` runs after gravity every tick and adds acceleration normal to the current arm phase. A rigid pattern phase advances independently of individual gas orbits, so rotating gas passes through compression lanes instead of carrying a painted arm with it. Stars do not read this force.

The cell integrator would otherwise merge colliding parcels irreversibly. Wave-bearing disks therefore resolve isothermal pressure in two conservative forms. A density-gradient acceleration pushes cloud edges outward, then a post-advection mass flux spreads excess density toward lower-density neighbors. Cooling gas also moves a bounded share down the local arm potential. Both transfers carry the source metal fraction and momentum, and neither creates gas. The potential gathers diffuse gas into broad lanes while pressure prevents those lanes from becoming point knots. Existing collapse scans turn sustained dense, cool lane cells into stellar associations. Full model and verification details: [spiral-density-waves.md](spiral-density-waves.md).

## Stars and the causal loop

Stars live in `src/rust/stars.rs`: struct-of-arrays, continuous f32 positions, stable u32 ids, compact-binary ids, lifecycle stages, and halo-dwell counters. Indices reorder on swap-remove. They are collisionless and bilinear-sample a coarse 64x64 acceleration field rebuilt every 4 ticks from gas, stars, and a central black hole, so they never jam and their integration cost stays O(N). Births sample a Salpeter-flavored IMF over masses 3-120. Luminosity is approximately m^2, lifetime is `900 x (30/m)^2` sim-time, and the renderer maps log-mass `class_index` through stellar-classification colors.

Nearby collapse events now form one temporary stellar association. A batch begins in a compact circular footprint and receives one shared galactic orbit. Radial gas inflow is mostly discarded, prograde gas rotation is retained, and an azimuthal average of the live field plus a smooth halo and black-hole floor supplies orbital support. Momentum-neutral internal spin and a softened center-of-mass binding force keep the association legible without propelling it. Binding fades with age. After an embedded grace period, members beyond the local tidal radius lose their association id while keeping their exact position and velocity, so they become ordinary stream stars. Full details: [stellar-associations.md](stellar-associations.md).

Light stars fade into dim resolved remnants, then retire into the diffuse stellar halo. Heavy birth draws (mass >= 30) split into equal partners with a shared binary id. The pair keeps the original system's lifetime and core-collapse fate. Its components supernova independently, return about 80% of their mass to nearby gas, and leave neutron stars. After both supernovae and a deterministic seed-derived delay, `NeutronStarMerger` combines the pair into one remnant, accounts for 1% radiated mass, and emits a short `GammaRayBurst`. The complete event and reservoir path is documented in [processes-events.md](processes-events.md).

## Black hole lifecycle

The central hole is live in both particle systems. The shared gas integrator adds its softened point-mass acceleration, so CPU, Barnes-Hut, and WebGPU force paths all include the same nuclear potential. `bh_accretion` (cadence 8) applies weak tangential viscosity inside 7 cells and only removes gas inside the 2-cell sink. Accretion is weighted toward low-angular-momentum gas, letting a nuclear ring orbit visibly while leaking inward instead of disappearing wholesale. `BlackHoleCapture` still requires a star to be both inside radius 0.5 and slow (< 0.8), so fast stars slingshot.

`bh_evaporation` applies Hawking radiation with the physically shaped `dM/dt = -HAWKING_COEFF/M^2`: negligible while fat and runaway once small, ending in a final flash at a deliberately exaggerated rate. Radiated mass exits through the `radiated_total` ledger sink and heats the core radiation field. The renderer's lens depth scales with `sqrt(bh_mass / seeded mass)`, so the lens deepens as the hole feeds and vanishes if it evaporates.

## Scenarios and seeding

A `Scenario` is a hardcoded `start => end-shape` pair - not just an initial condition, because the physics constants that steer its evolution (halo curve, flow relaxation, ejection geometry, and any density wave) belong to it too. Four ship, and the end shape is sturdy across seeds because noise only textures a scenario - the shape drivers are deterministic:

- `BangRing` - central explosion, symmetric ejection tuned to turn around near 0.62 disk_r, strong circularization parks it as a rotating ring with a hollow core.
- `BangSpiral` - explosion with an m=2 lobed ejection tilted 0.6 rad prograde (`eject_swirl`). A persistent rotating logarithmic wave gathers the ejecta into broad star-forming arms between about ticks 500 and 1000 and can preserve later arm windows.
- `IrregularSpiral` - domain-warped smoke noise with a seeded two-arm overdensity. The same ongoing wave and pressure model lets the disturbed disk settle into long-lived arms, typically later than the bang start.
- `IrregularElliptical` - smoke noise under an exponential radial envelope (real ellipticals are light-profile concentrated), weak rotation, high dispersion, sub-circular flow support so it stays compact and smooth.

The bang seeders deliberately fill the core BELOW the collapse density threshold (`core_fill_scale`): a dense core converts to stars before its ejecta travel anywhere, so the wide thin core only reaches star-forming density where mass piles up near the target radius - star formation happens IN the ring/arms. Ejection speed budgets the climb explicitly: self-gravity escape plus the halo potential difference to the target radius (`v_flat^2 * ln((rt^2 + rc^2) / rc^2)`).

The irregular seeder normalizes to a deterministic total gas budget: the fBm draw's mean varies +-35% seed to seed, and thin draws lose their seeded structure to dissipation long before t=1000, so per-seed budgets are what shape sturdiness is made of - noise only textures. The elliptical additionally applies a whisper of in-disk star drag (`star_drag`): stars are collisionless, so without it a young swarm slowly evaporates outward instead of settling into the central glow.

Every scenario seeds inside the disk radius and adds orbital support on top: `v = rotation_boost * sqrt(G * M_enc / r + v_c(r)^2)` tangentially, with M_enc prefix-summed over cells sorted by radius - the combined self-gravity + halo equilibrium speed. `seed_with_mode_seeded` gives byte-identical output for the same `(additional, scenario, seed)` - the `?seed=` URL invariant covers every scenario.

## Buffers

- `vel_x`, `vel_y`. Persistent per-cell velocity. Without persistence the sim restarts from rest each tick and produces imperceptible motion.
- `frac_x`, `frac_y`. Sub-grid fractional offsets so a cell accumulates toward its next grid cell across ticks rather than snapping. Worker snapshots carry them to the canvas renderer, so visible clouds move continuously between integer cell transfers.
- `xs_i`, `ys_i`. Integer cell positions. Integer diffs let us index an inv-r-cubed lookup with r squared, no `sqrt` in the hot loop.
- `inv_r3`. Precomputed `g * (r squared + soft) ^ (-3/2)` indexed by integer r squared. Populated in `new()`, reused across seeds and ticks.
- `scratch_mass`. Reused across ticks.
