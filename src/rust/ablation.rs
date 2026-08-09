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

use std::sync::OnceLock;

/// Resolved ablation configuration for this process.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ablation {
    /// Override the `gravity_field` process cadence, normally 4 ticks.
    /// Stars integrate against a field up to three ticks stale; `1` makes
    /// it fresh every tick. Tests whether the staleness is pumping energy.
    pub field_cadence: Option<u64>,
    /// 3x3 box-blur passes over the coarse star field after each rebuild.
    /// Removes small-scale clumpiness while keeping the large-scale
    /// pattern and the field's overall magnitude. Tests transient clump
    /// scattering on its own.
    pub field_smooth_passes: u32,
    /// Replace the coarse star field with its azimuthal average, so stars
    /// orbit a perfectly axisymmetric potential. Removes clumps *and* the
    /// spiral pattern - the upper bound on how much of the heating comes
    /// from non-axisymmetric structure of any kind.
    pub axisymmetric_field: bool,
    /// Leave stars out of the quadtree the star field is built from, so
    /// stars stop scattering off each other's associations. Isolates
    /// stellar self-gravity from gas clumpiness.
    pub no_star_self_gravity: bool,
    /// Zero the association binding acceleration. Associations still form,
    /// release, and stream; they just stop pulling on their members.
    pub no_association_binding: bool,
    /// Zero the internal velocity newborns receive about their
    /// association's center of mass, so the population is born cold.
    pub no_birth_dispersion: bool,
    /// Clamp a newborn association's orbital speed to this multiple of the
    /// local circular speed, instead of to the absolute
    /// `ASSOCIATION_ORBIT_SPEED_CAP`. 1.06 keeps newborns just above
    /// circular, well under the ~1.41 escape ratio.
    ///
    /// This one is not a force ablation. It was built and reverted on #66
    /// before `rotation_dispersion_ratio` existed, and judged against
    /// `star_circular_ratio`, which was then retracted as unable to tell a
    /// circular orbit from an eccentric one at pericenter. So its effect
    /// on the disk has never actually been measured. A switch is the
    /// cheapest way to measure it without re-landing a change that breaks
    /// the elliptical scenario.
    pub birth_orbit_ratio_cap: Option<f32>,
    /// Override `STAR_WAVE_COUPLING`, normally 0.0: the fraction of the
    /// analytic spiral and ring density-wave force that also acts on
    /// stars.
    ///
    /// This is the other half of the pair the ablation matrix pointed at.
    /// An axisymmetric field holds the disk but has no arms in it, so the
    /// question is whether a coherent analytic wave can put the arms back
    /// without heating the way a clump-dominated 64-grid field does. A
    /// density wave stars pass through should not scatter them.
    pub star_wave_coupling: Option<f32>,
    /// Bypass the `COLLAPSE_RADIATION_RESIST` gate in the collapse watch,
    /// so a dense cell can ignite regardless of how irradiated it is.
    ///
    /// Not a heating candidate. This one tests the star-formation drop
    /// that arrives with the birth ratio cap: capped stars stay in the
    /// disk instead of being flung into the halo, and the suspicion is
    /// that their radiation then suppresses the collapses that would have
    /// made the next generation. If collapse counts recover with the gate
    /// off, that loop is the mechanism.
    pub no_collapse_radiation_resist: bool,
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
        if self.axisymmetric_field {
            parts.push("axisymmetric-field".to_string());
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
        axisymmetric_field: flag_env("GALAXY_ABL_AXISYMMETRIC_FIELD"),
        no_star_self_gravity: flag_env("GALAXY_ABL_NO_STAR_SELF_GRAVITY"),
        no_association_binding: flag_env("GALAXY_ABL_NO_ASSOCIATION_BINDING"),
        no_birth_dispersion: flag_env("GALAXY_ABL_NO_BIRTH_DISPERSION"),
        birth_orbit_ratio_cap: parse_env("GALAXY_ABL_BIRTH_ORBIT_RATIO_CAP"),
        star_wave_coupling: parse_env("GALAXY_ABL_STAR_WAVE_COUPLING"),
        no_collapse_radiation_resist: flag_env("GALAXY_ABL_NO_COLLAPSE_RADIATION_RESIST"),
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
        // The resolved configuration comes from the environment, so this
        // asserts the routing rule rather than a particular override:
        // every other process keeps its declared cadence no matter what.
        assert_eq!(cadence_for("integrate_stars", 1), 1);
        assert_eq!(cadence_for("stellar_halo", 8), 8);
        let field = cadence_for("gravity_field", 4);
        assert_eq!(field, ablation().field_cadence.unwrap_or(4).max(1));
        assert!(field >= 1, "a zero cadence would divide by zero in is_due");
    }
}
