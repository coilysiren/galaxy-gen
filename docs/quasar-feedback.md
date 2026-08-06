# Quasar feedback

Quasars are brief active-galactic-nucleus episodes driven by the central black hole's own feeding history. They are not random renderer flashes.

## Ignition and duty cycle

`bh_accretion` runs every eight ticks and updates an exponential moving average of swallowed gas. An inactive hole ignites when all four conditions hold:

- simulation tick is at least 2400
- black-hole mass is at least 1.20 times its seeded mass
- smoothed accretion per scan is at least 0.00025 of seeded black-hole mass
- no episode or cooldown is active

Ignition derives an axis from the master seed and episode count, persists that axis for a 360-tick episode, and emits one `QuasarIgnition` event for instrumentation. The episode envelope eases in over 16 ticks and out over the final 48. Within it, a 56-tick pulse train attacks over 8 ticks and decays gradually. A completed episode starts a 1200-tick cooldown before another sustained feeding interval can ignite.

All duty-cycle fields travel in the worker meta snapshot. Loading, pausing, or resuming a permalink therefore preserves the episode exactly, including its next tick.

## Bipolar feedback

`quasar_feedback` runs every tick after gravity, scenario waves, and gas pressure, but before gas integration. Its force and radiation strength follow the authoritative pulse envelope, concentrating each ejection around a visible pulse. It uses the persisted axis to:

- add outward acceleration inside two opposed, widening cones
- deposit ionizing radiation into a slightly broader pair of cones
- leave gas mass and heavy elements on their existing cells and reservoirs

The ordinary gas integrator moves accelerated parcels. The radiation field then suppresses collapse and lets `gas_dissipation` lift hot material into the serialized halo reservoir. The existing `gas_fountain` may cool it back later. A strong episode may severely deplete the visible host, but the baryon and metal ledgers still close.

The canvas renders the same axis as feathered, overlapping ionization cones that extend beyond the current viewport, so their light reaches every screen edge without a ruler-straight boundary. Nuclear glare expands and contracts with the Rust pulse. Seed-stable knots of ejected material move outward from both poles on the same pulse phase, remaining visible after a crest as they cross the host and leave the screen. This is derived presentation state. Physics remains authoritative in Rust.

## Reference acceptance

The calibration reference is seed `409007255426557616`, size 250, `irregular-elliptical`:

- tick 2400: black-hole growth about 1.23, no active quasar
- tick 2500: first episode active, with its pulse train driving feedback and ejecta
- tick 2700: the opposed feedback has substantially reduced occupied cold-gas cells
- tick 2900: the first episode has ended
- tick 5262: the original supplied checkpoint is well beyond the first episode

The fixed Rust regression runs the same seed and scenario at size 50 for speed, proving no episode at 2400, exactly one active episode at 2500, and an inactive state by 2900. A controlled-axis unit test proves that feedback falls quiet between pulses, pulse crests accelerate opposed gas and heat the broader axis, and the baryon and metal totals remain unchanged. State round-trip coverage pins the duty-cycle fields and exact pulse strength. Playwright advances the tick-2400 reference through ignition, samples a complete pulse cycle, verifies visible bipolar ejecta and screen-edge illumination, then proves the episode is absent at tick 2900.

For native inspection, `ward exec debug-sim -- 2700 250 1 409007255426557616 3` selects only the irregular-to-elliptical scenario and reports black-hole growth, normalized feeding rate, activity, episode count, and ignition events at the quasar checkpoints.
