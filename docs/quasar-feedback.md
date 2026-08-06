# Quasar feedback

Quasars are brief active-nucleus episodes driven by the black hole's feeding history, not random renderer flashes.

## Ignition and duty cycle

`bh_accretion` runs every eight ticks and updates an exponential moving average of swallowed gas. An inactive hole ignites when all four conditions hold:

- simulation tick is at least 1000
- black-hole mass is at least 1.50 times its seeded mass
- smoothed accretion per scan is at least 0.00025 of seeded black-hole mass
- no episode or cooldown is active

Ignition derives an axis from the seed and episode count, persists it for 360 ticks, and emits `QuasarIgnition`. The envelope eases in over 16 ticks and out over the final 48. Its 56-tick pulses attack over 8 ticks and decay gradually. Completion starts a 500-tick cooldown.

Duty-cycle fields travel in the worker meta snapshot, so permalink loading and worker pauses preserve the exact episode.

## Bipolar feedback

`quasar_feedback` runs after ordinary gas forces and before integration. Force and radiation follow the authoritative pulse envelope. The persisted axis drives:

- add strong outward acceleration inside two opposed, widening cones
- deposit ionizing radiation into a slightly broader pair of cones
- leave gas mass and heavy elements on their existing cells and reservoirs

The ordinary integrator moves accelerated parcels through a broad physical half-angle. Radiation suppresses collapse and lets `gas_dissipation` lift hot material into the halo, where `gas_fountain` may cool it later. An episode may severely reshape the visible host, but the baryon and metal ledgers still close.

The canvas renders the same axis as feathered ionization cones extending beyond the viewport. Nuclear glare follows the Rust pulse. Seed-stable ejecta knots move from both poles, remaining visible after a crest as they cross the host. Physics remains authoritative in Rust.

## Reference acceptance

The browser calibration reference is seed `409007255426557616`, size 250, `irregular-elliptical`:

- tick 2700: black-hole growth about 1.25, below the growth gate
- about tick 3954: black-hole growth reaches 1.50 and the first episode ignites
- tick 3968: the first pulse train is active and the visible gas reservoir is about 60%
- tick 4261: late in the same episode, the host is visibly evacuated along the poles and the visible gas reservoir is about 43%

The size-50 Rust regression proves inactivity at 1000, one active episode at 1100, and inactivity by 1400. Unit coverage pins the trigger gates, cooldown, pulse state, and serialization. A two-pulse counterfactual requires feedback to increase mass-weighted bipolar extent by over eight cells versus an identical control while preserving both ledgers. Playwright samples a full pulse, checks ejecta and screen-edge illumination, then proves inactivity at 1400.

For native inspection, `ward exec debug-sim -- 5000 250 1 409007255426557616 3` reports growth, feeding rate, activity, episodes, and morphology. Native and WASM trajectories can cross discrete thresholds on different ticks because their floating-point execution differs, so browser checkpoints are calibrated in the browser.
