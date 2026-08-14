# Ablation switches

The environment variables [ablation.md](ablation.md) drives. Some default to
off; the rest override a shipped default, so anything #70 or #72 landed can be
re-measured against its own control without an edit.

| Environment variable | Effect |
|---|---|
| `GALAXY_ABL_FIELD_CADENCE` | Rebuild the coarse star field every N ticks instead of 4. `1` removes field staleness. |
| `GALAXY_ABL_FIELD_SMOOTH` | N 3x3 box-blur passes over the field. Removes cell-scale clumpiness, keeps magnitude. |
| `GALAXY_ABL_AXISYMMETRIC_FIELD` | Override the shipped axisymmetric star field. `0` restores the raw clumpy field - the pre-#70 control. |
| `GALAXY_ABL_NO_STAR_SELF_GRAVITY` | Leave stars out of the field's quadtree, so stars stop scattering off each other. |
| `GALAXY_ABL_NO_ASSOCIATION_BINDING` | Associations still form, release, and stream, but stop pulling on their members. |
| `GALAXY_ABL_NO_BIRTH_DISPERSION` | Newborns get their association's center-of-mass orbit exactly, with no internal velocity. |
| `GALAXY_ABL_BIRTH_ORBIT_RATIO_CAP` | Override the per-scenario newborn orbital ratio cap. `0` leaves only the absolute cap - the pre-#70 control. |
| `GALAXY_ABL_STAR_WAVE_COUPLING` | Override `STAR_WAVE_COUPLING`: the share of the analytic spiral and ring density wave that also acts on stars. |
| `GALAXY_ABL_NO_COLLAPSE_RADIATION_RESIST` | Let a dense cell ignite however irradiated it is. Tests whether retained stars suppress the next generation. |
| `GALAXY_ABL_COLLAPSE_RADIATION_RESIST` | Override the per-scenario irradiation level above which a dense cell defers ignition. |
| `GALAXY_ABL_BIRTH_VELOCITY_DISPERSION` | Override the per-scenario isotropic birth dispersion, as a multiple of local circular speed. `0` births cold. |
| `GALAXY_ABL_LENGTH_REFERENCE_SIZE` | Scale the sim's absolute length constants by `size / reference`, so every domain size is a scaled copy of the reference. |
| `GALAXY_ABL_RESOLVED_LUMINOSITY_FLOOR` | Override the per-scenario luminosity floor that bounds the resolved population. `0` disables retirement, the control the #72 numbers were taken against. |

## See also

- [ablation.md](ablation.md) - how to run the matrix and what it found.
- [stellar-heating.md](stellar-heating.md) - what the matrix found, and what shipped.
