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
        name: "integrate_gas",
        reads: &[StateKey::GasMass, StateKey::GasKinematics, StateKey::GasAccel],
        writes: &[StateKey::GasMass, StateKey::GasKinematics],
        requires_fresh: &[StateKey::GasAccel],
        cadence: 1,
        phase_offset: 0,
        run: Galaxy::process_integrate_gas,
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
        assert_eq!(names, vec!["gravity", "integrate_gas"]);
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
