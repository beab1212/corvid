//! Overlap-resolution policy for fragment reassembly.
//!
//! When two fragments cover overlapping byte ranges with differing content,
//! different stacks resolve the conflict differently (the classic "TCP overlap"
//! ambiguity). Isolating the choice here keeps the reassembly engine simple and
//! makes the behaviour auditable.

/// A half-open byte range `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

impl Span {
    pub fn new(start: u64, len: u64) -> Span {
        Span { start, end: start.saturating_add(len) }
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    pub fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn intersection(&self, other: &Span) -> Option<Span> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        if start < end {
            Some(Span { start, end })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlapPolicy {
    /// The first fragment to arrive for a byte wins (BSD-style).
    FirstWins,
    /// The most recent fragment wins (Windows-style).
    LastWins,
    /// Overlapping-but-inconsistent fragments are rejected outright.
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Accept the new bytes over `[start,end)`.
    TakeNew(Span),
    /// Keep the existing bytes; drop the new fragment's overlap.
    KeepOld,
    /// Reject the fragment as a conflict.
    Conflict,
}

impl OverlapPolicy {
    /// Decide how to merge `incoming` given the already-covered `existing`.
    pub fn resolve(self, existing: &Span, incoming: &Span, consistent: bool) -> Resolution {
        if !existing.overlaps(incoming) {
            return Resolution::TakeNew(*incoming);
        }
        if consistent {
            // Identical bytes: keep whichever, take only the newly-covered tail.
            return match incoming.end > existing.end {
                true => Resolution::TakeNew(Span { start: existing.end, end: incoming.end }),
                false => Resolution::KeepOld,
            };
        }
        match self {
            OverlapPolicy::FirstWins => Resolution::KeepOld,
            OverlapPolicy::LastWins => Resolution::TakeNew(*incoming),
            OverlapPolicy::Strict => Resolution::Conflict,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_takes_new() {
        let a = Span::new(0, 10);
        let b = Span::new(10, 5);
        assert_eq!(OverlapPolicy::Strict.resolve(&a, &b, false), Resolution::TakeNew(b));
    }

    #[test]
    fn strict_rejects_inconsistent_overlap() {
        let a = Span::new(0, 10);
        let b = Span::new(5, 10);
        assert_eq!(OverlapPolicy::Strict.resolve(&a, &b, false), Resolution::Conflict);
    }

    #[test]
    fn last_wins_takes_new() {
        let a = Span::new(0, 10);
        let b = Span::new(5, 10);
        assert_eq!(OverlapPolicy::LastWins.resolve(&a, &b, false), Resolution::TakeNew(b));
    }

    #[test]
    fn intersection_math() {
        let a = Span::new(0, 10);
        let b = Span::new(4, 10);
        assert_eq!(a.intersection(&b), Some(Span { start: 4, end: 10 }));
    }
}
