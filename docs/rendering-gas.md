# Gas sprites, blocks, and dust

Gas is drawn as soft pre-rendered sprites rather than per-cell gradients. `drawImage` of a gradient sprite is far cheaper, and alpha accumulation makes dense regions glow without any explicit density term.

## Color follows temperature, and brightness comes from accumulation

Gas hue follows the radiation field, not just brightness: cold clouds sit blue-violet, warm gas shifts magenta, and strongly irradiated regions glow H-alpha pink like real emission nebulae around young clusters.

The ramps stay deliberately flat and mid-dark. Brightness is meant to come from *accumulation* - screen blending of overlapping clouds - so a bright ramp double-counts density and clips the cores to white.

## Screen-space blocks decouple exposure from grid size

A gas sprite's on-screen footprint never goes below `GAS_MIN_FOOTPRINT_PX`, so past a certain grid density the renderer is stacking many sprites into the space of one. Cells are therefore aggregated into square blocks sized to hold sprite spacing near that target, independent of grid size: the sim gets finer, the sprite field does not. At size 250 on a typical viewport this resolves to one block per cell - exactly the pre-aggregation renderer - and at 500 it resolves to 2.

The side effect is worth stating on its own: **the gas field now looks the same at any grid size, and it did not before.** Because brightness comes from overlapping sprites accumulating, doubling the grid doubled the sprite count over the same screen area and quietly brightened the whole galaxy. Grid resolution was acting as an exposure control. Blocks decouple the two, so a bigger sim now means more detail at the same exposure.

Every block array is indexed by block and reallocated only when the block grid changes, so a steady-state frame allocates nothing. The values are precomputed once and consumed by the background pass, the foreground pass, and the dust pass alike - those three used to redo the same per-cell math independently. One walk over the cells and two over the blocks replaced three independent full-grid walks.

Shock ionization is stamped per supernova front rather than tested per block: a front only touches the blocks in its own annulus, so it costs the shells' area rather than blocks times waves.

## Two gas passes, split by hash

A stable per-block hash sends most blocks beneath the stars and a foreground share over them, so clusters sit *inside* their clouds instead of on top. Everything either pass needs is already precomputed, so both loops are pure compositing.

## Dust

Dust draws with multiply compositing, so its gradient runs from a dark absorbing core - multiplying toward brown-black - out to white, which is multiply identity and therefore leaves no edge seam.

It is only stamped for **coherent** dust: dense, cold, and embedded in a thick neighborhood. Isolated dense blocks stamping dark specks over the gas was the failure mode the neighbor count exists to prevent.

Dust over the star field is a separate, broader, fainter pass whose job is dimming stars that shine through thick clouds. The visible dark veining comes from the gas pass emitting less there, not from this pass. Dust sits under the foreground gas, so the glow layer re-softens it and the lanes read as embedded darkness rather than holes punched in the clouds.

## The opaque base

Without an opaque base, multiply-composited dust degenerates to plain painting wherever the canvas is transparent and stamps visible grey squares. The seeded backdrop bakes that space-black base in, so blitting it replaces the fill rather than adding a pass. It is laid down before the camera and frame rotation, which is what keeps the sky fixed to the screen instead of spinning with the disk.

## See also

- [rendering.md](rendering.md) - the frame's layer order.
- [starfield.md](starfield.md) - the seeded backdrop.
