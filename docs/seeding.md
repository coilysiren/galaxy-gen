# Seeding

How a scenario's initial condition is built. Same `(additional, scenario, seed)` gives byte-identical state, which is what `?seed=` sharing rests on.

## The irregular seeder: smoke

Three fBm stacks - density plus two warp components - each four octaves of smoothstep value noise from the seeded RNG. A domain warp samples density through a noise displacement so structure curls into wisps and billows instead of pooling, and a power-law contrast exponent carves thin filaments and true voids out of what would otherwise read as smooth milk.

Normalized fBm clusters tightly around its mean, so it is stretched about a slightly dark center - voids reach true zero, billows saturate - and *then* shaped with the power law. Order matters.

**The total gas budget is normalized to a deterministic value.** The fBm draw's mean varies +-35% seed to seed, and thin draws lose their seeded structure to dissipation long before t=1000. Fixing the budget is what makes the end shape sturdy across seeds: noise only textures a scenario, it does not decide its shape.

A two-arm logarithmic-spiral overdensity seeds the density wave that differential rotation shears into a pinwheel.

## The bang seeder: ejection

Ejection speed is keyed to the seeded core's own escape velocity. A fixed speed stops scaling once core mass grows with size squared, and the explosion jams into a ball.

The climb energy budgets the trip explicitly: self-gravity escape **plus the halo potential difference**, `2 dPhi = v_flat^2 * ln((rt^2 + rc^2) / rc^2)`. Without the halo term the ejecta stall far short of the intended ring.

Two-lobed ejection gives `bang => spiral` its arms - the fast lobes race ahead and differential rotation winds them. Zero lobe depth gives the symmetric shell that the ring scenario circularizes. Direction is radial tilted prograde by the swirl angle, with generous speed jitter to break up the diagonal-travel grid artifact.

## Orbital support, every scenario

On top of the seeder's own velocities: `v = boost * sqrt(G*M_enc/r + v_c(r)^2)` tangentially, with `M_enc` prefix-summed over cells sorted by radius and `v_c` the halo rotation curve. That is the *combined* self-gravity-plus-halo equilibrium speed, not self-gravity's alone.

A hand-tuned linear ramp under-spins the disk and it free-falls to the center within a few hundred ticks.

## See also

- [scenarios.md](scenarios.md) - what each scenario is aiming at.
- [sim-constants.md](sim-constants.md) - the smoke and ejection constants.
