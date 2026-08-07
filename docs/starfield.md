# Seeded backdrop

The deep-space sky the galaxy hangs in. Generated from the same
`?seed=` as the physics, so a permalink reproduces the sky along with
the galaxy, and built from the renderer's own sprites so it reads as the
same universe rather than a stock texture.

Lives in `src/js/lib/starfield.ts`. The renderer owns the sprite assets
and passes them in, so the generator holds no visual vocabulary of its own.

## What it draws

- **A distant starfield.** Positions and brightness are seeded. Class
  colors come from the renderer's own `starColors` table, pushed toward
  the cool, faint end - the giant / dwarf / compact entries at the top
  of the table are reached only rarely. Brightness follows a square-law
  curve, because a field of uniformly-lit points looks like noise
  instead of a sky. The brightest few get a soft halo and a faint
  diffraction cross.
- **Faint nebulosity.** Clouds of the renderer's gas sprites, stamped
  additively with a gaussian spread.
- **A little dust**, multiplied so it absorbs rather than adds. On bare
  space-black it does nothing, which is correct - it only bites where a
  cloud has already brightened the pixels underneath.
- **A seeded band.** Stars and haze concentrate along a great circle at
  a seeded angle, the distant-galactic-plane cue a conventional space
  backdrop leans on.

## Three constraints worth knowing

**It is scenery.** Everything is pulled well down in alpha, with a
single `FADE` constant as the master dimmer. If the sky ever starts
pulling attention off the galaxy, that is the knob.

**No teal.** The backdrop draws from the cold blue-violet, warm magenta,
and H-alpha pink gas tiers, never the shock-swept [OIII] teal one. Teal
is a shock diagnostic in the simulation; as ambient haze it reads as
green fog sitting behind the galaxy.

**The middle is reserved.** Haze fades out inside `CENTER_KEEPOUT` of
the short edge so the disk keeps a clean dark field behind it rather
than sitting on a colored wash. The exclusion is computed in pixel
space, not normalized space, so it stays circular on a wide viewport and
actually matches the disk it protects.

Note that the gas sprites are authored faint on purpose - the galaxy
stacks dozens of them per pixel. The backdrop spreads them thinly
instead, so its alpha multiplier has to be much higher than the sprites'
own to render as anything at all.

## Cost

Built once into an offscreen canvas per (seed, viewport) and blitted as
the opaque base of every frame. That blit replaces the `fillRect` the
renderer already did, so the per-frame cost is a wash - the `background`
pass reports 0.00ms at size 500. Generation stamps hundreds of sprites
and is far too expensive to repeat per frame, and has no reason to,
since nothing in it moves.

Baking the space-black base into the backdrop is also load-bearing for
correctness: the galaxy's multiply-composited dust needs opaque pixels
underneath or it stamps visible grey squares.

The backdrop is laid down before the camera transform and the
co-rotating frame rotation, which is what keeps it fixed to the screen.
Distant sky does not share the disk's rotation.

## Tests

`e2e/starfield.spec.ts` samples the four frame corners - the only
regions that are sky and nothing else, since the disk fits the short
edge and the halo reaches past it. It counts lit pixels rather than
averaging luminance, because the sky is faint enough that its
contribution to a mean is swamped by the base color. Coverage: a sky is
drawn, it stays dark, the same seed reproduces it, different seeds
differ, it does not change across ticks, and generation is cached.

## See also

[FEATURES.md](FEATURES.md), [recording.md](recording.md).
