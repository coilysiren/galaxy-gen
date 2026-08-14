# Stars, associations, and transients

Stars are bright glowing points over the gas layer. Color runs cool - light stars, warm cream - to hot, heavy stars, blue-white; size and halo derive from luminosity. Render-only exaggeration is fine here, because none of it flows back into the sim.

## Color is quantized, and so is alpha

The stellar-classification sequence runs M to O, keyed by the sim's log-mass `class_index` (0 is a red dwarf, 1 a blue giant). Real perceived star colors are subtle: warm orange through cream and white to blue-white.

Color is quantized into buckets whose CSS strings are built once. A galaxy in progress resolves tens of thousands of stars, and composing an `rgba(...)` string per star per layer - then making the canvas re-parse it - cost more than the disc it painted. Opacity moves to `globalAlpha`, which is a plain number, so each star still composites separately and a dense swarm still accumulates into a glow.

## Batching, and why alpha is quantized on a curve

Each `arc` plus `fill` is its own composited draw, and a mature galaxy resolves tens of thousands of stars across two layers. That draw-call count, not the arithmetic, is what the star pass costs. Discs are queued into (color, alpha) buckets and each bucket is emitted as one path with one fill, turning roughly 10k draws into a few hundred.

Alpha is quantized on a square-root curve rather than a linear one. The faint glow layers sit near 0.01 and the cores near 0.9, and a linear ladder would collapse every glow onto the same rung.

## Three brightness tiers

Like a long-exposure field. Most stars are bare points of their class color; the bright minority get a tight glow; only the rare giants at the top of the luminosity range earn diffraction spikes. Spikes stay per-star strokes rather than joining the batch, because only the heaviest main-sequence stars clear the cut.

Light is additive: overlapping stars brighten instead of occluding, so a dense swarm - a cluster core, an elliptical spheroid - reads as a glow rather than a sprinkle of isolated dots.

Newborn main-sequence stars begin beneath the entire cloud field and age cross-fades them into the exposed layer, so a birth reveals as its natal gas moves ahead of it rather than as an instantaneous pellet spray.

## Association glow

Bound associations get one shared, restrained pool of unresolved light. The glow is derived entirely from member positions and disappears as the physics strips cluster ids, so a dissolving association naturally turns into discrete tidal-stream stars instead of dragging a fake blob behind it.

## Transients

- **Supernova.** A Sedov-Taylor-flavored blast front: radius grows as `E^0.2 t^0.4` with the progenitor mass standing in for energy, so a 120-mass giant's remnant dwarfs a 30-mass star's and the shock visibly decelerates. It draws as a shell with a bright leading edge and a fading wake - a wave, not a stroked circle - and stays understated, because a big epoch fires many at once.
- **Shimmer.** Refractive distortion at each young blast front: an annulus-clipped self-blit of the canvas, scaled slightly outward about the blast center. Pure GPU compositing with no pixel read-back, so it stays cheap however busy the supernova epoch gets, and it is capped anyway.
- **Gamma-ray burst.** Opposed relativistic jets whose orientation is a stable position hash, because the compact binary has no resolved spin axis in the simulation state.
- **Quasar.** A brief active nucleus reads the same pulse and axis as the physical feedback. Feathered cones light the whole viewport while each pulse carries soft knots of ejected material away from the nucleus.

## See also

- [rendering.md](rendering.md) - the frame's layer order.
- [stellar-associations.md](stellar-associations.md) - the physics behind the glow.
