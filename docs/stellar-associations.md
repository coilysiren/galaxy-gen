# Stellar associations

Stellar associations turn repeated nearby collapse events into coherent, temporary structures. They make dense star-forming regions visually legible without introducing a second authoritative object graph or making clusters immortal.

## Formation

Each `StarBirth` still draws a bounded IMF batch whose masses close the baryonic ledger. The birth now looks for a young main-sequence member within 3.2 cells. A match reuses that member's association id. Otherwise the birth allocates the next dense id.

The new batch starts inside a 0.9-cell circular footprint. Compact-binary partners retain their smaller separation. One center-of-mass orbit is chosen at the collapse site:

- only 12% of radial gas motion survives star formation
- prograde gas motion remains
- an eight-sample azimuthal average of the live stellar field removes the nearest-clump directional bias
- a weighted smooth halo and black-hole floor restores support hidden by the deliberately quarter-strength stellar field
- the final tangential speed is capped below the old maximum birth envelope

Members receive a small internal prograde spin around the combined association center. The batch's mass-weighted mean internal velocity is subtracted exactly, so that spin does not change its center-of-mass momentum.

## Binding and dissolution

`cluster_id` remains the authoritative association state on each star. Every stellar integration tick reduces those ids into mass-weighted centers, velocities, ages, and member counts.

A softened internal acceleration attracts each bound member toward its association center. The process subtracts the association's mass-weighted mean acceleration from every member. Internal binding therefore cannot propel the center of mass or manufacture galactic angular momentum.

Associations are temporary:

- groups with fewer than three members dissolve
- binding fades continuously and ends when the oldest member reaches 620 sim-time
- a 56 sim-time embedded phase protects a newborn group from immediate numerical stripping
- after that grace period, a local field-derived tidal radius releases exterior members

Release only clears `cluster_id`. Position, velocity, mass, age, lifecycle stage, binary identity, and star identity remain unchanged. Ordinary differential gravity then stretches the released stars into tidal streams. The existing stellar-halo process can later phase-mix them out of the resolved population.

## Rendering and state

The full persisted star record remains the existing 14-float layout. Worker render snapshots add `clusterId` as the sixth value in each star record. JS reads the shared `STAR_RENDER_FLOATS` constant instead of duplicating the stride.

The renderer groups still-bound members by id and derives one low-alpha radial glow from their weighted center and RMS spread. Groups with fewer than four visible members or a broad stream-like spread receive no glow. Clearing the physics id therefore removes the glow naturally.

There is no serialized association object. `cluster_id` and `next_cluster_id` already round-trip in the star and meta buffers. The UI reports the cumulative number of distinct associations formed, not the number of `StarBirth` events.

## Validation

Rust tests cover nearby joining, distinct distant formation, suppressed radial inheritance, prograde support, momentum-neutral binding, tidal release, and the six-float render contract. The native `ward exec debug-sim -- <ticks> <size> <seed-count> <start-seed>` probe accepts an exact URL seed for deterministic tuning before browser inspection.
