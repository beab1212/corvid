//! A secondary index mapping a scalar field value to the record keys that carry
//! it. Supports point and range lookups for the query layer.

use std::collections::BTreeMap;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct FieldIndex {
    // Ordered so we can answer range queries.
    by_value: BTreeMap<u64, HashSet<u64>>,
    key_value: std::collections::HashMap<u64, u64>,
}

impl FieldIndex {
    pub fn new() -> FieldIndex {
        FieldIndex { by_value: BTreeMap::new(), key_value: Default::default() }
    }

    pub fn distinct_values(&self) -> usize {
        self.by_value.len()
    }

    /// Index `key` under `value`, replacing any previous value for `key`.
    pub fn insert(&mut self, key: u64, value: u64) {
        if let Some(&old) = self.key_value.get(&key) {
            if old == value {
                return;
            }
            if let Some(set) = self.by_value.get_mut(&old) {
                set.remove(&key);
                if set.is_empty() {
                    self.by_value.remove(&old);
                }
            }
        }
        self.by_value.entry(value).or_default().insert(key);
        self.key_value.insert(key, value);
    }

    pub fn remove(&mut self, key: u64) {
        if let Some(value) = self.key_value.remove(&key) {
            if let Some(set) = self.by_value.get_mut(&value) {
                set.remove(&key);
                if set.is_empty() {
                    self.by_value.remove(&value);
                }
            }
        }
    }

    /// All keys with exactly `value`.
    pub fn point(&self, value: u64) -> Vec<u64> {
        self.by_value.get(&value).map(|s| s.iter().copied().collect()).unwrap_or_default()
    }

    /// All keys whose value lies in `[lo, hi]`.
    pub fn range(&self, lo: u64, hi: u64) -> Vec<u64> {
        let mut out = Vec::new();
        for (_, set) in self.by_value.range(lo..=hi) {
            out.extend(set.iter().copied());
        }
        out.sort_unstable();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_and_range() {
        let mut idx = FieldIndex::new();
        idx.insert(1, 10);
        idx.insert(2, 10);
        idx.insert(3, 20);
        assert_eq!(idx.point(10).len(), 2);
        assert_eq!(idx.range(10, 20).len(), 3);
    }

    #[test]
    fn reindex_moves_key() {
        let mut idx = FieldIndex::new();
        idx.insert(1, 10);
        idx.insert(1, 20);
        assert!(idx.point(10).is_empty());
        assert_eq!(idx.point(20), vec![1]);
    }
}
