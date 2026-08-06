# Stellar co-rotating frame

The renderer follows the representative angular motion of resolved stars.
Nebular gas physically orbits faster, so it now sweeps through a star-forming
region and leaves newborn stars behind. This reads as a cloud dropping stars
instead of firing them into space.

## Presentation only

Rust remains an inertial simulation. Gas positions, stellar positions and
velocities, event coordinates, gravity, conservation ledgers, worker state,
and serialization are unchanged.

The canvas rotates every world-space layer around the galactic center:

- gas, dust, and diffuse halo light
- stars and association glows
- birth, nebula, supernova, and gamma-ray transients

The black hole stays at the center of rotation. Its lens remains a
screen-space post-process. Shock-front shimmer maps event centers through the
same world rotation before applying camera pan and zoom.

## Deterministic phase

Each scenario owns one renderer-only coefficient calibrated from the median
angular velocity of resolved stars in a representative seeded run. The
angular rate scales with the inverse square root of galaxy size, matching the
way world radius and gravitating mass grow together. Presentation applies a
16x multiplier above that calibrated rate so the nebular lead remains legible
at normal playback speed.

Frame phase is derived from scenario, size, and absolute simulation tick. It
is not integrated from wall-clock animation time. Pausing freezes the frame,
stepping advances it once, and a `seed + scenario + size + t` permalink
reconstructs the same visual orientation.

The renderer publishes `data-frame-angle` and `data-frame-rate` on
`#dataviz` for browser verification. Camera state remains separately
observable through the existing `data-cam-*` attributes.
