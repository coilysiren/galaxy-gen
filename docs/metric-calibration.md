# Metric calibration

Why every star metric has a test against a population whose answer is known by construction.

Every metric here has a unit test against a population whose answer is known by construction - a cold rotating ring must score high, the same stars randomized must score near zero. A metric nobody has checked that way is how #66 went wrong the first time: one that silently returns garbage still produces a plausible curve.

The age-split calibration is worth stating precisely, because the generational-offset effect is weaker than it looks. Two cold cohorts a factor of two apart in speed still pool to about 4.2, so offset alone cannot drive pooled `vsig` under 1.0 unless the old cohort has lost nearly all its streaming - which is heating again.

The ablation switches are calibrated too: axisymmetrization must remove azimuthal structure *without* changing the rotation curve, and smoothing must remove cell-scale roughness while leaving field strength alone. Otherwise a run under either measures a different galaxy rather than the same galaxy with one thing removed.


## See also

- [star-metrics.md](star-metrics.md) - the metrics themselves.
- [ablation-switches.md](ablation-switches.md) - the switches calibrated here.
