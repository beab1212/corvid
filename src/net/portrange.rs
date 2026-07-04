//! Port-range sets for filter predicates like `port in 1024-2048,8080`.

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortRange {
    pub lo: u16,
    pub hi: u16,
}

impl PortRange {
    pub fn single(p: u16) -> PortRange {
        PortRange { lo: p, hi: p }
    }

    pub fn new(lo: u16, hi: u16) -> PortRange {
        if lo <= hi {
            PortRange { lo, hi }
        } else {
            PortRange { lo: hi, hi: lo }
        }
    }

    pub fn contains(&self, p: u16) -> bool {
        p >= self.lo && p <= self.hi
    }

    pub fn overlaps(&self, other: &PortRange) -> bool {
        self.lo <= other.hi && other.lo <= self.hi
    }

    pub fn width(&self) -> u32 {
        (self.hi as u32) - (self.lo as u32) + 1
    }
}

#[derive(Debug, Clone, Default)]
pub struct PortSet {
    ranges: Vec<PortRange>,
}

impl PortSet {
    pub fn new() -> PortSet {
        PortSet { ranges: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn add(&mut self, r: PortRange) {
        self.ranges.push(r);
        self.normalise();
    }

    pub fn contains(&self, p: u16) -> bool {
        self.ranges.iter().any(|r| r.contains(p))
    }

    /// Total number of ports covered.
    pub fn cardinality(&self) -> u32 {
        self.ranges.iter().map(|r| r.width()).sum()
    }

    fn normalise(&mut self) {
        if self.ranges.len() < 2 {
            return;
        }
        self.ranges.sort_by_key(|r| r.lo);
        let mut merged: Vec<PortRange> = Vec::with_capacity(self.ranges.len());
        for r in self.ranges.drain(..) {
            match merged.last_mut() {
                Some(last) if r.lo <= last.hi.saturating_add(1) => {
                    last.hi = last.hi.max(r.hi);
                }
                _ => merged.push(r),
            }
        }
        self.ranges = merged;
    }

    /// Parse `"80,443,1024-2048"`.
    pub fn parse(s: &str) -> Result<PortSet> {
        let mut set = PortSet::new();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let r = match part.split_once('-') {
                Some((a, b)) => PortRange::new(parse_port(a)?, parse_port(b)?),
                None => PortRange::single(parse_port(part)?),
            };
            set.add(r);
        }
        Ok(set)
    }
}

fn parse_port(s: &str) -> Result<u16> {
    s.trim().parse::<u16>().map_err(|_| Error::malformed("bad port"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_contains() {
        let set = PortSet::parse("80, 443, 1024-2048").unwrap();
        assert!(set.contains(80));
        assert!(set.contains(1500));
        assert!(!set.contains(79));
    }

    #[test]
    fn adjacent_ranges_merge() {
        let mut set = PortSet::new();
        set.add(PortRange::new(10, 20));
        set.add(PortRange::new(21, 30));
        assert_eq!(set.len(), 1);
        assert_eq!(set.cardinality(), 21);
    }
}
