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
    /// A star crossed the central black hole's capture radius.
    BlackHoleCapture = 5,
    /// A bound neutron-star pair reached its seeded merger delay.
    NeutronStarMerger = 6,
    /// The compact merger launched its brief relativistic jets.
    GammaRayBurst = 7,
}

pub const EVENT_KIND_COUNT: usize = 8;

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
    executed_counts: [u64; EVENT_KIND_COUNT],
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
            executed_counts: [0; EVENT_KIND_COUNT],
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

    pub fn pending(&self) -> impl Iterator<Item = &Event> {
        self.pending.iter()
    }

    pub fn executed_count(&self, kind: EventKind) -> u64 {
        self.executed_counts[kind as usize]
    }

    pub fn executed_total(&self) -> u64 {
        self.executed_counts.iter().sum()
    }

    /// Flat u32 serialization for the worker state round-trip. Layout:
    /// [next_id lo/hi, seq_tick lo/hi, seq_in_tick, n_pending, then 11
    /// u32 per pending event]. The instrumentation ring and counters are
    /// intentionally dropped - they are diagnostics, not sim state.
    pub fn to_flat(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(6 + self.pending.len() * 11);
        out.push(self.next_id as u32);
        out.push((self.next_id >> 32) as u32);
        out.push(self.seq_tick as u32);
        out.push((self.seq_tick >> 32) as u32);
        out.push(self.seq_in_tick);
        out.push(self.pending.len() as u32);
        for ev in &self.pending {
            out.push(ev.id as u32);
            out.push((ev.id >> 32) as u32);
            out.push(ev.tick as u32);
            out.push((ev.tick >> 32) as u32);
            out.push(ev.seq);
            out.push(ev.kind as u32);
            out.push(ev.source);
            out.push(ev.target);
            out.push(ev.payload.to_bits());
            out.push(ev.parent as u32);
            out.push((ev.parent >> 32) as u32);
        }
        out
    }

    pub fn from_flat(data: &[u32]) -> EventQueue {
        let mut q = EventQueue::new();
        if data.len() < 6 {
            return q;
        }
        q.next_id = data[0] as u64 | ((data[1] as u64) << 32);
        q.seq_tick = data[2] as u64 | ((data[3] as u64) << 32);
        q.seq_in_tick = data[4];
        let n = data[5] as usize;
        for chunk in data[6..].chunks_exact(11).take(n) {
            q.pending.push(Event {
                id: chunk[0] as u64 | ((chunk[1] as u64) << 32),
                tick: chunk[2] as u64 | ((chunk[3] as u64) << 32),
                seq: chunk[4],
                kind: kind_from_u32(chunk[5]),
                source: chunk[6],
                target: chunk[7],
                payload: f32::from_bits(chunk[8]),
                parent: chunk[9] as u64 | ((chunk[10] as u64) << 32),
            });
        }
        q
    }
}

fn kind_from_u32(v: u32) -> EventKind {
    match v {
        0 => EventKind::CloudCollapse,
        1 => EventKind::StarBirth,
        2 => EventKind::Supernova,
        3 => EventKind::ShockWave,
        5 => EventKind::BlackHoleCapture,
        6 => EventKind::NeutronStarMerger,
        7 => EventKind::GammaRayBurst,
        _ => EventKind::CloudDissipate,
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
    fn test_flat_round_trip_preserves_pending_and_counters() {
        let mut q = EventQueue::new();
        q.emit(9, EventKind::Supernova, 5, NO_REF, 1.25, NO_PARENT);
        q.emit(9, EventKind::ShockWave, 5, 77, -3.5, 1);
        q.emit(9, EventKind::GammaRayBurst, 9, 42, 18.0, 2);
        let flat = q.to_flat();
        let mut back = EventQueue::from_flat(&flat);
        let due = back.take_due(10);
        assert_eq!(due.len(), 3);
        assert_eq!(due[0].kind, EventKind::Supernova);
        assert_eq!(due[1].payload, -3.5);
        assert_eq!(due[1].parent, 1);
        assert_eq!(due[2].kind, EventKind::GammaRayBurst);
        // Emission after restore continues the id sequence.
        let id = back.emit(10, EventKind::CloudCollapse, 0, NO_REF, 0.0, NO_PARENT);
        assert_eq!(id, 4);
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
