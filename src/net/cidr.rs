//! IPv4 CIDR prefixes and membership tests, used by filter address matching.

use crate::error::{Error, Result};
use crate::net::addr::{fmt_ipv4, parse_ipv4};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr4 {
    base: u32,
    prefix: u8,
}

impl Cidr4 {
    pub fn new(addr: u32, prefix: u8) -> Result<Cidr4> {
        if prefix > 32 {
            return Err(Error::malformed("cidr: prefix > 32"));
        }
        let mask = Self::mask(prefix);
        Ok(Cidr4 { base: addr & mask, prefix })
    }

    fn mask(prefix: u8) -> u32 {
        if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix as u32)
        }
    }

    /// Parse `a.b.c.d/len`.
    pub fn parse(s: &str) -> Result<Cidr4> {
        let (addr_s, prefix_s) = s.split_once('/').ok_or_else(|| Error::malformed("cidr: missing /"))?;
        let addr = parse_ipv4(addr_s.trim())?;
        let prefix: u8 = prefix_s.trim().parse().map_err(|_| Error::malformed("cidr: bad prefix"))?;
        Cidr4::new(addr, prefix)
    }

    pub fn contains(&self, addr: u32) -> bool {
        let mask = Self::mask(self.prefix);
        (addr & mask) == self.base
    }

    pub fn prefix(&self) -> u8 {
        self.prefix
    }

    pub fn network(&self) -> u32 {
        self.base
    }

    pub fn broadcast(&self) -> u32 {
        self.base | !Self::mask(self.prefix)
    }

    pub fn to_string_lossy(&self) -> String {
        format!("{}/{}", fmt_ipv4(self.base), self.prefix)
    }
}

/// A set of prefixes with a longest-prefix-match lookup.
#[derive(Debug, Default)]
pub struct PrefixSet {
    entries: Vec<Cidr4>,
}

impl PrefixSet {
    pub fn new() -> PrefixSet {
        PrefixSet { entries: Vec::new() }
    }

    pub fn insert(&mut self, cidr: Cidr4) {
        self.entries.push(cidr);
        // Keep longest prefixes first so `matches` finds the most specific.
        self.entries.sort_by(|a, b| b.prefix.cmp(&a.prefix));
    }

    pub fn matches(&self, addr: u32) -> Option<Cidr4> {
        self.entries.iter().copied().find(|c| c.contains(addr))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership() {
        let c = Cidr4::parse("10.0.0.0/8").unwrap();
        assert!(c.contains(parse_ipv4("10.1.2.3").unwrap()));
        assert!(!c.contains(parse_ipv4("11.0.0.1").unwrap()));
    }

    #[test]
    fn longest_prefix() {
        let mut set = PrefixSet::new();
        set.insert(Cidr4::parse("10.0.0.0/8").unwrap());
        set.insert(Cidr4::parse("10.1.0.0/16").unwrap());
        let m = set.matches(parse_ipv4("10.1.2.3").unwrap()).unwrap();
        assert_eq!(m.prefix(), 16);
    }
}
