//! An in-memory keyed record store with TTL and LRU eviction.
//!
//! Decoded records can be parked here keyed by an application id (e.g. a
//! correlation key) so a later message can retrieve and augment them. Entries
//! expire on a logical clock and the store is bounded, evicting the
//! least-recently-used entry when full.

use std::collections::HashMap;

use crate::decode::RecordImage;

struct Entry {
    image: RecordImage,
    inserted: u64,
    /// Monotonic access stamp used for LRU ordering, independent of the
    /// logical clock (which only governs TTL).
    touched_at: u64,
}

pub struct RecordStore {
    entries: HashMap<u64, Entry>,
    capacity: usize,
    ttl: u64,
    clock: u64,
    access: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl RecordStore {
    pub fn new(capacity: usize, ttl: u64) -> RecordStore {
        RecordStore {
            entries: HashMap::new(),
            capacity: capacity.max(1),
            ttl,
            clock: 0,
            access: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn stats(&self) -> (u64, u64, u64) {
        (self.hits, self.misses, self.evictions)
    }

    /// Advance the store's logical clock, expiring stale entries.
    pub fn tick(&mut self, now: u64) {
        self.clock = now;
        if self.ttl == 0 {
            return;
        }
        let ttl = self.ttl;
        self.entries.retain(|_, e| now.saturating_sub(e.inserted) < ttl);
    }

    pub fn put(&mut self, key: u64, image: RecordImage) {
        self.access += 1;
        let touched_at = self.access;
        let clock = self.clock;
        self.entries.insert(key, Entry { image, inserted: clock, touched_at });
        self.enforce_capacity();
    }

    pub fn get(&mut self, key: u64) -> Option<&RecordImage> {
        self.access += 1;
        let stamp = self.access;
        match self.entries.get_mut(&key) {
            Some(e) => {
                e.touched_at = stamp;
                self.hits += 1;
                Some(&e.image)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    pub fn remove(&mut self, key: u64) -> Option<RecordImage> {
        self.entries.remove(&key).map(|e| e.image)
    }

    fn enforce_capacity(&mut self) {
        while self.entries.len() > self.capacity {
            // Evict the least-recently-touched entry.
            let victim = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.touched_at)
                .map(|(&k, _)| k);
            match victim {
                Some(k) => {
                    self.entries.remove(&k);
                    self.evictions += 1;
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoded;

    fn img(v: u32) -> RecordImage {
        RecordImage { values: vec![Decoded::U32(v)] }
    }

    #[test]
    fn put_get_hit_miss() {
        let mut s = RecordStore::new(4, 0);
        s.put(1, img(10));
        assert!(s.get(1).is_some());
        assert!(s.get(2).is_none());
        let (hits, misses, _) = s.stats();
        assert_eq!((hits, misses), (1, 1));
    }

    #[test]
    fn ttl_expires() {
        let mut s = RecordStore::new(4, 100);
        s.tick(0);
        s.put(1, img(1));
        s.tick(200);
        assert!(s.get(1).is_none());
    }

    #[test]
    fn lru_eviction() {
        let mut s = RecordStore::new(2, 0);
        s.put(1, img(1));
        s.put(2, img(2));
        let _ = s.get(1); // touch 1
        s.put(3, img(3)); // evict 2 (LRU)
        assert!(s.get(2).is_none());
        assert!(s.get(1).is_some());
    }
}
