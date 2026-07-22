# Processes, events, and the causal graph

The simulation core follows one principle: the process registry defines causality, struct-of-arrays store state, processes perform bulk transformations, events represent discrete changes, and rendering derives appearance from the resulting world.

## Process registry (`src/rust/process.rs`)

A static list of descriptors - name, declared reads, declared writes, `requires_fresh` (reads that must be produced earlier in the same tick), cadence, phase offset, and a plain fn pointer into `Galaxy`. `Galaxy::tick` runs due descriptors in declared order, then executes the tick's due events. The registry order IS the causal chain: changing it is a physics change, not a refactor.

Validation is a set of unit tests, not a runtime borrow engine:

* every `requires_fresh` read has an earlier same-tick writer with a matching cadence
* every process declares reads or writes
* the ordering matches a golden list that must be extended deliberately

Cadence lets motion run every tick while fields and lifecycle rules run less often: a process runs when `tick_count % cadence == phase_offset % cadence`.

## Event queue (`src/rust/events.rs`)

Deterministic queued-event model:

* an event emitted during tick N is scheduled for tick N+1 - same-tick recursive execution is structurally impossible
* execution order is stable (tick, seq), where seq is emission order within the emitting tick; ids are globally monotonic
* each event carries kind, source, target, a scalar payload, and a causal parent id, so a supernova-induced star birth is traceable back through `ShockWave` to the `Supernova` that caused it
* a bounded ring of executed events feeds instrumentation counters and renderer transients - a supernova flash is the renderer's reading of a Supernova event, never authoritative state

## RNG service

One u64 master seed (the `?seed=` URL value). Streams are derived statelessly per (process id, tick) via splitmix64 mixing, so streams are independent per process, adding a process never shifts another's draw sequence, and there is no RNG state to serialize across the worker boundary. `seed_with_mode` draws a random master seed and delegates to `seed_with_mode_seeded`, so every run is structurally reproducible.

## Determinism contract

Same seed + same tick count + same dt sequence -> identical state. Guarded by golden-hash tests in `mod tests_golden` (galaxy.rs) that pin the mass field after 100 ticks for every scenario. A deliberate physics change recaptures the goldens and says so in the commit.

## The causal loop

Registry order (also the golden-ordering test): gravity, gravity_field, integrate_gas, integrate_stars, radiation_field, collapse_watch, stellar_aging, gas_dissipation. Motion runs every tick; fields every 4; lifecycle rules every 8-16.

The loop, end to end: gas clumps under gravity -> cells that stay dense and cool accumulate collapse heat -> CloudCollapse consumes gas into a birth budget -> StarBirth spawns 1-5 stars (masses sum exactly to the budget, shared cluster id, prograde orbital velocity from the field) -> stars deposit radiation, which resists further collapse and dissipates gas -> stellar_aging retires light stars to remnants and detonates heavy ones -> Supernova returns ~80% of the star's mass to nearby gas with an outward kick, keeps a dim remnant, and emits ShockWave -> the shock boosts collapse heat around the blast, recording the shock's event id as the cells' heat parent -> induced CloudCollapse events carry that parent, so a supernova-triggered star birth has a complete verifiable ancestry (asserted by the integration test).

Conservation: the baryonic ledger (gas + stars + in-flight birth budgets + the explicit dissipated sink) stays constant to sub-1.0 through formation and supernovae - a per-tick assertion in the ledger test.

## See also

* [galaxy-rust.md](galaxy-rust.md) - constants, buffers, hot path
* [tick-worker.md](tick-worker.md) - worker message protocol
* [FEATURES.md](FEATURES.md) - capability inventory
