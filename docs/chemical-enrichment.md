# Chemical enrichment

Galaxy Gen tracks heavy-element mass alongside every baryonic carrier. The
quantity is composition, not additional mass. Stellar nucleosynthesis converts
part of an existing star into heavy elements, so the baryonic ledger remains
closed while the composition ledger grows by an explicit yield.

## Carriers

- Each cold-gas cell stores heavy-element mass beside its integer gas mass.
- Each resolved star stores the composition inherited from its birth cloud.
- Hot halo gas, diffuse stellar-halo mass, the black hole, and the radiated
  sink each have a serialized heavy-element reservoir.
- A pending StarBirth event carries both its gas budget and its heavy-element
  budget across the one-tick event boundary.

Every transfer keeps the source metallicity, including cell movement and
overflow, collapse, radiation-driven dissipation, fountain lift and cooling,
stellar phase mixing, black-hole accretion and capture, compact mergers, and
Hawking radiation.

## Production

Core-collapse supernovae return inherited heavy elements with their gas ejecta
and synthesize a bounded yield equal to 2% of progenitor mass. Type Ia
supernovae convert up to 35% of their white-dwarf binary mass into heavy
elements. The new amount is recorded in metal_produced_total. Tests enforce:

```text
tracked heavy elements = seeded heavy elements + produced heavy elements
0 <= carrier heavy-element mass <= carrier mass
```

The composition arrays and scalar reservoirs are included in worker
snapshot/restore state. Same seed, tick count, and time-step sequence therefore
produce the same enrichment history.

## Rendering

The worker sends per-cell metallicity in render snapshots. Dense, cold gas only
forms visible dust lanes when it contains enough heavy elements, and dust
opacity scales with enrichment. Supernova-swept [OIII] emission also scales with
the local abundance instead of appearing at a fixed intensity.
