# Quasar feedback

Quasars are long-lived active-galactic-nucleus state driven by the central black hole's own feeding history. They are not random renderer flashes.

## Ignition and duty cycle

`bh_accretion` runs every eight ticks and updates an exponential moving average of swallowed gas. An inactive hole ignites when all four conditions hold:

- simulation tick is at least 2400
- black-hole mass is at least 1.20 times its seeded mass
- smoothed accretion per scan is at least 0.00025 of seeded black-hole mass
- no earlier episode or cooldown is active

Ignition derives an axis from the master seed and episode count, persists that axis with a 3000-tick remaining duration, and emits one `QuasarIgnition` event for instrumentation. Brightness eases in over 80 ticks and out over the final 240. A completed episode starts a 1200-tick cooldown before another sustained feeding interval can ignite.

All duty-cycle fields travel in the worker meta snapshot. Loading, pausing, or resuming a permalink therefore preserves the episode exactly, including its next tick.

## Bipolar feedback

`quasar_feedback` runs every tick after gravity, scenario waves, and gas pressure, but before gas integration. It uses the persisted axis to:

- add outward acceleration inside two opposed, widening cones
- deposit ionizing radiation into a slightly broader pair of cones
- leave gas mass and heavy elements on their existing cells and reservoirs

The ordinary gas integrator moves accelerated parcels. The radiation field then suppresses collapse and lets `gas_dissipation` lift hot material into the serialized halo reservoir. The existing `gas_fountain` may cool it back later. A strong episode may severely deplete the visible host, but the baryon and metal ledgers still close.

The canvas renders the same axis as broad ionization cones, narrow bright jets, terminal lobes, and nuclear glare. This is derived presentation state. Physics remains authoritative in Rust.

## Reference acceptance

The calibration reference is seed `409007255426557616`, size 250, `irregular-elliptical`:

- tick 2400: black-hole growth about 1.23, no active quasar
- tick 2500: first episode active at full brightness
- tick 2700: the opposed feedback has substantially reduced occupied cold-gas cells
- tick 5262: the original supplied checkpoint remains inside the first episode's fade-out

The fixed Rust regression runs the same seed and scenario at size 50 for speed, proving no episode at 2400 and exactly one active episode at 2500. A controlled-axis unit test proves opposed gas acceleration, broader axis-aligned heating, and unchanged baryon and metal totals. State round-trip coverage pins the duty-cycle fields and exact next-tick radiation. Playwright loads the tick-2500 permalink and checks worker-to-UI activity, axis metadata, the conditional quasar statistic, and nonblank canvas output.

For native inspection, `ward exec debug-sim -- 2700 250 1 409007255426557616 3` selects only the irregular-to-elliptical scenario and reports black-hole growth, normalized feeding rate, activity, episode count, and ignition events at the quasar checkpoints.
