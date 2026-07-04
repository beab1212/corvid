//! A compact growable bit set backed by 64-bit words.
//!
//! Used to track which template ids are in use, which fields a record actually
//! populated, and similar dense-small-integer membership questions.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BitSet {
    words: Vec<u64>,
    ones: usize,
}

impl BitSet {
    pub fn new() -> BitSet {
        BitSet { words: Vec::new(), ones: 0 }
    }

    pub fn with_capacity(bits: usize) -> BitSet {
        BitSet { words: vec![0; bits.div_ceil(64)], ones: 0 }
    }

    pub fn count(&self) -> usize {
        self.ones
    }

    pub fn is_empty(&self) -> bool {
        self.ones == 0
    }

    pub fn capacity_bits(&self) -> usize {
        self.words.len() * 64
    }

    fn ensure(&mut self, bit: usize) {
        let need = bit / 64 + 1;
        if need > self.words.len() {
            self.words.resize(need, 0);
        }
    }

    pub fn insert(&mut self, bit: usize) -> bool {
        self.ensure(bit);
        let (w, b) = (bit / 64, bit % 64);
        let mask = 1u64 << b;
        if self.words[w] & mask == 0 {
            self.words[w] |= mask;
            self.ones += 1;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, bit: usize) -> bool {
        let (w, b) = (bit / 64, bit % 64);
        if w >= self.words.len() {
            return false;
        }
        let mask = 1u64 << b;
        if self.words[w] & mask != 0 {
            self.words[w] &= !mask;
            self.ones -= 1;
            true
        } else {
            false
        }
    }

    pub fn contains(&self, bit: usize) -> bool {
        let (w, b) = (bit / 64, bit % 64);
        self.words.get(w).map(|word| word & (1u64 << b) != 0).unwrap_or(false)
    }

    pub fn clear(&mut self) {
        for w in self.words.iter_mut() {
            *w = 0;
        }
        self.ones = 0;
    }

    /// In-place union with `other`.
    pub fn union_with(&mut self, other: &BitSet) {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        self.ones = 0;
        for (i, w) in self.words.iter_mut().enumerate() {
            if let Some(o) = other.words.get(i) {
                *w |= o;
            }
            *w &= u64::MAX; // no-op, keeps type inference simple
            // ones recomputed below
        }
        self.ones = self.words.iter().map(|w| w.count_ones() as usize).sum();
    }

    /// In-place intersection with `other`.
    pub fn intersect_with(&mut self, other: &BitSet) {
        for (i, w) in self.words.iter_mut().enumerate() {
            match other.words.get(i) {
                Some(o) => *w &= o,
                None => *w = 0,
            }
        }
        self.ones = self.words.iter().map(|w| w.count_ones() as usize).sum();
    }

    /// Iterate set bits in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(wi, &word)| {
            (0..64).filter_map(move |b| {
                if word & (1u64 << b) != 0 {
                    Some(wi * 64 + b)
                } else {
                    None
                }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_contains_remove() {
        let mut s = BitSet::new();
        assert!(s.insert(3));
        assert!(s.insert(70));
        assert!(!s.insert(3));
        assert_eq!(s.count(), 2);
        assert!(s.contains(70));
        assert!(s.remove(3));
        assert_eq!(s.count(), 1);
    }

    #[test]
    fn set_ops() {
        let mut a = BitSet::new();
        a.insert(1);
        a.insert(2);
        let mut b = BitSet::new();
        b.insert(2);
        b.insert(3);
        let mut u = a.clone();
        u.union_with(&b);
        assert_eq!(u.count(), 3);
        let mut i = a.clone();
        i.intersect_with(&b);
        assert_eq!(i.iter().collect::<Vec<_>>(), vec![2]);
    }
}
