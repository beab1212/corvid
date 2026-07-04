//! Flow identity.
//!
//! A flow is keyed by the classic 5-tuple plus the logical flow id carried in
//! `FLOW_OPEN`/`DATA_RECORD`. Keys are cheap to hash and compare and are used
//! directly as map keys, so distinct flows never alias even under hash
//! collision.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub src: u32,
    pub dst: u32,
    pub flow_id: u32,
    pub sport: u16,
    pub dport: u16,
    pub proto: u8,
}

impl FlowKey {
    pub fn new(src: u32, dst: u32, flow_id: u32, sport: u16, dport: u16, proto: u8) -> FlowKey {
        FlowKey { src, dst, flow_id, sport, dport, proto }
    }

    /// A synthetic key derived only from a flow id, used by the fragment and
    /// stream paths where the 5-tuple is not meaningful.
    pub fn from_flow_id(flow_id: u32) -> FlowKey {
        FlowKey { src: 0, dst: 0, flow_id, sport: 0, dport: 0, proto: 0 }
    }

    /// A stable 64-bit digest, used for LRU ordering keys and logging.
    pub fn digest(&self) -> u64 {
        let mut h = crate::util::fnv1a(&self.src.to_be_bytes());
        h ^= crate::util::fnv1a(&self.dst.to_be_bytes()).rotate_left(17);
        h ^= (self.flow_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^= ((self.sport as u64) << 16) | (self.dport as u64);
        h ^= (self.proto as u64) << 48;
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_keys_distinct() {
        let a = FlowKey::new(1, 2, 3, 4, 5, 6);
        let b = FlowKey::new(1, 2, 3, 4, 5, 7);
        assert_ne!(a, b);
        assert_ne!(a.digest(), b.digest());
    }
}
