# The stellar population

What a star is made of, how bright it is, and when it stops being drawn.

## The IMF and what follows from it

Births sample a Salpeter-flavored IMF, `dN/dm` proportional to `m^-2.35`, between the mass bounds: many faint red dwarfs, rare blue giants. Luminosity follows a main-sequence-ish power law - roughly `m^2` - and lifetime falls steeply with mass (`900 * (30/m)^2` in sim time), so M-dwarfs outlive the session while O-stars die in minutes.

`class_index` is log-mass normalized to 0..1, M through O. It is sim state only - the renderer keys resolved-star color on `age` instead, so nothing in the frame currently reads it. See [rendering-stars.md](rendering-stars.md).

## The resolved-luminosity floor

An unbound main-sequence star below `RESOLVED_LUMINOSITY_FLOOR` stops being drawn as a point and retires into the diffuse stellar-halo reservoir, through the same path aged remnants already use. Association members are exempt whatever their brightness, so a cluster reads as a cluster while it is one.

**This is the population's only real sink.** Mass recycles through supernovae and the fountain, and only about 4% of stars born reach end of life inside a run - half of all births live 64800 ticks - so without a floor the resolved count grows without bound. Measured on galaxy-gen#72: 23-30k with the floor against 123-131k without.

Luminosity is mass squared, so a floor of 100 is ten solar masses. Under the IMF that is about 81% of stars carrying about 12% of the light, and the standing population skews further that way than the birth numbers do, because the massive stars are the ones that die.

Deleting that light outright costs 1.5% of frame brightness, which is why the floor needs no diffuse-light machinery to replace it. Three renderer treatments were measured; `drop` beat a super-particle scheme and a smooth-glow layer both.

## DRAGON: the elliptical uses zero

Its defining feature is a concentrated stellar spheroid, and **that spheroid is made of the accumulated faint old population**. Applying the disk floor to it drops `spheroid_concentration` from 0.45-0.85 to 0.28-0.32, which is the object losing its identity rather than shedding invisible light.

It needs its retired light actually rendered before it can have a floor. Tracked on galaxy-gen#72, not bodged here - and it means the elliptical's population is currently unbounded.

## See also

- [stellar-evolution.md](stellar-evolution.md) - the death channels.
- [stellar-heating.md](stellar-heating.md) - birth orbits and rotational support.
- [lifecycle-chains.md](lifecycle-chains.md) - binaries, remnants, and the ledger.
