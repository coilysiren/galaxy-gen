# Ablation switches

The environment variables [ablation.md](ablation.md) drives. Most default to
off. Two override a shipped constant instead, so a landed default can still be
re-measured without editing it.

| Environment variable | Effect |
|---|---|
| `GALAXY_ABL_FIELD_CADENCE` | Rebuild the coarse star field every N ticks instead of 4. `1` removes field staleness. |
| `GALAXY_ABL_FIELD_SMOOTH` | N 3x3 box-blur passes over the field. Removes cell-scale clumpiness, keeps magnitude. |
| `GALAXY_ABL_AXISYMMETRIC_FIELD` | Replace the field with its azimuthal average. Removes arms and clumps, keeps the rotation curve. |
| `GALAXY_ABL_NO_STAR_SELF_GRAVITY` | Leave stars out of the field's quadtree, so stars stop scattering off each other. |
| `GALAXY_ABL_NO_ASSOCIATION_BINDING` | Associations still form, release, and stream, but stop pulling on their members. |
| `GALAXY_ABL_NO_BIRTH_DISPERSION` | Newborns get their association's center-of-mass orbit exactly, with no internal velocity. |
| `GALAXY_ABL_BIRTH_ORBIT_RATIO_CAP` | Clamp a newborn's orbital speed to this multiple of local circular speed rather than to an absolute speed. |
| `GALAXY_ABL_STAR_WAVE_COUPLING` | Override `STAR_WAVE_COUPLING`: the share of the analytic spiral and ring density wave that also acts on stars. |
| `GALAXY_ABL_NO_COLLAPSE_RADIATION_RESIST` | Let a dense cell ignite however irradiated it is. Tests whether retained stars suppress the next generation. |
| `GALAXY_ABL_BIRTH_VELOCITY_DISPERSION` | Isotropic random birth velocity as a multiple of local circular speed. Tests giving a spheroid its dispersion on purpose. |
| `GALAXY_ABL_LENGTH_REFERENCE_SIZE` | Scale the sim's absolute length constants by `size / reference`, so every domain size is a scaled copy of the reference. |
| `GALAXY_ABL_RESOLVED_LUMINOSITY_FLOOR` | Override the per-scenario luminosity floor that bounds the resolved population. `0` disables retirement, the control the #72 numbers were taken against. |

## See also

- [ablation.md](ablation.md) - how to run the matrix and what it found.
