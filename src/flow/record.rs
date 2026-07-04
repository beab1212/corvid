//! The per-flow accumulation record.
//!
//! Records are plain data with a fixed layout so they can live directly in an
//! arena chunk (see [`crate::flow::table`]). Everything here is `Copy`-friendly;
//! there are no owned allocations inside a record.

use crate::flow::key::FlowKey;

/// Coarse lifecycle state of a flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FlowState {
    New = 0,
    Active = 1,
    Idle = 2,
    Closing = 3,
}

/// A flow's running totals plus the template binding it was last updated under.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FlowRecord {
    pub key: FlowKey,
    pub octets: u64,
    pub packets: u64,
    pub records: u64,
    pub first_ms: u64,
    pub last_ms: u64,
    pub expire_at: u64,
    /// Id of the template this flow was last decoded with.
    pub template_id: u16,
    /// Generation of that template, so a layout change is detectable.
    pub template_gen: u32,
    /// Field count of the bound template, cached for the layout-change check.
    pub bound_field_count: u16,
    pub state: FlowState,
    pub layout_changed: bool,
}

impl FlowRecord {
    pub fn new(key: FlowKey, now: u64) -> FlowRecord {
        FlowRecord {
            key,
            octets: 0,
            packets: 0,
            records: 0,
            first_ms: now,
            last_ms: now,
            expire_at: now,
            template_id: 0,
            template_gen: 0,
            bound_field_count: 0,
            state: FlowState::New,
            layout_changed: false,
        }
    }

    pub fn accumulate(&mut self, octets: u64, packets: u64, now: u64) {
        if self.records == 0 {
            self.first_ms = now;
        }
        self.octets = self.octets.wrapping_add(octets);
        self.packets = self.packets.wrapping_add(packets);
        self.records = self.records.wrapping_add(1);
        self.last_ms = now;
        if self.state == FlowState::New {
            self.state = FlowState::Active;
        }
    }

    pub fn bind_template(&mut self, id: u16, generation: u32, field_count: u16) {
        if self.template_id == id && self.template_gen != generation {
            self.layout_changed = true;
        }
        self.template_id = id;
        self.template_gen = generation;
        self.bound_field_count = field_count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulation() {
        let mut r = FlowRecord::new(FlowKey::from_flow_id(1), 100);
        r.accumulate(50, 1, 100);
        r.accumulate(50, 1, 200);
        assert_eq!(r.octets, 100);
        assert_eq!(r.records, 2);
        assert_eq!(r.state, FlowState::Active);
    }

    #[test]
    fn layout_change_flagged() {
        let mut r = FlowRecord::new(FlowKey::from_flow_id(1), 0);
        r.bind_template(5, 1, 3);
        r.bind_template(5, 2, 4);
        assert!(r.layout_changed);
    }
}
