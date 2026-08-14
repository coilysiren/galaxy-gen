# The gas integrator

Semi-implicit Euler with a transfer scheme: cells carry sub-grid fractional offsets and hop to a neighbour when an offset crosses a half cell. Four decisions in it are load-bearing, and each was arrived at by breaking the sim in a specific way first.

## Flow relaxation, not drag

Velocity decays toward the local circular flow `u(r) = v_c(r) t_hat`, not toward rest. Same dt-scaled exponential, different attractor: a rotating disk rather than stillness. Plain drag toward rest froze every galaxy into a static blob by t=1000.

The halo centripetal pull and the boundary spring are applied in the integration step rather than in the force kernels, so the CPU, Barnes-Hut, and WebGPU paths all inherit them. The halo is what makes the circular flow an actual force equilibrium - relaxation alone would re-aim velocities the potential cannot sustain.

## The step cap clamps the vector norm

Not each axis. A per-axis clamp lets diagonal movers travel sqrt(2) faster than axis-aligned ones, which funnels every fast transit - bang ejecta especially - into four diagonal sectors and shreds rings into a four-blob pinwheel.

## Two passes, because one is not enough

**Pass 1 records intent.** Every cell integrates its velocity and writes down where it *wants* to go.

**Pass 2 resolves admissions iteratively, like a traffic wave.** A mover is admitted only when its destination has room counting residents *confirmed* to be leaving.

Both halves matter, and each failure mode was observed:

- Resolving in one sequential sweep made a full destination block its mover even when that resident was itself leaving this tick. Every dense cloud froze solid for motion toward higher indices (+x/+y). Bulk cloud motion - a convoy of full cells advancing together - needs the intent pass.
- Trusting mere intent is over-permissive in a jam, where everyone intends to move and nobody actually can, and it collapses whole clouds onto their leading edge into one mega-blob.

Iterating unwinds a convoy from its free end: the front car pulls away, the next fills the gap. Incompressibility semantics are unchanged - a genuinely full destination still parks the mover at its cell edge with velocity intact.

## Pressure overflow

Cells above `CELL_MASS_CAP` shed the excess to their four neighbours, carrying momentum with the shed mass. Without it a capped region gridlocks permanently - transfer rejection alone freezes `rms_radius` within ~500 ticks. The sweep is sequential and in-place, so a shed can cascade within the same tick, which just propagates the pressure wave faster.

## Conservative structured-gas transport

Grid parcels merge irreversibly whenever their paths meet, so acceleration alone cannot keep a diffuse cloud resolved. A post-advection flux moves excess density toward lower-density neighbours, and cooling gas drifts a bounded share down the arm potential or toward the ring annulus. Every transfer carries the source metal fraction and momentum exactly. The ring score is radial only, so it cannot introduce an azimuthal arm or bar into an axisymmetric scenario.

## See also

- [sim-constants.md](sim-constants.md) - the constants named here.
- [gas-forces.md](gas-forces.md) - what supplies the accelerations.
