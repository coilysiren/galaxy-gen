# Canvas renderer

`src/js/lib/dataviz.tsx`. Canvas, not SVG: one `<canvas>` per frame, DPR-aware
and clamped at 2x.

## Frame composition

A `?seed=`-derived backdrop (starfield, faint nebulosity from the same gas
sprites, galactic-plane band) blits as the screen-fixed opaque base. The world
renders in a deterministic stellar co-rotating frame, so faster gas sweeps
through star-forming regions and leaves newborn stars behind instead of
appearing to fire them outward.

New stars begin dimly beneath both gas passes and ease into the exposed layer
with age. Association glow follows the same reveal.

Pan and zoom is a dev utility behind `?debug=1` (`data-cam-{tx,ty,zoom}`).
Without it the view is locked.

## Layers

Nebular gas sprites at fractional physical positions in four tiers (cold
blue-violet, warm magenta, H-alpha pink, shock-swept [OIII] teal), split into
under- and over-star passes. Then multiply-composited dust lanes, bound
association glows, stellar-class points, cyan compact remnants, a diffuse
phase-mixed stellar halo, and transients for supernova and planetary-nebula
shells and gamma-ray jets. Active quasars add bipolar light cones,
pulse-synchronized packets, and nuclear glare on their gas-feedback axis.

## Hiding the boundary

Two radial fades hide the finite domain, because gas and stars end differently.

Stars keep the wide fade (0.88 to 1.32 of the disk radius), matching their
genuine two-tier soft halo in the physics. Gas fades to zero at 0.94, inside the
confinement radius. Gas is sprung at the disk radius, which makes that radius an
equilibrium every outward-drifting parcel parks on, and under the star fade that
density ridge drew at 0.82 alpha. That was the brightest ring in the frame,
sitting exactly on the domain boundary.

A screen-space vignette then keeps a wide or short viewport from ending the
image on a straight edge.

## Post-processing and cost

A gravitational-lens post-process adds point-mass deflection, Einstein-ring
arclets, an inverted inner image, event-horizon shadow, and photon ring.

Gas composites per screen-space block rather than per cell, which decouples
frame cost, and gas exposure, from grid resolution.

## See also

- [rendering-fades.md](rendering-fades.md) - fades, vignette, and the lens.
- [rendering-gas.md](rendering-gas.md) - gas sprites, screen-space blocks, dust.
- [rendering-stars.md](rendering-stars.md) - star batching, glow, transients.
- [FEATURES.md](FEATURES.md) - the inventory entry this expands.
- [starfield.md](starfield.md) - the seeded backdrop.
- [co-rotating-frame.md](co-rotating-frame.md) - the reference frame.
- [perf-rewrite.md](journal/perf-rewrite.md) - the inner-loop rewrite.
