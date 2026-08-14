# Gas forces and reservoirs

The forces acting on gas beyond plain gravity, and where gas goes when it leaves the visible disk. Each section summarizes a model that has its own document.

## Galactic fountain

Radiation-dissipated gas is not destroyed. It moves into `halo_gas_mass`, a hot circumgalactic reservoir serialized with the rest of the simulation. `gas_fountain` runs every eight ticks and drives the cold share of active gas around a 480-tick 40-60% limit cycle. Feedback lifts irradiated cells first. Cooling returns small parcels to existing moving disk filaments before sparsely seeding empty annular cells, with circular velocity and a slight inward drift. The exchange changes visibility and density without creating baryons, and the UI reports `cold / (cold + halo)`.

## Ring density wave

`BangRing` carries an axisymmetric annular potential with its minimum at 0.58 disk radii. `ring_density_wave` adds radial gas acceleration toward that minimum after gravity, while `gas_pressure` prevents the collected ejecta from collapsing into a few point knots. Post-advection transport moves a bounded gas share toward neighboring cells closer to the annulus and carries source metals and momentum exactly. No azimuthal term exists, and stars do not receive the ring force.

Ring star formation uses scenario-owned collapse tuning. Only cells inside the annulus accumulate collapse heat, and a lower per-scan probability distributes births through the sustained structure instead of consuming the launch core in one burst. Pitch-independent metrics measure annular mass concentration, hollow-core depletion, azimuthal coverage, and radial width. A fixed seed must hold all four plus a resolved-cell floor throughout ticks 1400 through 1500 while continuing to emit births. Full details: [ring-density-waves.md](ring-density-waves.md).

## Spiral density waves

The two spiral scenarios carry an ongoing two-arm logarithmic potential, not only a spiral-shaped seed. `spiral_density_wave` runs after gravity every tick and adds acceleration normal to the current arm phase. A rigid pattern phase advances independently of individual gas orbits, so rotating gas passes through compression lanes instead of carrying a painted arm with it. Stars do not read this force.

The cell integrator would otherwise merge colliding parcels irreversibly. Structured-gas scenarios therefore resolve isothermal pressure in two conservative forms. A density-gradient acceleration pushes cloud edges outward, then a post-advection mass flux spreads excess density toward lower-density neighbors. Spiral cooling also moves a bounded share down the local arm potential. Both transfers carry the source metal fraction and momentum, and neither creates gas. The potential gathers diffuse gas into broad lanes while pressure prevents those lanes from becoming point knots. Existing collapse scans turn sustained dense, cool lane cells into stellar associations. Full model and verification details: [spiral-density-waves.md](spiral-density-waves.md).
