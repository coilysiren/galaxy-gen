# Stars, associations, and the spheroid

How the star population is stored, born, aged, and retired, and how the elliptical scenario turns that population into a pressure-supported spheroid.

## Stars and the causal loop

Stars live in `src/rust/stars.rs`: struct-of-arrays, continuous f32 positions, stable u32 ids, compact-binary ids, lifecycle stages, and halo-dwell counters. Indices reorder on swap-remove. They are collisionless and bilinear-sample a coarse 64x64 acceleration field rebuilt every 4 ticks from gas, stars, and a central black hole, so they never jam and their integration cost stays O(N). Births sample a Salpeter-flavored IMF over masses 3-120. Luminosity is approximately m^2, lifetime is `900 x (30/m)^2` sim-time, and the renderer maps log-mass `class_index` through stellar-classification colors.

Nearby collapse events now form one temporary stellar association. A batch begins in a compact circular footprint and receives one shared galactic orbit. Radial gas inflow is mostly discarded, prograde gas rotation is retained, and an azimuthal average of the live field plus a smooth halo and black-hole floor supplies orbital support. Momentum-neutral internal spin and a softened center-of-mass binding force keep the association legible without propelling it. Binding fades with age. After an embedded grace period, members beyond the local tidal radius lose their association id while keeping their exact position and velocity, so they become ordinary stream stars. Full details: [stellar-associations.md](stellar-associations.md).

Light stars fade into dim resolved remnants, then retire into the diffuse stellar halo. Heavy birth draws (mass >= 30) split into equal partners with a shared binary id. The pair keeps the original system's lifetime and core-collapse fate. Its components supernova independently, return about 80% of their mass to nearby gas, and leave neutron stars. After both supernovae and a deterministic seed-derived delay, `NeutronStarMerger` combines the pair into one remnant, accounts for 1% radiated mass, and emits a short `GammaRayBurst`. The complete event and reservoir path is documented in [processes-events.md](processes-events.md).

## Elliptical relaxation

The irregular-to-elliptical scenario has no spiral or ring force. Its seeded cloud starts with an exponential radial envelope, sub-circular bulk flow, and high velocity dispersion. Scenario pressure (`gas_pressure = 0.35`) keeps the assembling gas resolved instead of allowing a few saturated cells to consume the field. A lower collapse threshold (`collapse_density_fraction = 0.4`) and probability (`collapse_chance = 0.2`) spread stellar associations through that reservoir. Stars then remain collisionless and phase-mix under the shared coarse field while weak drag removes enough ordered motion to leave a pressure-supported spheroid.

Five stellar metrics separate that result from a point mass, bar, ring, or spiral: inner mass concentration, low-order angular smoothness, projected inertia-axis ratio, mass-weighted RMS extent, and net rotational support. The fixed size-50, seed-42 acceptance follows every tick from 900 through 1000 and also requires resolved gas, at least 500 stars, continued births, continued phase mixing, and low spiral and ring signatures. Full model and verification details: [elliptical-relaxation.md](elliptical-relaxation.md).
