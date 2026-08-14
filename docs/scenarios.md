# Scenarios and seeding

A scenario is a `start => end-shape` pair and the physics constants that steer the run toward it. Four ship.

## Scenarios and seeding

A `Scenario` is a hardcoded `start => end-shape` pair - not just an initial condition, because the physics constants that steer its evolution (halo curve, flow relaxation, ejection geometry, and any density wave) belong to it too. Four ship, and the end shape is sturdy across seeds because noise only textures a scenario - the shape drivers are deterministic:

- `BangRing` - central explosion and symmetric ejection settle into an ongoing annular potential near 0.58 disk_r. Conservative pressure and radial transport keep the hollow ring broad and resolved, while annulus-scoped collapse forms stars there.
- `BangSpiral` - explosion with an m=2 lobed ejection tilted 0.6 rad prograde (`eject_swirl`). A persistent rotating logarithmic wave gathers the ejecta into broad star-forming arms between about ticks 500 and 1000 and can preserve later arm windows.
- `IrregularSpiral` - domain-warped smoke noise with a seeded two-arm overdensity. The same ongoing wave and pressure model lets the disturbed disk settle into long-lived arms, typically later than the bang start.
- `IrregularElliptical` - smoke noise under an exponential radial envelope (real ellipticals are light-profile concentrated), weak rotation, high dispersion, sub-circular flow support so it stays compact and smooth.

The bang seeders deliberately fill the core below the default collapse density threshold (`core_fill_scale`): a dense core converts to stars before its ejecta travel anywhere, so the wide thin core only reaches star-forming density where mass piles up near the target radius. The ring further restricts collapse qualification to its annulus. Ejection speed budgets the climb explicitly: self-gravity escape plus the halo potential difference to the target radius (`v_flat^2 * ln((rt^2 + rc^2) / rc^2)`).

The irregular seeder normalizes to a deterministic total gas budget: the fBm draw's mean varies +-35% seed to seed, and thin draws lose their seeded structure to dissipation long before t=1000, so per-seed budgets are what shape sturdiness is made of - noise only textures. The elliptical additionally applies a whisper of in-disk star drag (`star_drag`): stars are collisionless, so without it a young swarm slowly evaporates outward instead of settling into the central glow.

Every scenario seeds inside the disk radius and adds orbital support on top: `v = rotation_boost * sqrt(G * M_enc / r + v_c(r)^2)` tangentially, with M_enc prefix-summed over cells sorted by radius - the combined self-gravity + halo equilibrium speed. `seed_with_mode_seeded` gives byte-identical output for the same `(additional, scenario, seed)` - the `?seed=` URL invariant covers every scenario.
