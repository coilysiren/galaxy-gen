# Ablation harness

A way to ask which force is responsible for something, by switching one
candidate off and re-running, instead of tuning a constant and hoping the
result is attributable.

Run the matrix:

```bash
ward exec ablation-sweep                       # 2500 ticks, size 500, 2 seeds
ward exec ablation-sweep 5000 500 3 12345 2    # ticks size seeds start-seed scenario
```

Or set a switch on a single `debug-sim` run:

```bash
GALAXY_ABL_AXISYMMETRIC_FIELD=1 ward exec debug-sim 2500 500 2 12345 2
```

Every switch defaults to off. The wasm build reads no environment and always
runs the shipped physics, and the golden mass field test proves a default
native build does too. `debug-sim` prints the resolved configuration as its
first line, so a captured run always records the physics that produced it.

## Switches

| Environment variable | Effect |
|---|---|
| `GALAXY_ABL_FIELD_CADENCE` | Rebuild the coarse star field every N ticks instead of 4. `1` removes field staleness. |
| `GALAXY_ABL_FIELD_SMOOTH` | N 3x3 box-blur passes over the field. Removes cell-scale clumpiness, keeps magnitude. |
| `GALAXY_ABL_AXISYMMETRIC_FIELD` | Replace the field with its azimuthal average. Removes arms and clumps, keeps the rotation curve. |
| `GALAXY_ABL_NO_STAR_SELF_GRAVITY` | Leave stars out of the field's quadtree, so stars stop scattering off each other. |
| `GALAXY_ABL_NO_ASSOCIATION_BINDING` | Associations still form, release, and stream, but stop pulling on their members. |
| `GALAXY_ABL_NO_BIRTH_DISPERSION` | Newborns get their association's center-of-mass orbit exactly, with no internal velocity. |
| `GALAXY_ABL_BIRTH_ORBIT_RATIO_CAP` | Clamp a newborn's orbital speed to this multiple of local circular speed rather than to an absolute speed. |
| `GALAXY_ABL_STAR_WAVE_COUPLING` | Override `STAR_WAVE_COUPLING`: the share of the analytic spiral and ring density wave that also acts on stars. |
| `GALAXY_ABL_NO_COLLAPSE_RADIATION_RESIST` | Let a dense cell ignite however irradiated it is. Tests whether retained stars suppress the next generation. |
| `GALAXY_ABL_BIRTH_VELOCITY_DISPERSION` | Isotropic random birth velocity as a multiple of local circular speed. Tests giving a spheroid its dispersion on purpose. |
| `GALAXY_ABL_LENGTH_REFERENCE_SIZE` | Scale the sim's absolute length constants by `size / reference`, so every domain size is a scaled copy of the reference. |

## Why the switches live in the kernel

The alternative is editing a constant per run. galaxy-gen#66 did that and had
to retract a conclusion, because a constant was reverted in the probe but not
at the call site and the probe was believed. A switch the kernel itself reads
cannot drift from what ran. Any probe that mirrors a switch has to read the
same switch - `Galaxy::birth_circular_ratio` does, for exactly that reason.

## Reading the results

`vsig` is `v_rot / sigma` over the resolved disk: above ~1.5 is a rotating
disk, below ~0.7 a pressure-supported mush. `vsy` / `vsm` / `vso` are the
same ratio for young, middle-aged, and old stars.

Prefer the cohort numbers when the question is whether something is *heating*
stars. Pooled `vsig` reports post-birth heating and a generational offset in
birth orbits identically, and only the cohorts tell them apart. Both metrics
carry calibration tests against populations whose answer is known by
construction; so do the two field filters, because a miscalibrated ablation
tool is just a faster way to reach a wrong conclusion.

## What it found

Single-factor ablation found nothing: not field staleness, clumpiness,
non-axisymmetric structure, stellar self-gravity, association binding, or
birth dispersion. Each on its own left the disk crossing into spheroid
territory on the same schedule.

Two factors together hold it indefinitely - a birth orbit ratio cap plus an
axisymmetric field, `vsig` 2.2-2.4 at t=2500 against 0.3-0.5 for baseline.
Neither works alone, because there are two independent heat sources and
either is sufficient on its own: an over-fast birth becomes permanent
eccentricity in a torque-free potential, and a correct birth is scattered
apart by a lumpy one. One-at-a-time ablation cannot find that pair, which is
worth remembering the next time a sweep comes back empty.

Two follow-ups narrowed the second source. Box smoothing does not substitute
for axisymmetrization - twenty passes is a Gaussian of about 11% of the disk
radius, which erases cell texture and barely touches an arm, and it saturates
around 0.5. So the heating is the large-scale transient mass distribution,
not grid noise. Meanwhile the analytic density wave does not heat at all,
even at 0.70 coupling. The difference is coherence, not amplitude: stars are
heated by structure that changes under them, not by structure that rotates
with them.

One caution the sweep earned the hard way: **a result at one domain size is a
hypothesis at another.** Several of the sim's length scales are absolute cell
counts rather than fractions of the disk radius, so size 150 is a different
physical setup from size 500 and not a smaller picture of it. Compare like with
like - same size, same tick count - or the age effect and the size effect will
be indistinguishable.

Full numbers on galaxy-gen#70, which compiles #65 and #66.
