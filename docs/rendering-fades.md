# Fades, vignette, and the lens

Three separate falloffs, each hiding something different. Two are world-space and hide the simulation's circular boundary; the third is screen-space and hides the viewport's straight one.

## Why stars and gas fade at different radii

Stars keep the wide fade, 0.88 to 1.32 of the disk radius. They have a genuine two-tier soft halo in the physics - soft radius, divergent gradient, halo drag - reaching three times the disk radius, so there is real structure out there worth drawing.

Gas fades much earlier, to zero at 0.94, and for a different reason. Gas is confined by a spring at the disk radius, which makes that radius an equilibrium: any parcel drifting outward parks on it. That is a real density ridge in the simulation, not a rendering artifact. Under the star fade it drew at 0.82 alpha, so the brightest ring in the frame sat exactly on the domain boundary while the fade spent its range on the near-empty sky outside.

Seeded density already feathers from 0.55 of the disk radius (`EDGE_FEATHER_START`), so the two tapers overlap rather than fight. The physics-side half of the fix - spreading the ridge across a band inside the wall - is tracked on galaxy-gen#65 and is not landed; this side makes sure the band is already dark, so no amount of pile-up can paint an edge.

## Vignette

Screen-space, and a separate concern from the world-space fades: it keeps a wide or short viewport from ending the image on a straight edge. It runs after the lens, so the lens cannot warp bright material back into an edge that was already faded. Deliberately gentle - this is a frame, not a mood filter, and the sky it fades to is the page background.

## Gravitational lens

Screen-space point-mass deflection, `r_src = r - thetaE^2 / r`. Sources appear pushed outward, the region inside the Einstein radius shows the inverted image (negative `r_src` flips through the center), and the warp tapers back to identity at the edge of the lens region so there is no seam. Lens depth follows the hole's live mass, so it deepens as the hole feeds and vanishes if Hawking evaporation finishes it.

The deflection is purely radial, so the warp is a stack of concentric annuli each uniformly scaled about the hole, drawn as clipped self-blits. That keeps the whole effect on the GPU. The per-pixel version had to read the framebuffer back every frame, and that stall alone was most of the frame at any grid size.

Two details that are not obvious from the math:

- A destination radius `r` shows the source at `r*f`, so each blit is scaled by `1/f`. A negative `f` is the inverted image inside the Einstein radius: a negative scale mirrors through the center, which is exactly that inversion.
- The region is cleared before the rings are drawn, because compositing its own pixels source-over onto the originals double-blends every semi-transparent pixel into a visible square. The untouched snapshot is then laid back down, because the rings only cover the disc of radius R and without it the square's corners stay cleared and the lens reads as a dark box.

Roughly one ring per 2.5 device pixels keeps the radial stepping below what the eye resolves. The bounds stop a tiny lens from paying for rings it cannot show, and a huge one from issuing hundreds.

## See also

- [rendering.md](rendering.md) - the frame's layer order.
- [rendering-gas.md](rendering-gas.md) - sprites, blocks, dust.
- [rendering-stars.md](rendering-stars.md) - star batching and transients.
