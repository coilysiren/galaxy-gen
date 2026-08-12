# Why each ablation switch exists

Per-switch reasoning behind [ablation-switches.md](ablation-switches.md). The
switch list says what each one does. This says why it is worth a run, which is
the part that goes stale in a code comment but not in a doc.

## Field switches

`FIELD_CADENCE`, `FIELD_SMOOTH`, `AXISYMMETRIC_FIELD`, and
`NO_STAR_SELF_GRAVITY` isolate the coarse gravity field. Stars integrate
against a field up to three ticks stale, built from a quadtree stars
themselves are in. Cadence tests staleness, smoothing tests clump texture,
axisymmetrization bounds how much heating comes from non-axisymmetric
structure of any kind, and dropping stars from the tree separates stellar
self-gravity from gas clumpiness.

## Association switches

`NO_ASSOCIATION_BINDING` and `NO_BIRTH_DISPERSION` cover the other live force
path. Associations still form, release, and stream with binding off, so the
switch isolates the potential rather than the lifecycle.

## `BIRTH_ORBIT_RATIO_CAP`

Not a force ablation. It was built and reverted on galaxy-gen#66 before
`rotation_dispersion_ratio` existed, and judged against `star_circular_ratio`,
which was later retracted as unable to tell a circular orbit from an eccentric
one at pericenter. Its effect on the disk has therefore never been measured. A
switch is the cheapest way to measure it without re-landing a change that
breaks the elliptical scenario.

## `STAR_WAVE_COUPLING`

The other half of the pair the ablation matrix pointed at. An axisymmetric
field holds the disk but has no arms in it, so the question is whether a
coherent analytic wave can put the arms back without heating the way a
clump-dominated 64-grid field does. A density wave stars pass through should
not scatter them.

## `NO_COLLAPSE_RADIATION_RESIST`

Not a heating candidate. It tests the star-formation drop that arrives with
the birth ratio cap: capped stars stay in the disk instead of being flung into
the halo, and the suspicion is that their radiation then suppresses the
collapses that would have made the next generation. If collapse counts recover
with the gate off, that loop is the mechanism.

## `BIRTH_VELOCITY_DISPERSION`

A pressure-supported spheroid is defined by having dispersion comparable to its
rotation. The elliptical scenario currently gets that for free from the
birth-speed bug: stars launched at 2-3x circular scatter into a spheroid. Cap
the births and it collapses into a small rotating core. This switch asks
whether giving the scenario the dispersion explicitly, which is what it
physically wants, rebuilds the spheroid without the bug.

## `LENGTH_REFERENCE_SIZE`

The sim's length scales are absolute cell counts while `disk_r` is not, so an
association is three times larger relative to the disk at size 150 than at size
500, and the coarse field is three times softer relative to it. Size 150 is a
different physical setup, not a smaller picture of the same one, which is why
the scenario tests and the deployed site disagree. This switch measures what
making them proportional would buy before anyone commits to the refactor. See
galaxy-gen#70.

## `RESOLVED_LUMINOSITY_FLOOR`

The floor ships on by default; the switch exists only to re-measure it. `0`
disables retirement entirely, the control the galaxy-gen#72 numbers were taken
against.

## See also

- [ablation.md](ablation.md) - how to run the matrix and what it found.
- [ablation-switches.md](ablation-switches.md) - the switch list.
