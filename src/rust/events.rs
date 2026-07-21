//! Deterministic queued-event model. See docs/processes-events.md.
//!
//! Events emitted during tick N are scheduled for tick N+1 and execute in
//! stable emission order - same-tick recursive execution is structurally
//! impossible. A bounded ring of executed events feeds instrumentation and
//! renderer transients (a supernova flash is the renderer's reading of a
//! Supernova event, never authoritative sim state).

use std::collections::VecDeque;

/// Discrete transition kinds for the living-galaxy causal loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    CloudCollapse = 0,
    StarBirth = 1,
    Supernova = 2,
    ShockWave = 3,
    CloudDissipate = 4,
}

/// No-source / no-target / no-parent sentinels.
pub const NO_REF: u32 = u32::MAX;
pub const NO_PARENT: u64 = 0;

#[derive(Clone, Copy, Debug)]
pub struct Event {
    /// Globally unique, monotonically assigned. Ordering by id equals
    /// ordering by (tick, seq).
    pub id: u64,
    /// Tick at which the event executes (emission tick + 1).
    pub tick: u64,
    /// Emission order within the emitting tick.
    pub seq: u32,
    pub kind: EventKind,
    /// Cell index or star index depending on kind. NO_REF when absent.
    pub source: u32,
    /// Target cell (region center) or star index. NO_REF when absent.
    pub target: u32,
    /// Kind-dependent scalar (mass budget, kick strength, ...).
    pub payload: f32,
    /// Causal parent event id, NO_PARENT for root causes.
    pub parent: u64,
}

#[derive(Clone)]
pub struct EventQueue {
    /// Scheduled events in emission (= id) order.
    pending: Vec<Event>,
    /// Executed events, newest last, bounded by ring_cap.
    ring: VecDeque<Event>,
    ring_cap: usize,
    next_id: u64,
    /// Tick the seq counter belongs to.
    seq_tick: u64,
    seq_in_tick: u32,
    /// Per-kind executed-event counters for instrumentation.
    executed_counts: [u64; 5],
}

impl EventQueue {
    pub fn new() -> EventQueue {
        EventQueue {
            pending: Vec::new(),
            ring: VecDeque::new(),
            ring_cap: 256,
            next_id: 1,
            seq_tick: 0,
            seq_in_tick: 0,
            executed_counts: [0; 5],
        }
    }

    /// Schedule an event for the tick after `current_tick`. Returns its id.
    pub fn emit(
        &mut self,
        current_tick: u64,
        kind: EventKind,
        source: u32,
        target: u32,
        payload: f32,
        parent: u64,
    ) -> u64 {
        if self.seq_tick != current_tick {
            self.seq_tick = current_tick;
            self.seq_in_tick = 0;
        }
        let ev = Event {
            id: self.next_id,
            tick: current_tick + 1,
            seq: self.seq_in_tick,
            kind,
            source,
            target,
            payload,
            parent,
        };
        self.next_id += 1;
        self.seq_in_tick += 1;
        self.pending.push(ev);
        ev.id
    }

    /// Drain every event due at or before `tick`, in stable (tick, seq)
    /// order. Emission order is monotonic in id, so pending is already
    /// sorted and a split preserves ordering.
    pub fn take_due(&mut self, tick: u64) -> Vec<Event> {
        let mut due = Vec::new();
        let mut rest = Vec::with_capacity(self.pending.len());
        for ev in self.pending.drain(..) {
            if ev.tick <= tick {
                due.push(ev);
            } else {
                rest.push(ev);
            }
        }
        self.pending = rest;
        due
    }

    /// Record an executed event into the instrumentation ring.
    pub fn record_executed(&mut self, ev: Event) {
        self.executed_counts[ev.kind as usize] += 1;
        if self.ring.len() == self.ring_cap {
            self.ring.pop_front();
        }
        self.ring.push_back(ev);
    }

    pub fn recent(&self) -> impl Iterator<Item = &Event> {
        self.ring.iter()
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn executed_count(&self, kind: EventKind) -> u64 {
        self.executed_counts[kind as usize]
    }

    pub fn executed_total(&self) -> u64 {
        self.executed_counts.iter().sum()
    }
}

#[cfg(test)]
mod tests_event_queue {
    use super::*;

    #[test]
    fn test_events_execute_next_tick_in_emission_order() {
        let mut q = EventQueue::new();
        q.emit(5, EventKind::CloudCollapse, 10, NO_REF, 1.0, NO_PARENT);
        q.emit(5, EventKind::StarBirth, 11, NO_REF, 2.0, NO_PARENT);
        assert!(q.take_due(5).is_empty(), "same-tick execution is forbidden");
        let due = q.take_due(6);
        assert_eq!(due.len(), 2);
        assert_eq!(due[0].kind, EventKind::CloudCollapse);
        assert_eq!(due[0].seq, 0);
        assert_eq!(due[1].kind, EventKind::StarBirth);
        assert_eq!(due[1].seq, 1);
        assert!(due[0].id < due[1].id);
    }

    #[test]
    fn test_handler_emission_lands_one_tick_further() {
        let mut q = EventQueue::new();
        let parent = q.emit(1, EventKind::Supernova, 3, NO_REF, 0.0, NO_PARENT);
        let due = q.take_due(2);
        assert_eq!(due.len(), 1);
        // A handler running at tick 2 emits with current_tick = 2.
        q.emit(2, EventKind::ShockWave, 3, 40, 0.5, due[0].id);
        assert!(q.take_due(2).is_empty());
        let next = q.take_due(3);
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].parent, parent);
    }

    #[test]
    fn test_seq_resets_per_tick_and_ids_stay_monotonic() {
        let mut q = EventQueue::new();
        let a = q.emit(1, EventKind::CloudCollapse, 0, NO_REF, 0.0, NO_PARENT);
        let b = q.emit(2, EventKind::CloudCollapse, 1, NO_REF, 0.0, NO_PARENT);
        let due1 = q.take_due(2);
        let due2 = q.take_due(3);
        assert_eq!(due1[0].seq, 0);
        assert_eq!(due2[0].seq, 0, "seq must reset per emitting tick");
        assert!(b > a);
    }

    #[test]
    fn test_ring_is_bounded_and_counts_by_kind() {
        let mut q = EventQueue::new();
        for i in 0..300u64 {
            let ev = Event {
                id: i + 1,
                tick: i,
                seq: 0,
                kind: EventKind::Supernova,
                source: NO_REF,
                target: NO_REF,
                payload: 0.0,
                parent: NO_PARENT,
            };
            q.record_executed(ev);
        }
        assert_eq!(q.recent().count(), 256);
        assert_eq!(q.executed_count(EventKind::Supernova), 300);
    }
}
