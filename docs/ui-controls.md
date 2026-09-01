# UI controls and URL state

The React shell in `src/js/lib/application.tsx`. Plain `useState`, no store.

## Layout

On desktop the control panel floats over a full-bleed canvas. On mobile the two
share a column, canvas above and a horizontal control bar below, so chrome never
covers the sim.

## Controls

Galaxy size (default 500), scenario dropdown (the four start => end pairs),
generate, play-pause, reset, and record. Seed mass and dt are fixed constants,
both retired as config surfaces. Star color is not a control: the resolved
layer is always keyed by age. See [rendering-stars.md](rendering-stars.md).

Reset rebuilds the current seed at tick zero and drops the spent `t` from the
address. That is distinct from generate, which rolls a fresh universe unless
`lock=1` pins the seed.

## Default view versus debug

The default view carries no instrumentation. The sim tick renders as a caption
under the canvas, the one number `?t=` makes actionable. The full lifecycle
counter table, tick-ms, FPS, single-step, and the camera all sit behind
`?debug=1`.

A chrome toggle hides the panel entirely and mirrors to `?ui=0`, so a clean
frame is a shareable address and a recording can be captured without UI. The
toggle rests near-invisible and wakes on pointer movement, so the hidden state
is never a trap. There is no keyboard surface.

## URL round-trip

`?seed=&size=&scenario=&lock=&t=&ui=&debug=` round-trips through
`history.replaceState`. Generate cycles to a fresh seed each press unless
`lock=1` pins it, while a URL-provided seed is honored for the first generate
either way.

`t=` makes any moment addressable. Pausing or stepping stamps the current sim
tick into the URL, and loading a seed+t link auto-generates and fast-forwards to
that exact tick. Determinism guarantees the identical frame.

Seeds are u64: `crypto.getRandomValues` for a fresh one, `BigInt` for
paste and validate.

## Test contract

A `data-wasm-ready` gate marks readiness. Every E2E-touched element carries a
`data-testid`, which is load-bearing rather than decorative.

## See also

- [FEATURES.md](FEATURES.md) - the inventory entry this expands.
- [rendering.md](rendering.md) - what the canvas draws.
- [recording.md](recording.md) - capturing a run.
