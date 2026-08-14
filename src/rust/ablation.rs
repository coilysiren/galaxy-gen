//! Ablation switches for the stellar-heating investigation, galaxy-gen#66.
//!
//! The stellar disk starts rotation-dominated (`vsig` above 2.5) and ends
//! pressure-supported (below 0.5) by t=2500. Two candidate fixes were tried
//! and neither moved the crossover, so the remaining method is ablation:
//! turn one candidate heat source off at a time and watch whether the
//! crossover moves. Tuning a constant answers a different question.
//!
//! Every switch is off by default. A default native build and every wasm
//! build - which reads no environment at all - run the shipped physics
//! byte-for-byte unchanged, so this module cannot alter the site or the
//! golden fields while it sits here inert.
//!
//! Reading the switches from the sim rather than hand-editing constants per
//! run is deliberate. The previous sweep on #66 had to be retracted because
//! a constant was reverted in the probe but not at the call site, and the
//! probe was believed. A switch the kernel itself reads cannot drift from
//! what actually ran, and `debug-sim` prints the resolved configuration.
//!
//! Only two force paths reach a star: the coarse gravity field
//! (`Galaxy::process_gravity_field` plus `Galaxy::sample_field`) and the
//! association binding potential in `Galaxy::process_integrate_stars`.
//! `STAR_WAVE_COUPLING` is 0.0 and supernovae kick gas, not stars. The
//! switches below cover both live paths and the birth velocities that set
//! the population's initial dispersion.
//!
//! Per-switch reasoning lives in docs/ablation-rationale.md.

use std::sync::OnceLock;

/// Resolved ablation configuration for this process.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ablation {
    /// Override the `gravity_field` cadence, normally 4 ticks. `1` rebuilds the
    /// field every tick, testing whether staleness pumps energy.
    pub field_cadence: Option<u64>,
    /// 3x3 box-blur passes over the coarse star field after each rebuild, cutting
    /// small-scale clumpiness while keeping the large-scale pattern.
    pub field_smooth_passes: u32,
    /// Override the shipped axisymmetric star field. `0` restores the raw
    /// clumpy field - the control the #70 numbers were taken against.
    pub axisymmetric_field: Option<bool>,
    /// Leave stars out of the quadtree the star field is built from, isolating
    /// stellar self-gravity from gas clumpiness.
    pub no_star_self_gravity: bool,
    /// Zero the association binding acceleration. Associations still form,
    /// release, and stream; they just stop pulling on their members.
    pub no_association_binding: bool,
    /// Zero the internal velocity newborns receive about their association's
    /// center of mass, so the population is born cold.
    pub no_birth_dispersion: bool,
    /// Override the per-scenario newborn orbital ratio cap. `0` disables it,
    /// leaving only the absolute cap - the pre-#70 control.
    pub birth_orbit_ratio_cap: Option<f32>,
    /// Override `STAR_WAVE_COUPLING`, normally 0.0: the fraction of the analytic
    /// spiral and ring density-wave force that also acts on stars.
    pub star_wave_coupling: Option<f32>,
    /// Bypass the `COLLAPSE_RADIATION_RESIST` gate in the collapse watch, so a
    /// dense cell ignites however irradiated it is.
    pub no_collapse_radiation_resist: bool,
    /// Override `COLLAPSE_RADIATION_RESIST`, the irradiation level above which
    /// a dense cell defers ignition. Higher lets more gas ignite.
    pub collapse_radiation_resist: Option<f32>,
    /// Override the per-scenario isotropic birth dispersion, as a multiple of
    /// local circular speed. `0` births cold - the pre-#70 control.
    pub birth_velocity_dispersion: Option<f32>,
    /// Reference domain size for the sim's absolute length constants. Every length
    /// is scaled by `size / reference`, making any run a scaled copy.
    pub length_reference_size: Option<f32>,
    /// Override `RESOLVED_LUMINOSITY_FLOOR`, below which an unbound main-sequence
    /// star stops being drawn as a point. `0` disables retirement.
    pub resolved_luminosity_floor: Option<f32>,
}

impl Ablation {
    /// True when nothing is switched on, i.e. the shipped physics.
    pub fn is_inert(&self) -> bool {
        *self == Ablation::default()
    }

    /// One-line summary for the `debug-sim` header, so a captured run
    /// records the configuration it was produced under.
    pub fn describe(&self) -> String {
        if self.is_inert() {
            return "baseline".to_string();
        }
        let mut parts: Vec<String> = Vec::new();
        if let Some(cadence) = self.field_cadence {
            parts.push(format!("field-cadence={cadence}"));
        }
        if self.field_smooth_passes > 0 {
            parts.push(format!("field-smooth={}", self.field_smooth_passes));
        }
        if let Some(axisymmetric) = self.axisymmetric_field {
            parts.push(format!("axisymmetric-field={axisymmetric}"));
        }
        if self.no_star_self_gravity {
            parts.push("no-star-self-gravity".to_string());
        }
        if self.no_association_binding {
            parts.push("no-association-binding".to_string());
        }
        if self.no_birth_dispersion {
            parts.push("no-birth-dispersion".to_string());
        }
        if let Some(cap) = self.birth_orbit_ratio_cap {
            parts.push(format!("birth-orbit-ratio-cap={cap}"));
        }
        if let Some(coupling) = self.star_wave_coupling {
            parts.push(format!("star-wave-coupling={coupling}"));
        }
        if self.no_collapse_radiation_resist {
            parts.push("no-collapse-radiation-resist".to_string());
        }
        if let Some(resist) = self.collapse_radiation_resist {
            parts.push(format!("collapse-radiation-resist={resist}"));
        }
        if let Some(sigma) = self.birth_velocity_dispersion {
            parts.push(format!("birth-velocity-dispersion={sigma}"));
        }
        if let Some(reference) = self.length_reference_size {
            parts.push(format!("length-reference-size={reference}"));
        }
        if let Some(floor) = self.resolved_luminosity_floor {
            parts.push(format!("resolved-luminosity-floor={floor}"));
        }
        parts.join(",")
    }
}

static RESOLVED: OnceLock<Ablation> = OnceLock::new();

/// The ablation configuration, resolved once per process.
pub fn ablation() -> &'static Ablation {
    RESOLVED.get_or_init(load)
}

/// Cadence a process should actually run at. Only `gravity_field` is
/// overridable; everything else returns its declared cadence.
pub fn cadence_for(name: &str, declared: u64) -> u64 {
    if name != "gravity_field" {
        return declared;
    }
    ablation().field_cadence.unwrap_or(declared).max(1)
}

#[cfg(target_arch = "wasm32")]
fn load() -> Ablation {
    // The browser build has no environment to read and must never diverge
    // from the shipped physics.
    Ablation::default()
}

#[cfg(not(target_arch = "wasm32"))]
fn load() -> Ablation {
    Ablation {
        field_cadence: parse_env("GALAXY_ABL_FIELD_CADENCE"),
        field_smooth_passes: parse_env("GALAXY_ABL_FIELD_SMOOTH").unwrap_or(0),
        axisymmetric_field: parse_env::<u8>("GALAXY_ABL_AXISYMMETRIC_FIELD").map(|v| v != 0),
        no_star_self_gravity: flag_env("GALAXY_ABL_NO_STAR_SELF_GRAVITY"),
        no_association_binding: flag_env("GALAXY_ABL_NO_ASSOCIATION_BINDING"),
        no_birth_dispersion: flag_env("GALAXY_ABL_NO_BIRTH_DISPERSION"),
        birth_orbit_ratio_cap: parse_env("GALAXY_ABL_BIRTH_ORBIT_RATIO_CAP"),
        star_wave_coupling: parse_env("GALAXY_ABL_STAR_WAVE_COUPLING"),
        no_collapse_radiation_resist: flag_env("GALAXY_ABL_NO_COLLAPSE_RADIATION_RESIST"),
        collapse_radiation_resist: parse_env("GALAXY_ABL_COLLAPSE_RADIATION_RESIST"),
        birth_velocity_dispersion: parse_env("GALAXY_ABL_BIRTH_VELOCITY_DISPERSION"),
        length_reference_size: parse_env("GALAXY_ABL_LENGTH_REFERENCE_SIZE"),
        resolved_luminosity_floor: parse_env("GALAXY_ABL_RESOLVED_LUMINOSITY_FLOOR"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_env<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok()?.trim().parse().ok()
}

/// Unset, empty, `0`, and `false` are all off; anything else is on.
#[cfg(not(target_arch = "wasm32"))]
fn flag_env(key: &str) -> bool {
    match std::env::var(key) {
        Ok(value) => {
            let value = value.trim();
            !(value.is_empty() || value == "0" || value.eq_ignore_ascii_case("false"))
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_ablation_is_inert_and_describes_itself_as_baseline() {
        let default = Ablation::default();
        assert!(default.is_inert());
        assert_eq!(default.describe(), "baseline");
    }

    #[test]
    fn test_any_switch_makes_the_configuration_visible() {
        let ablated = Ablation {
            field_cadence: Some(1),
            no_association_binding: true,
            ..Ablation::default()
        };
        assert!(!ablated.is_inert());
        assert_eq!(ablated.describe(), "field-cadence=1,no-association-binding");
    }

    #[test]
    fn test_cadence_override_applies_only_to_the_field_rebuild() {
        // Asserts the routing rule rather than a particular override: the
        // resolved configuration comes from the environment.
        assert_eq!(cadence_for("integrate_stars", 1), 1);
        assert_eq!(cadence_for("stellar_halo", 8), 8);
        let field = cadence_for("gravity_field", 4);
        assert_eq!(field, ablation().field_cadence.unwrap_or(4).max(1));
        assert!(field >= 1, "a zero cadence would divide by zero in is_due");
    }
}
