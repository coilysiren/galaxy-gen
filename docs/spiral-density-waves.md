# Spiral density waves

The spiral scenarios model nebulae as active gas structures. Their arms are not a render effect and are not preserved by freezing a seeded mask. A rotating gravitational pattern gathers orbiting gas into broad compression lanes, and the existing collapse loop forms stars from gas that remains dense there.

## Arm potential

For a gas parcel at polar position `(r, theta)`, the two-arm phase is:

```text
phi = 2 theta - pitch ln(r) - pattern_phase
```

`SPIRAL_PITCH` fixes the logarithmic arm shape. `spiral_pattern_step` advances a rigid pattern every tick. The acceleration is proportional to `-sin(phi) grad(phi)`, with smooth inner and outer radial tapers. This points off-arm gas toward the nearest potential minimum while leaving the nuclear region and disk boundary free of a hard force discontinuity.

The process acts on gas acceleration only. Stars remain collisionless and continue to read the ordinary coarse gravity field. Gas therefore crosses the pattern, compresses, forms stars, and moves on. New stars do not remain attached to a painted gas cell.

## Resolved gas pressure

The grid integrator moves one parcel per occupied cell. Without another mechanism, admitted collisions merge parcels and reduce the number of resolved cloud cells permanently. A force can align the surviving knots but cannot make them broad nebulae again.

Wave-bearing scenarios add three gas responses:

- A finite-difference density-gradient acceleration pushes cloud edges toward lower density.
- After advection, a bounded isothermal flux moves excess mass toward lower-density cardinal neighbors.
- Cooling gas moves a smaller bounded share to a neighboring cell with lower arm potential.

The two mass transfers are computed from the pre-transfer mass field. Every transfer carries the source metal fraction and linear momentum. Unit tests pin total gas mass, metal mass, and x/y momentum across the operation. The ordinary cell cap and pressure overflow remain the final guard against gridlock.

Pressure competes with the arm potential. The potential creates coherent high-density lanes, while pressure keeps each lane several cells wide instead of collapsing it into isolated points.

## Star-forming regions

No special star-spawn path belongs to the density wave. `collapse_watch` observes the resulting gas state on its normal cadence. Cells that remain dense, cool, and weakly irradiated emit `CloudCollapse`, then `StarBirth`. The birth consumes gas, preserves composition, and joins or creates a temporary stellar association. Supernovae and the galactic fountain later return gas to the same evolving disk.

This keeps the causal chain physical inside the simulation:

```text
rotating wave -> broad dense gas lane -> cloud collapse -> stellar association
```

## Morphology checks

The native `debug-sim` probe reports two pitch-aware metrics:

- `spi` is the mass-weighted complex amplitude of `2 theta - pitch ln(r)`. A bar or two opposite clumps does not match the configured radial phase.
- `cov` divides the visible disk into eight radial bands and reports the share whose local arm phase aligns with the global pattern. Each covered band must contain enough occupied cells and gas mass.

Visible acceptance uses both metrics plus a minimum occupied-cell count. This prevents a few aligned knots from passing as a galaxy. A fixed-seed Rust test verifies that `bang => spiral` remains coherent, radially covered, resolved, and actively star-forming throughout ticks 1000 through 1100. Long-run probes cover all scenarios and checkpoints through tick 4000. Browser verification uses deterministic `?seed=&size=&scenario=&t=` links to inspect the same simulation state rendered by the application.

The intended timing is scenario-specific. The bang start can establish arms around ticks 500 to 1000. The irregular start is more disturbed and commonly settles into its strongest sustained arm window around ticks 1500 to 3000.
