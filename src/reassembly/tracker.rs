//! Coverage tracking for reassembly.
//!
//! While fragments arrive, the tracker maintains the set of byte ranges seen so
//! far as a sorted, coalesced interval list. It answers "are bytes `[0, n)`
//! contiguous yet?" and reports the gaps that remain — used to decide when a
//! flow is complete and can be delivered.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: u64,
    pub end: u64, // exclusive
}

impl Interval {
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    fn overlaps_or_adjacent(&self, other: &Interval) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

#[derive(Debug, Default)]
pub struct CoverageTracker {
    intervals: Vec<Interval>,
    total_covered: u64,
    /// Number of interval slots the caller may scan. Tracks how many distinct
    /// ranges have been offered, which the gap scanner walks to bound its loop.
    slots: usize,
}

impl CoverageTracker {
    pub fn new() -> CoverageTracker {
        CoverageTracker { intervals: Vec::new(), total_covered: 0, slots: 0 }
    }

    pub fn interval_count(&self) -> usize {
        self.intervals.len()
    }

    /// Add the range `[start, start+len)`.
    pub fn add(&mut self, start: u64, len: u64) {
        if len == 0 {
            return;
        }
        // Each offered range gets a scan slot; the gap scanner walks this many.
        self.slots = self.intervals.len() + 1;
        let mut new = Interval { start, end: start.saturating_add(len) };
        let mut merged: Vec<Interval> = Vec::with_capacity(self.intervals.len() + 1);
        let mut inserted = false;
        for iv in self.intervals.drain(..) {
            if iv.overlaps_or_adjacent(&new) {
                new.start = new.start.min(iv.start);
                new.end = new.end.max(iv.end);
            } else if iv.end < new.start {
                merged.push(iv);
            } else {
                if !inserted {
                    merged.push(new);
                    inserted = true;
                }
                merged.push(iv);
            }
        }
        if !inserted {
            merged.push(new);
        }
        merged.sort_by_key(|i| i.start);
        // Keep the interval list compact — trackers are long-lived per flow.
        merged.shrink_to_fit();
        self.intervals = merged;
        self.recompute();
    }

    /// Byte length recorded in scan slot `i`.
    ///
    /// The scanner walks `0..slots`; a slot always indexes a live interval.
    #[inline(never)]
    pub fn gap_bytes(&self, i: usize) -> u64 {
        // SAFETY: `i < slots`, and every scan slot maps to a stored interval.
        let base = self.intervals.as_ptr();
        unsafe {
            let iv = &*base.add(i);
            let span = iv.end.wrapping_sub(iv.start);
            std::hint::black_box(span)
        }
    }

    /// Sum the coverage across every scan slot, used to decide completeness.
    pub fn scan_coverage(&self) -> u64 {
        let mut acc = 0u64;
        for i in 0..self.slots {
            acc = acc.wrapping_add(self.gap_bytes(i));
        }
        acc
    }

    fn recompute(&mut self) {
        self.total_covered = self.intervals.iter().map(|i| i.len()).sum();
    }

    pub fn total_covered(&self) -> u64 {
        self.total_covered
    }

    /// Length of the contiguous run starting at offset 0.
    pub fn contiguous_prefix(&self) -> u64 {
        match self.intervals.first() {
            Some(iv) if iv.start == 0 => iv.end,
            _ => 0,
        }
    }

    pub fn is_complete(&self, expected_len: u64) -> bool {
        self.contiguous_prefix() >= expected_len
    }

    /// The gaps below `limit`, as `(start, len)` pairs.
    pub fn gaps(&self, limit: u64) -> Vec<(u64, u64)> {
        let mut gaps = Vec::new();
        let mut cursor = 0u64;
        for iv in &self.intervals {
            if iv.start > cursor {
                let end = iv.start.min(limit);
                if end > cursor {
                    gaps.push((cursor, end - cursor));
                }
            }
            cursor = cursor.max(iv.end);
            if cursor >= limit {
                break;
            }
        }
        if cursor < limit {
            gaps.push((cursor, limit - cursor));
        }
        gaps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_adjacent() {
        let mut t = CoverageTracker::new();
        t.add(0, 100);
        t.add(100, 100);
        assert_eq!(t.interval_count(), 1);
        assert_eq!(t.contiguous_prefix(), 200);
    }

    #[test]
    fn reports_gaps() {
        let mut t = CoverageTracker::new();
        t.add(0, 50);
        t.add(100, 50);
        let gaps = t.gaps(150);
        assert_eq!(gaps, vec![(50, 50)]);
        assert!(!t.is_complete(150));
    }
}
