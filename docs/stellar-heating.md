# Why the stellar disk stops rotating, and what fixed it

The stellar disk used to start rotation-dominated and end a pressure-supported mush by t=2500, burying good gas structure under 40k non-rotating points. The gas was always fine. Evidence: galaxy-gen#70, compiling #65 and #66.

## Two independently sufficient causes

Stars were put on orbits the potential they read cannot hold, and two separate mechanisms converted that mismatch into random motion. **Either is sufficient on its own, which is why one-at-a-time ablation found nothing.**

1. **The birth orbit was wrong.** `ASSOCIATION_ORBIT_SPEED_CAP` is absolute and bound on nearly every birth, handing newborns 2-3x their own field's circular speed - past the ~1.41 escape ratio. Angular momentum is conserved in a torque-free potential, so that mismatch never decays.
2. **The field was lumpy.** Stars integrate against a coarse field rebuilt from a clump-dominated quadtree. A perfectly circular birth orbit gets scattered apart by it anyway.

An empty single-factor sweep is not an absence of causes.

## What ships

- `birth_orbit_ratio_cap` (1.06 everywhere) clamps a newborn's orbital speed to a multiple of *its own* circular speed. The absolute cap stays as a backstop for degenerate radii.
- `STAR_FIELD_AXISYMMETRIC` replaces the field stars read with its azimuthal average, preserving the radial profile and therefore the circular speed at every radius.
- `birth_velocity_dispersion` gives the elliptical its dispersion on purpose (1.5) rather than borrowing the side effect of a bad cap. Disks carry 0.3.
- `COLLAPSE_RADIATION_RESIST` rises 20 to 80 for disks: capped stars stay in the disk and irradiate the gas that would form the next generation, throttling formation 44%.

Measured at size 500, t=2500, two seeds - `vsig` is rotational support, above ~1.5 is a disk and below ~0.7 is a mush:

| scenario | before | after |
|---|---|---|
| irregular => spiral | 0.48 / 0.31 | **2.50 / 3.30** |
| bang => spiral | - | 2.49 / 2.75 |
| irregular => elliptical | 0.52 / 0.49 (the target) | **0.58 / 0.45** |

The disk holds to t=5000 (`vsig` 1.83, old cohort 1.51), no crossover at any checkpoint, and the population settles near 20k: #70 removed a bad sink and #72's floor added a good one, and they nearly cancel.

## Losing the arms was the fear, and it was unfounded

Axisymmetrizing the field removes the arms from the *potential*, not from the picture. `stellar_arm_affinity` is unchanged or better. **Stars trace arms because they are born in the arms, where the gas collapses - not because the field they orbit in has arms.**

Relatedly, a coherent analytic wave does not heat stars even at 0.70 coupling, while the field's own arms do. The difference is coherence, not amplitude: a rigid pattern is something stars pass through; the gas's mass distribution churns underneath them. `STAR_WAVE_COUPLING` stays 0.0 - it cost 10-20% of support and bought no arm tracing.

## Two dragons

- **The elliptical's spheroid used to be produced by the birth bug.** Its pressure support came from stars being launched past escape. Fixing the cap in isolation makes it rotate (`vsig` 3.08); it needs `birth_velocity_dispersion` in the same change.
- **The elliptical keeps the old radiation gate.** At 80 its extra supernovae sweep gas into an annulus and it fails its own ring-signature check.

## Still open

- Central over-concentration below size 250 (#70 item 4). The sim's length constants are absolute cell counts, so size 150 is a different physical setup, not a smaller picture. `GALAXY_ABL_LENGTH_REFERENCE_SIZE` measures the alternative.
- The elliptical has no resolved-luminosity floor, so its population is unbounded (56k at t=5000). It needs its retired light rendered first - galaxy-gen#72.

## See also

- [ablation.md](ablation.md) - the harness, and [ablation-switches.md](ablation-switches.md) to re-measure any of this.
- [stellar-associations.md](stellar-associations.md) - how a birth batch gets its orbit.
