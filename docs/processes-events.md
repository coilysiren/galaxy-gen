# Processes, events, and the causal graph

The simulation core follows one principle: the process registry defines causality, struct-of-arrays store state, processes perform bulk transformations, events represent discrete changes, and rendering derives appearance from the resulting world.

## Process registry (`src/rust/process.rs`)

A static list of descriptors - name, declared reads, declared writes, `requires_fresh` (reads that must be produced earlier in the same tick), cadence, phase offset, and a plain fn pointer into `Galaxy`. `Galaxy::tick` runs due descriptors in declared order, then executes the tick's due events. The registry order IS the causal chain: changing it is a physics change, not a refactor.

Validation is a set of unit tests, not a runtime borrow engine:

- every `requires_fresh` read has an earlier same-tick writer with a matching cadence
- every process declares reads or writes
- the ordering matches a golden list that must be extended deliberately

Cadence lets motion run every tick while fields and lifecycle rules run less often: a process runs when `tick_count % cadence == phase_offset % cadence`.

## Event queue (`src/rust/events.rs`)

Deterministic queued-event model:

- an event emitted during tick N is scheduled for tick N+1 - same-tick recursive execution is structurally impossible
- execution order is stable (tick, seq), where seq is emission order within the emitting tick; ids are globally monotonic
- each event carries kind, source, target, two scalar payloads, and a causal parent id, so a `StarBirth` can carry mass plus composition while a supernova-induced birth remains traceable through `ShockWave` to the `Supernova` that caused it
- a bounded ring of executed events feeds instrumentation counters and renderer transients - a supernova flash is the renderer's reading of a Supernova event, never authoritative state

## RNG service

One u64 master seed (the `?seed=` URL value). Streams are derived statelessly per (process id, tick) via splitmix64 mixing, so streams are independent per process, adding a process never shifts another's draw sequence, and there is no RNG state to serialize across the worker boundary. `seed_with_mode` draws a random master seed and delegates to `seed_with_mode_seeded`, so every run is structurally reproducible.

## Determinism contract

Same seed + same tick count + same dt sequence -> identical state. Guarded by golden-hash tests in `mod tests_golden` (galaxy.rs) that pin the mass field after 100 ticks for every scenario. A deliberate physics change recaptures the goldens and says so in the commit.

## The causal loop

Registry order (also the golden-ordering test): gravity, gravity_field, integrate_gas, integrate_stars, radiation_field, collapse_watch, stellar_halo, stellar_aging, bh_accretion, bh_evaporation, gas_dissipation, gas_fountain. Motion runs every tick, fields every 4 ticks, and lifecycle rules every 8-16 ticks.

The loop, end to end: gas clumps under gravity -> cells that stay dense and cool accumulate collapse heat -> CloudCollapse consumes gas into a birth budget -> StarBirth joins or creates a temporary stellar association whose masses sum exactly to the budget -> stars deposit radiation, which resists further collapse and lifts hot gas out of the visible disk -> the galactic fountain cools halo gas back into moving disk filaments while its cold share follows a deterministic 40-60% limit cycle -> stellar_aging retires light stars to temporary remnants and detonates heavy ones -> Supernova returns about 80% of the star's mass to nearby gas, leaves a neutron star, and emits ShockWave -> the shock boosts collapse heat around the blast and preserves causal parentage for any induced CloudCollapse and StarBirth.

`integrate_stars` also owns association binding and release. It rebuilds deterministic mass-weighted aggregates from the stars' persisted `cluster_id` values, applies a softened internal force, subtracts each association's mass-weighted recoil, and clears ids when age, membership, or the local tide dissolves the group. Clearing an id is a lifecycle mutation only. It does not alter the released star's phase-space state.

Core-collapse-scale birth draws split into equal compact-binary partners. Each component retains the original system's lifetime and core-collapse fate. Once both partners have become neutron stars and their seed-derived delay expires, stellar_aging emits `NeutronStarMerger`. Its handler combines position, momentum, and mass into one remnant, moves a small radiated fraction into the sink, then emits the causally linked `GammaRayBurst` on the following tick. The burst produces both a renderer transient and a temporary radiation-field splat.

Intermediate-mass draws also split into stable pairs. Their components become red giants, emit `PlanetaryNebula` while returning inherited composition with their envelopes, and leave white dwarfs. Once both seed-derived delay clocks expire, stellar_aging emits `TypeIaSupernova`. Its handler disrupts both stars, deposits enriched ejecta, accounts for any fractional remainder in the radiated sink, and emits a causally linked `ShockWave`.

`stellar_halo` counts consecutive scans beyond 1.18 disk radii. A star that remains there phase-mixes out of the resolved point population after eight scans. Old quiet remnants, merged remnants, and unpaired neutron stars retire on their own clocks. All of their mass moves into `stellar_halo_mass`, which the renderer reads as diffuse light. A binary that loses one partner to the halo becomes unpaired rather than merging with a missing object.

Conservation: the baryonic ledger (cold gas + hot halo gas + resolved stars + diffuse stellar halo + in-flight birth budgets + black hole + radiated sink) stays constant to sub-1.0 through formation, fountain exchange, supernovae, phase mixing, and compact mergers. A second ledger tracks heavy elements across the same carriers plus explicit stellar yields. Per-tick, transport, state round-trip, fountain-direction, phase-mixing, and merger tests guard both.

## See also

- [galaxy-rust.md](galaxy-rust.md) - constants, buffers, hot path
- [tick-worker.md](tick-worker.md) - worker message protocol
- [FEATURES.md](FEATURES.md) - capability inventory
