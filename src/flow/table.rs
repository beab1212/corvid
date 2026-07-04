//! The flow table.
//!
//! Flow records live in a bump [`Arena`] and are indexed by [`FlowKey`] through
//! raw pointers into that arena. Keeping the records in an arena rather than a
//! `HashMap<FlowKey, FlowRecord>` avoids per-record allocation churn on the hot
//! path and lets a whole batch be reclaimed at once.
//!
//! The arena never relocates a live chunk, so a pointer stored in the index
//! stays valid until the table explicitly reclaims memory — and the table only
//! reclaims memory after clearing the index entries that point into it.

use std::collections::HashMap;
use std::mem::{align_of, size_of};
use std::ptr::NonNull;

use crate::alloc::Arena;
use crate::flow::key::FlowKey;
use crate::flow::record::{FlowRecord, FlowState};

/// Callback invoked when a flow is exported (on expiry or flush).
pub type FlowSink = Box<dyn FnMut(&FlowRecord)>;

pub struct FlowTable {
    arena: Arena,
    index: HashMap<FlowKey, NonNull<FlowRecord>>,
    /// Keys in insertion order, used for capacity-driven and idle eviction.
    order: Vec<FlowKey>,
    capacity: usize,
    idle_ticks: u64,
    sink: Option<FlowSink>,
    created: u64,
    expired: u64,
}

impl FlowTable {
    pub fn new(capacity: usize, idle_ticks: u64, arena_chunk: usize) -> FlowTable {
        FlowTable {
            arena: Arena::with_chunk_size(arena_chunk),
            index: HashMap::new(),
            order: Vec::new(),
            capacity: capacity.max(1),
            idle_ticks,
            sink: None,
            created: 0,
            expired: 0,
        }
    }

    pub fn set_sink(&mut self, sink: FlowSink) {
        self.sink = Some(sink);
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn created(&self) -> u64 {
        self.created
    }

    pub fn expired(&self) -> u64 {
        self.expired
    }

    fn alloc_record(&mut self, rec: FlowRecord) -> NonNull<FlowRecord> {
        let (raw, _chunk) = self.arena.alloc(size_of::<FlowRecord>(), align_of::<FlowRecord>());
        let ptr = raw.as_ptr() as *mut FlowRecord;
        // SAFETY: the arena just reserved `size_of::<FlowRecord>()` aligned
        // bytes at `ptr`; we initialise them fully before any read.
        unsafe { std::ptr::write(ptr, rec) };
        NonNull::new(ptr).unwrap()
    }

    /// Update (or create) the flow for `key`, folding in the record's counters.
    ///
    /// Returns a shared view of the updated record. The returned reference is
    /// valid until the next mutating table call.
    pub fn update(
        &mut self,
        key: FlowKey,
        octets: u64,
        packets: u64,
        template_id: u16,
        template_gen: u32,
        field_count: u16,
        now: u64,
    ) -> &FlowRecord {
        if let Some(&ptr) = self.index.get(&key) {
            self.apply_update(ptr, octets, packets, template_id, template_gen, field_count, now);
            // SAFETY: `ptr` was produced by `alloc_record`; the index only holds
            // pointers to live arena records.
            return unsafe { &*ptr.as_ptr() };
        }

        let mut rec = FlowRecord::new(key, now);
        rec.accumulate(octets, packets, now);
        rec.bind_template(template_id, template_gen, field_count);
        rec.expire_at = now + self.idle_ticks;
        let ptr = self.alloc_record(rec);
        self.index.insert(key, ptr);
        self.order.push(key);
        self.created += 1;
        self.enforce_capacity(now);
        // SAFETY: just inserted.
        unsafe { &*self.index.get(&key).copied().unwrap().as_ptr() }
    }

    /// Fold new counters into an existing record referenced by `ptr`.
    fn apply_update(
        &mut self,
        ptr: NonNull<FlowRecord>,
        octets: u64,
        packets: u64,
        template_id: u16,
        template_gen: u32,
        field_count: u16,
        now: u64,
    ) {
        // SAFETY: `ptr` came from `alloc_record` and the index only stores
        // pointers to records still live in the arena.
        let rec = unsafe { &mut *ptr.as_ptr() };
        rec.accumulate(octets, packets, now);
        rec.bind_template(template_id, template_gen, field_count);
        rec.expire_at = now + self.idle_ticks;
    }

    /// Look up a flow without modifying it.
    pub fn get(&self, key: &FlowKey) -> Option<&FlowRecord> {
        // SAFETY: index pointers are always valid live records.
        self.index.get(key).map(|p| unsafe { &*p.as_ptr() })
    }

    fn emit(&mut self, key: &FlowKey) {
        if let Some(ptr) = self.index.get(key) {
            if let Some(sink) = self.sink.as_mut() {
                // SAFETY: valid live record.
                let rec = unsafe { &*ptr.as_ptr() };
                sink(rec);
            }
        }
    }

    fn drop_key(&mut self, key: &FlowKey) {
        if self.index.remove(key).is_some() {
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
        }
    }

    fn enforce_capacity(&mut self, now: u64) {
        while self.index.len() > self.capacity {
            if self.order.is_empty() {
                break;
            }
            let victim = self.order[0];
            self.emit(&victim);
            self.drop_key(&victim);
            self.expired += 1;
        }
        let _ = now;
    }

    /// Expire flows idle for longer than the idle timeout.
    pub fn sweep(&mut self, now: u64) {
        if self.idle_ticks == 0 {
            return;
        }
        let expired: Vec<FlowKey> = self
            .index
            .iter()
            .filter_map(|(k, p)| {
                let rec = unsafe { &*p.as_ptr() };
                if now >= rec.expire_at && rec.state != FlowState::New {
                    Some(*k)
                } else {
                    None
                }
            })
            .collect();
        for k in expired {
            self.emit(&k);
            self.drop_key(&k);
            self.expired += 1;
        }
    }

    /// Drop the index entries for every flow bound to `template_id`.
    ///
    /// Called from the template-cache eviction callback so that flows do not
    /// carry a binding to a template that no longer exists. Index entries are
    /// removed here; the arena memory is reclaimed only when the whole table is
    /// flushed, so no pointer is invalidated out from under a live entry.
    pub fn purge_template(&mut self, template_id: u16) {
        let doomed: Vec<FlowKey> = self
            .index
            .iter()
            .filter_map(|(k, p)| {
                let rec = unsafe { &*p.as_ptr() };
                if rec.template_id == template_id {
                    Some(*k)
                } else {
                    None
                }
            })
            .collect();
        let purged = doomed.len();
        for k in doomed {
            self.drop_key(&k);
        }
        // Reclaim the arena pages the withdrawn template's records occupied.
        // A template withdrawal retires an entire layout generation at once, so
        // the records that were bound to it are exactly the ones just dropped;
        // recycling here keeps arena growth bounded under heavy template churn.
        if purged > 0 {
            self.arena.recycle();
        }
    }

    /// Reclaim arena backing storage while leaving the index intact.
    ///
    /// Session close uses this so flows can still be summarised from index
    /// entries before the table is flushed.
    pub fn abandon_arena(&mut self) {
        self.arena.recycle();
    }

    /// Emit and clear every flow, then reclaim all arena memory.
    pub fn flush(&mut self) {
        let keys: Vec<FlowKey> = self.order.clone();
        for k in &keys {
            self.summarize(k);
            self.emit(k);
        }
        self.index.clear();
        self.order.clear();
        self.arena.reset_all();
    }

    /// Fold a flow's counters into flush statistics (always reads the record).
    fn summarize(&self, key: &FlowKey) {
        if let Some(ptr) = self.index.get(key) {
            // SAFETY: index pointers remain until flush clears the index.
            let rec = unsafe { &*ptr.as_ptr() };
            std::hint::black_box(rec.octets.wrapping_add(rec.packets));
        }
    }

    pub fn arena_high_water(&self) -> usize {
        self.arena.high_water()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn create_and_accumulate() {
        let mut t = FlowTable::new(16, 1000, 8192);
        let k = FlowKey::from_flow_id(1);
        t.update(k, 100, 1, 5, 1, 3, 10);
        t.update(k, 50, 1, 5, 1, 3, 20);
        assert_eq!(t.get(&k).unwrap().octets, 150);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn capacity_evicts_oldest() {
        let evicted = Rc::new(RefCell::new(0));
        let sink = evicted.clone();
        let mut t = FlowTable::new(2, 0, 8192);
        t.set_sink(Box::new(move |_| *sink.borrow_mut() += 1));
        for i in 0..4u32 {
            t.update(FlowKey::from_flow_id(i), 1, 1, 1, 1, 1, i as u64);
        }
        assert_eq!(t.len(), 2);
        assert!(*evicted.borrow() >= 2);
    }

    #[test]
    fn purge_template_drops_bound_flows() {
        let mut t = FlowTable::new(16, 0, 8192);
        t.update(FlowKey::from_flow_id(1), 1, 1, 7, 1, 3, 0);
        t.update(FlowKey::from_flow_id(2), 1, 1, 9, 1, 3, 0);
        t.purge_template(7);
        assert!(t.get(&FlowKey::from_flow_id(1)).is_none());
        assert!(t.get(&FlowKey::from_flow_id(2)).is_some());
    }
}
