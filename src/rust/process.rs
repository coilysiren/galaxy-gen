//! Process registry: causality written down, checkable, greppable.
//! See docs/processes-events.md.
//!
//! The registry is a static list, not a framework - no dynamic topological
//! sorting, no parallel scheduling. The scheduler in `Galaxy::tick` runs
//! descriptors in declared order, skipping those whose cadence is not due,
//! then executes the events scheduled for this tick.

use crate::galaxy::Galaxy;

/// Coarse-grained state ownership keys for read/write declarations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateKey {
    GasMass,
    GasKinematics,
    GasAccel,
    GravityField,
    RadiationField,
    StarKinematics,
    StarLifecycle,
    CollapseWatch,
    EventQueue,
    BlackHole,
    HaloGas,
    StellarHalo,
    Composition,
}

pub struct ProcessDescriptor {
    pub name: &'static str,
    /// Everything the process reads. Reads of previous-tick state are fine.
    pub reads: &'static [StateKey],
    pub writes: &'static [StateKey],
    /// Reads that must be produced by an earlier process in the SAME tick.
    /// Validated by tests_graph below.
    pub requires_fresh: &'static [StateKey],
    /// Run every `cadence` ticks (tick_count % cadence == phase_offset).
    pub cadence: u64,
    pub phase_offset: u64,
    pub run: fn(&mut Galaxy, f32),
}

/// Declared execution order. This IS the causal chain - a change here is a
/// physics change, not a refactor.
pub fn registry() -> &'static [ProcessDescriptor] {
    &REGISTRY
}

static REGISTRY: &[ProcessDescriptor] = &[
    ProcessDescriptor {
        name: "gravity",
        reads: &[StateKey::GasMass],
        writes: &[StateKey::GasAccel],
        requires_fresh: &[],
        cadence: 1,
        phase_offset: 0,
        run: Galaxy::process_gravity,
    },
    ProcessDescriptor {
        name: "spiral_density_wave",
        reads: &[
            StateKey::GasMass,
            StateKey::GasKinematics,
            StateKey::GasAccel,
        ],
        writes: &[StateKey::GasAccel],
        requires_fresh: &[StateKey::GasAccel],
        cadence: 1,
        phase_offset: 0,
        run: Galaxy::process_spiral_density_wave,
    },
    ProcessDescriptor {
        name: "ring_density_wave",
        reads: &[
            StateKey::GasMass,
            StateKey::GasKinematics,
            StateKey::GasAccel,
        ],
        writes: &[StateKey::GasAccel],
        requires_fresh: &[StateKey::GasAccel],
        cadence: 1,
        phase_offset: 0,
        run: Galaxy::process_ring_density_wave,
    },
    ProcessDescriptor {
        name: "gas_pressure",
        reads: &[StateKey::GasMass, StateKey::GasAccel],
        writes: &[StateKey::GasAccel],
        requires_fresh: &[StateKey::GasAccel],
        cadence: 1,
        phase_offset: 0,
        run: Galaxy::process_gas_pressure,
    },
    ProcessDescriptor {
        // Offset 1 so the field exists from the first tick after seeding
        // instead of three ticks of zero-field star drift.
        name: "gravity_field",
        reads: &[StateKey::GasMass, StateKey::StarKinematics],
        writes: &[StateKey::GravityField],
        requires_fresh: &[],
        cadence: 4,
        phase_offset: 1,
        run: Galaxy::process_gravity_field,
    },
    ProcessDescriptor {
        name: "integrate_gas",
        reads: &[
            StateKey::GasMass,
            StateKey::GasKinematics,
            StateKey::GasAccel,
            StateKey::Composition,
        ],
        writes: &[
            StateKey::GasMass,
            StateKey::GasKinematics,
            StateKey::Composition,
        ],
        requires_fresh: &[StateKey::GasAccel],
        cadence: 1,
        phase_offset: 0,
        run: Galaxy::process_integrate_gas,
    },
    ProcessDescriptor {
        // Reads a possibly stale field by design - fields update less
        // often than motion. Hence no requires_fresh on GravityField.
        name: "integrate_stars",
        reads: &[
            StateKey::GravityField,
            StateKey::StarKinematics,
            StateKey::StarLifecycle,
        ],
        writes: &[StateKey::StarKinematics, StateKey::StarLifecycle],
        requires_fresh: &[],
        cadence: 1,
        phase_offset: 0,
        run: Galaxy::process_integrate_stars,
    },
    ProcessDescriptor {
        name: "radiation_field",
        reads: &[StateKey::StarKinematics, StateKey::StarLifecycle],
        writes: &[StateKey::RadiationField],
        requires_fresh: &[],
        cadence: 4,
        phase_offset: 1,
        run: Galaxy::process_radiation_field,
    },
    ProcessDescriptor {
        // Low cadence: lifecycle-scale rules run far less often than
        // motion. Emits CloudCollapse.
        name: "collapse_watch",
        reads: &[
            StateKey::GasMass,
            StateKey::GasKinematics,
            StateKey::RadiationField,
            StateKey::CollapseWatch,
        ],
        writes: &[StateKey::CollapseWatch, StateKey::EventQueue],
        requires_fresh: &[],
        cadence: 16,
        phase_offset: 3,
        run: Galaxy::process_collapse_watch,
    },
    ProcessDescriptor {
        // Stars that remain beyond the luminous disk phase-mix into an
        // unresolved stellar halo. Old dim remnants retire there too.
        name: "stellar_halo",
        reads: &[StateKey::StarKinematics, StateKey::StarLifecycle],
        writes: &[
            StateKey::StarKinematics,
            StateKey::StarLifecycle,
            StateKey::StellarHalo,
            StateKey::Composition,
        ],
        requires_fresh: &[],
        cadence: 8,
        phase_offset: 4,
        run: Galaxy::process_stellar_halo,
    },
    ProcessDescriptor {
        // Lifecycle-scale cadence. Emits envelope loss, both supernova
        // channels, and compact mergers, and advances retirement clocks.
        name: "stellar_aging",
        reads: &[StateKey::StarLifecycle, StateKey::Composition],
        writes: &[
            StateKey::StarLifecycle,
            StateKey::EventQueue,
            StateKey::Composition,
        ],
        requires_fresh: &[],
        cadence: 8,
        phase_offset: 7,
        run: Galaxy::process_stellar_aging,
    },
    ProcessDescriptor {
        // The hole feeds: core gas accretion plus BlackHoleCapture
        // emission for stars inside the capture radius.
        name: "bh_accretion",
        reads: &[
            StateKey::GasMass,
            StateKey::StarKinematics,
            StateKey::BlackHole,
            StateKey::Composition,
        ],
        writes: &[
            StateKey::GasMass,
            StateKey::BlackHole,
            StateKey::EventQueue,
            StateKey::Composition,
        ],
        requires_fresh: &[],
        cadence: 8,
        phase_offset: 2,
        run: Galaxy::process_bh_accretion,
    },
    ProcessDescriptor {
        // Hawking evaporation - the shrink half of the lifecycle. Feeds
        // the radiated ledger sink and heats the core radiation field.
        name: "bh_evaporation",
        reads: &[StateKey::BlackHole],
        writes: &[StateKey::BlackHole, StateKey::RadiationField],
        requires_fresh: &[],
        cadence: 8,
        phase_offset: 6,
        run: Galaxy::process_bh_evaporation,
    },
    ProcessDescriptor {
        // Emits CloudDissipate; irradiated gas rises into the hot halo.
        name: "gas_dissipation",
        reads: &[
            StateKey::GasMass,
            StateKey::RadiationField,
            StateKey::Composition,
        ],
        writes: &[
            StateKey::GasMass,
            StateKey::HaloGas,
            StateKey::EventQueue,
            StateKey::Composition,
        ],
        requires_fresh: &[],
        cadence: 8,
        phase_offset: 5,
        run: Galaxy::process_gas_dissipation,
    },
    ProcessDescriptor {
        // Feedback lifts visible gas into the halo, then cooling returns
        // it to the rotating disk so the cold reservoir breathes.
        name: "gas_fountain",
        reads: &[
            StateKey::GasMass,
            StateKey::GasKinematics,
            StateKey::StarLifecycle,
            StateKey::HaloGas,
            StateKey::Composition,
        ],
        writes: &[
            StateKey::GasMass,
            StateKey::GasKinematics,
            StateKey::HaloGas,
            StateKey::Composition,
        ],
        requires_fresh: &[],
        cadence: 8,
        phase_offset: 1,
        run: Galaxy::process_gas_fountain,
    },
];

/// True when `p` is due at `tick`.
pub fn is_due(p: &ProcessDescriptor, tick: u64) -> bool {
    tick % p.cadence == p.phase_offset % p.cadence
}

#[cfg(test)]
mod tests_graph {
    use super::*;

    #[test]
    fn test_fresh_reads_have_an_earlier_same_tick_writer() {
        let reg = registry();
        for (i, p) in reg.iter().enumerate() {
            for key in p.requires_fresh {
                assert!(
                    p.reads.contains(key),
                    "{}: requires_fresh key {key:?} must also be declared as a read",
                    p.name
                );
                let produced_earlier = reg[..i].iter().any(|earlier| {
                    earlier.writes.contains(key)
                        && earlier.cadence == p.cadence
                        && earlier.phase_offset == p.phase_offset
                });
                assert!(
                    produced_earlier,
                    "{}: requires fresh {key:?} but no earlier process with a \
                     matching cadence writes it",
                    p.name
                );
            }
        }
    }

    #[test]
    fn test_every_process_declares_reads_and_writes() {
        for p in registry() {
            assert!(
                !p.reads.is_empty() || !p.writes.is_empty(),
                "{}: a process with no declared reads or writes is either \
                 dead or lying",
                p.name
            );
            assert!(p.cadence >= 1, "{}: cadence must be at least 1", p.name);
        }
    }

    #[test]
    fn test_golden_ordering() {
        // The causal chain, spelled out. Extending the registry means
        // extending this list deliberately.
        let names: Vec<&str> = registry().iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            vec![
                "gravity",
                "spiral_density_wave",
                "ring_density_wave",
                "gas_pressure",
                "gravity_field",
                "integrate_gas",
                "integrate_stars",
                "radiation_field",
                "collapse_watch",
                "stellar_halo",
                "stellar_aging",
                "bh_accretion",
                "bh_evaporation",
                "gas_dissipation",
                "gas_fountain",
            ]
        );
    }

    #[test]
    fn test_cadence_due_math() {
        let p = ProcessDescriptor {
            name: "t",
            reads: &[],
            writes: &[StateKey::GasMass],
            requires_fresh: &[],
            cadence: 4,
            phase_offset: 1,
            run: |_, _| {},
        };
        assert!(is_due(&p, 1));
        assert!(!is_due(&p, 2));
        assert!(is_due(&p, 5));
    }
}
