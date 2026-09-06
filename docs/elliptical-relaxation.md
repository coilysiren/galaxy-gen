# Elliptical relaxation

`IrregularElliptical` starts from an asymmetric gas cloud and relaxes into a smooth, pressure-supported stellar spheroid. It does not use the logarithmic spiral force, annular force, or either structured-gas transport path.

## Assembly model

The seeded gas follows an irregular exponential radial envelope. Its sub-circular flow and high velocity dispersion produce an assembling cloud instead of a cold disk. The static halo and ordinary gravity draw that material inward.

Resolved isothermal pressure (`gas_pressure = 0.35`) acts on gas before integration and as conservative neighbor flux afterward. It prevents the cloud from collapsing into one or a few capped cells while preserving gas mass, metals, and momentum. Elliptical collapse tuning uses a `0.4` density fraction and a `0.2` scan probability. That combination forms stellar associations throughout the reservoir instead of consuming only a central knot.

New stars inherit local gas motion and then evolve collisionlessly. Association binding is temporary. Phase mixing and weak scenario drag reduce coherent streams while the stellar body settles in the shared coarse gravity field. No morphology force is applied to stars.

## Measurements

The native simulation exposes five stellar morphology measurements inside the resolved disk:

- **Concentration** is the stellar mass fraction inside `0.35` disk radii. A lower and upper bound reject both diffuse debris and a point mass.
- **Smoothness** is one minus the strongest normalized angular Fourier amplitude for modes 1 through 4. Clumps, a bar, and spiral arms lower it.
- **Axis ratio** is the projected minor-to-major ratio from the centroided stellar inertia tensor.
- **Extent** is the mass-weighted stellar RMS radius divided by disk radius. It rejects one-cell collapse.
- **Rotational support** is absolute mean tangential speed divided by RMS stellar speed. A pressure-supported body remains below a cold disk.

A synthetic unit test compares a circular resolved body with a bar and a central point mass so these measurements cannot pass all three shapes interchangeably.

## Acceptance window

The fixed size-50, seed-42 run is checked at every tick from 900 through 1000:

- concentration stays from `0.45` through `0.85`
- smoothness stays at least `0.70`
- axis ratio stays at least `0.65`
- extent stays from `0.30` through `0.65`
- rotational support stays at most `0.60`
- at least 150 gas cells and 500 resolved stars remain
- core star density stays at least `0.55`, which is the only bar that reads amount rather than
  shape: the five above all divide by total stellar mass, so a cut that retires most of the
  population moves none of them. galaxy-gen#7051
- spiral coherence stays at most `0.35` and ring concentration at most `0.25`
- star births and phase-mixed stars both increase during the window

`just debug-sim 5000 50 3 42` reports these values as `econ`, `esm`, `axis`, `ext`, and `erot`. Its checkpoints extend through tick 5000 for longer-run inspection. The deterministic golden hash separately pins the scenario's mass field at tick 100.
