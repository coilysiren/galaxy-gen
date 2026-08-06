# Ring density waves

The `bang => ring` scenario models a central gas launch that circularizes into a long-lived star-forming annulus. The ring is simulation state, not a render mask. An axisymmetric potential, resolved gas pressure, and conservative radial transport maintain it while stars remain collisionless.

## Annular potential

`ring_density_wave` runs after gravity every tick. For target radius `r0`, gas receives a smooth radial acceleration proportional to:

```text
-tanh((r - r0) / width)
```

The force points outward inside the target and inward outside it. It has no azimuthal component, so it cannot paint arms or a bar. The shipped target is 0.58 disk radii, near the bang ejecta's intended turnaround radius. Inner and outer cutoffs avoid a singular center and a hard force at the disk boundary.

Stars never receive the annular force. A star born in the ring inherits its association orbit and then evolves in the ordinary coarse gravitational field.

## Resolved gas transport

`gas_pressure` applies the local isothermal density-gradient acceleration used by other structured-gas scenarios. After advection, the integrator computes pressure flux from the pre-transfer mass field and moves excess mass toward lower-density neighbors.

Ring gas may also move a bounded share to a cardinal neighbor closer to the annular minimum. Every transfer carries the source metal fraction and linear momentum. Unit tests run both ring and spiral transport through the same conservation assertions.

## Annular star formation

The ring uses scenario-owned collapse density and probability values. Cells outside the target annulus cannot accumulate collapse heat. This prevents the launch core from becoming the primary stellar population and distributes later births through the maintained gas ring. The existing `CloudCollapse => StarBirth` event chain remains authoritative.

## Morphology checks

The native probe reports four pitch-independent ring measurements:

* `ring` is the visible gas mass fraction within 0.12 disk radii of the target.
* `hollow` is the visible gas fraction outside the inner core.
* `rcov` is the fraction of twelve annular sectors containing enough cells and mass.
* `rw` is the mass-weighted radial RMS distance from the target, in disk radii.

Acceptance combines all four with an occupied-cell floor. Seed 42 at size 50 must remain concentrated, hollow, azimuthally covered, narrow, resolved, and actively star-forming throughout ticks 1400 through 1500. Browser verification uses the same deterministic seed and tick links at a larger rendered size.
