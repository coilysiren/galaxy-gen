# Star-population metrics, and which ones to trust

The numbers `debug-sim` prints about the star layer. Two of them look interchangeable and are not, and one has cost a retraction.

## vsig - rotational support

`rotation_dispersion_ratio`: mean streaming speed over velocity dispersion, averaged across radial bins of the luminous disk.

**This is the metric that separates a disk from a mush**, because dispersion is what makes a mush a mush. Rotation-dominated disks run above ~1.5; pressure-supported spheroids sit below ~0.7.

Binned rather than global, because a disk's streaming speed varies with radius - pooling every radius charges the radial gradient to dispersion and understates rotation everywhere.

It pools every age, which is what "is the visible star layer a disk" asks.

## vsy / vsm / vso - the age split

`rotation_dispersion_ratio_for_age` restricted to an age window, and it separates the two ways a star layer loses rotational support - which the pooled number cannot tell apart:

- **Each cohort cold, only the mixture hot.** The dispersion was written in at birth and accumulates as generations pile up. No force is heating anything, and ablating forces will not move it.
- **Old cohorts hotter than young ones.** Something is heating stars after birth, and the gap between cohorts is the heating rate.

Both read as a low pooled `vsig`. The discriminator is the *cohort* number, not the pooled one.

## scirc - DO NOT TUNE AGAINST THIS

`star_circular_ratio`: mean tangential speed over the circular speed of the field the stars actually read.

**It cannot distinguish a circular disk orbit from an eccentric orbit caught at pericenter**, where tangential speed is high by construction, so it scores fast eccentric interlopers well - exactly the population that reads as noise on screen. galaxy-gen#66 has the numbers.

Still useful for the narrower question of whether stars carry the angular momentum their potential holds. 1.0 is balanced; above it they climb outward and pool past the disk edge, below it they fall inward, and either way the population randomizes. Disk stars only - halo stars are on plunging orbits by construction and would swamp it.

## bcirc - the same ratio at birth

`birth_circular_ratio`: what a newborn would be handed right now at a representative radius. Splits "born wrong" from "drifted wrong" - if birth is already off 1.0, the orbits never had a chance.

**DRAGON: this probe mirrors the birth site's clamps by hand.** #66 reported an uncapped ratio while births were actually capped, and the retraction cost more than the duplication does. Probe and call site move together.

## arm - arm tracing

`stellar_arm_affinity`: mean gas density at star positions over mean gas density across disk cells, per radial bin, averaged. 1.0 means stars are spread like area; above 1.0 means they prefer dense gas, which is what makes a spiral read as a spiral.

Binned by radius on purpose: both stars and gas concentrate toward the center, so a global ratio scores that radial concentration as arm tracing. Normalizing within each radius isolates the azimuthal question, which is the one that matters visually.

Deliberately blind to kinematics. A hot, thoroughly mixed population that still clusters on the arms scores well here and badly on `vsig`, and for the look this sim is after that is the right trade.

## sctr / sctry - central concentration

`central_fraction_for_age`. Against the all-ages number, this says whether a concentrated star field was *born* that way or drifted inward: young stars have not had time to move, so they track the radial shape of star formation itself. If the two agree, nothing is migrating.

## cden - the amount, where the rest are shape

`core_star_density`, resolved stars per unit area inside 0.35 disk radii. Every other bar on this page and all five spheroid measures divide by a total, so they describe how what remains is arranged and cannot see the population emptying underneath them.

Measured on galaxy-gen#7051: a luminosity floor on the elliptical retires 81% of the stars and 47% of the mass, and `econ` does not move. Across three seeds it read 1.02x, 1.06x, 0.84x of its unfloored value, and at a harsher floor it went *up*. **A count fraction fails the same way** - `core/total` on counts read 1.06x to 1.18x as the body emptied, because it is another normalized ratio. Only the unnormalized density tracked the damage, at 0.30x and then 0.13x.

**It encodes central concentration, so it belongs to the elliptical alone.** A ring is hollow by construction and a spiral keeps its light in the arms, so those two guard the same property with a plain minimum star count instead.

## See also

- [metric-calibration.md](metric-calibration.md) - why every one of these has a known-answer test.
- [stellar-heating.md](stellar-heating.md) - what they were built to measure.
- [ablation.md](ablation.md) - the harness that reads them.
