# The boundary ridge

Gas is sprung inward past the disk radius (`CONFINE_STIFFNESS`), and stars get a two-tier halo instead. Both exist to hide a finite domain, and one of them creates a problem it cannot simply delete.

## The ridge is real, and load bearing

The spring makes the disk radius an **equilibrium**: any parcel drifting outward parks on it. So the densest gas ring in the sim sits exactly on the domain boundary. That is a genuine density feature of the physics, not a rendering artifact - and the renderer used to draw it at 0.82 alpha, which made it the brightest ring in the frame and the visible edge of the world.

**Spreading the ridge was tried and reverted.** Ramping the confinement across a band inside the disk radius, the way stars get with `HARD_CLIP_FACTOR`, perturbs the outer disk at a 0.18 band enough to:

- drop spiral coherence to 0.22
- stop ring star formation
- break elliptical relaxation
- change the golden mass field for every scenario

So the ridge is load bearing for scenario dynamics. Removing it means retuning the scenario force models with it gone and regenerating goldens - real work, not a constant tweak. Not *drawing* it was the cheap fix, and that is what shipped: gas fades to zero at 0.94 of the disk radius, inside the wall, so no amount of pile-up can paint an edge.

Tracked on galaxy-gen#65.

## Stars get a different treatment

Between the soft radius (the disk edge) and the hard clip at 3x it lies a halo band with a repulsive gradient `a = K (r - soft)/(hard - r)` - gentle at the soft edge, divergent at the hard edge, so no finite speed ever reaches the clip. `STAR_HALO_DRAG` bleeds energy inside the band only, since the halo spring is conservative and ejecta would otherwise oscillate forever instead of rejoining the disk.

A star that stays beyond 1.18 disk radii for eight lifecycle scans phase-mixes into the unresolved halo before it can advertise the numerical backstop.

This replaced an older rim hard-stop that parked all ejecta in a ring at `disk_r + 3` - the same class of artifact as the gas ridge, solved properly.

## See also

- [rendering-fades.md](rendering-fades.md) - the renderer half of this.
- [sim-constants.md](sim-constants.md) - the constants named here.
