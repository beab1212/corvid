//! Top-N tracking of the heaviest keys by a cumulative weight.
//!
//! Used to surface "top talkers" — the source addresses responsible for the
//! most octets. A bounded min-heap keeps the current top N; lighter keys fall
//! out as heavier ones arrive.

use std::collections::HashMap;

pub struct TopN {
    n: usize,
    weights: HashMap<u64, u64>,
}

impl TopN {
    pub fn new(n: usize) -> TopN {
        TopN { n: n.max(1), weights: HashMap::new() }
    }

    /// Add `weight` to `key`'s running total.
    pub fn add(&mut self, key: u64, weight: u64) {
        let e = self.weights.entry(key).or_insert(0);
        *e = e.saturating_add(weight);
    }

    pub fn distinct_keys(&self) -> usize {
        self.weights.len()
    }

    /// Return the current top N as `(key, weight)` pairs, heaviest first.
    pub fn ranked(&self) -> Vec<(u64, u64)> {
        let mut all: Vec<(u64, u64)> = self.weights.iter().map(|(&k, &w)| (k, w)).collect();
        all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        all.truncate(self.n);
        all
    }

    /// Compact the map down to the current top `keep` keys to bound memory.
    pub fn compact(&mut self, keep: usize) {
        if self.weights.len() <= keep {
            return;
        }
        let ranked = {
            let mut all: Vec<(u64, u64)> = self.weights.iter().map(|(&k, &w)| (k, w)).collect();
            all.sort_by(|a, b| b.1.cmp(&a.1));
            all.truncate(keep);
            all
        };
        self.weights = ranked.into_iter().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_heaviest_first() {
        let mut t = TopN::new(2);
        t.add(1, 10);
        t.add(2, 30);
        t.add(3, 20);
        let r = t.ranked();
        assert_eq!(r[0].0, 2);
        assert_eq!(r[1].0, 3);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn compaction_keeps_top() {
        let mut t = TopN::new(2);
        for i in 0..100u64 {
            t.add(i, i);
        }
        t.compact(5);
        assert_eq!(t.distinct_keys(), 5);
        assert_eq!(t.ranked()[0].0, 99);
    }
}
