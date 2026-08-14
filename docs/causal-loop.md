# The causal loop

Registry order is the causal chain, and the chain closes: gas makes stars, stars change gas, and the black hole feeds on both. This is the end-to-end walk.

## Order of operations

Registry order (also the golden-ordering test): gravity, spiral_density_wave, ring_density_wave, gas_pressure, quasar_feedback, gravity_field, integrate_gas, integrate_stars, radiation_field, collapse_watch, stellar_halo, stellar_aging, bh_accretion, bh_evaporation, gas_dissipation, gas_fountain. Motion runs every tick, fields every 4 ticks, and lifecycle rules every 8-16 ticks.

## Gas to stars and back

The loop, end to end: gas clumps under gravity -> scenario density waves gather gas into spiral lanes or an annulus, while the elliptical scenario assembles without a wave -> gas pressure and conservative transport keep the reservoir resolved -> dense, cool cells accumulate collapse heat -> CloudCollapse consumes gas into a birth budget -> StarBirth joins or creates a temporary stellar association whose masses sum exactly to the budget -> stars deposit radiation, which resists further collapse and lifts hot gas out of the visible disk -> the galactic fountain cools halo gas back into moving disk filaments while its cold share follows a deterministic 40-60% limit cycle -> stellar_aging retires light stars to temporary remnants and detonates heavy ones -> Supernova returns about 80% of the star's mass to nearby gas, leaves a neutron star, and emits ShockWave -> the shock boosts collapse heat around the blast and preserves causal parentage for any induced CloudCollapse and StarBirth.

## The black-hole branch

The black-hole branch closes another feedback loop: nuclear viscosity delivers low-angular-momentum gas -> `bh_accretion` grows the hole and smooths its feeding rate -> sustained growth emits `QuasarIgnition` and starts a brief persisted active-nucleus state -> `quasar_feedback` pulses opposed gas acceleration and ionizing radiation before gas integration -> `gas_dissipation` lifts irradiated material into the hot halo -> `gas_fountain` can later return that same accounted mass to the disk. The renderer reads the persisted activity, age, pulse strength, and axis, not the one-tick ignition event. See [quasar-feedback.md](quasar-feedback.md).

## Where the scenario forces sit

`spiral_density_wave` and `ring_density_wave` write their scenario force to gas acceleration only. `gas_pressure` follows as the shared isothermal density-gradient response, including for the wave-free elliptical scenario. None alters the coarse field consumed by stars. The following `integrate_gas` process applies conservative neighbor pressure flux and bounded cold-gas transport down an active arm or annular potential when present. Mass, heavy elements, and linear momentum remain closed across these transfers. See [spiral-density-waves.md](spiral-density-waves.md), [ring-density-waves.md](ring-density-waves.md), and [elliptical-relaxation.md](elliptical-relaxation.md).
