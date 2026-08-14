# Lifecycle chains and the ledger

The event chains that run after a star is born - binding, binary evolution, retirement - and the conservation guarantees that hold across all of them.

## Associations

`integrate_stars` also owns association binding and release. It rebuilds deterministic mass-weighted aggregates from the stars' persisted `cluster_id` values, applies a softened internal force, subtracts each association's mass-weighted recoil, and clears ids when age, membership, or the local tide dissolves the group. Clearing an id is a lifecycle mutation only. It does not alter the released star's phase-space state.

## Compact binaries

Core-collapse-scale birth draws split into equal compact-binary partners. Each component retains the original system's lifetime and core-collapse fate. Once both partners have become neutron stars and their seed-derived delay expires, stellar_aging emits `NeutronStarMerger`. Its handler combines position, momentum, and mass into one remnant, moves a small radiated fraction into the sink, then emits the causally linked `GammaRayBurst` on the following tick. The burst produces both a renderer transient and a temporary radiation-field splat.

## Intermediate-mass pairs

Intermediate-mass draws also split into stable pairs. Their components become red giants, emit `PlanetaryNebula` while returning inherited composition with their envelopes, and leave white dwarfs. Once both seed-derived delay clocks expire, stellar_aging emits `TypeIaSupernova`. Its handler disrupts both stars, deposits enriched ejecta, accounts for any fractional remainder in the radiated sink, and emits a causally linked `ShockWave`.

## Retirement to the diffuse halo

`stellar_halo` counts consecutive scans beyond 1.18 disk radii. A star that remains there phase-mixes out of the resolved point population after eight scans. Old quiet remnants, merged remnants, and unpaired neutron stars retire on their own clocks. All of their mass moves into `stellar_halo_mass`, which the renderer reads as diffuse light. A binary that loses one partner to the halo becomes unpaired rather than merging with a missing object.

## Conservation

Conservation: the baryonic ledger (cold gas + hot halo gas + resolved stars + diffuse stellar halo + in-flight birth budgets + black hole + radiated sink) stays constant to sub-1.0 through formation, fountain exchange, supernovae, phase mixing, and compact mergers. A second ledger tracks heavy elements across the same carriers plus explicit stellar yields. Per-tick, transport, state round-trip, fountain-direction, phase-mixing, and merger tests guard both.
