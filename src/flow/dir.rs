//! Flow direction inference and key canonicalisation.
//!
//! Unidirectional flow records carry a source and destination; to pair the two
//! halves of a conversation (see [`crate::flow::biflow`]) we need a canonical
//! ordering that maps both directions to the same key. The convention: the
//! endpoint with the smaller `(addr, port)` tuple is the "initiator".

use crate::flow::key::FlowKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}

/// The canonical (initiator-first) form of a key plus the direction the
/// original key represented relative to that canonical form.
#[derive(Debug, Clone, Copy)]
pub struct Canonical {
    pub key: FlowKey,
    pub direction: Direction,
}

fn endpoint_le(a_addr: u32, a_port: u16, b_addr: u32, b_port: u16) -> bool {
    (a_addr, a_port) <= (b_addr, b_port)
}

/// Canonicalise `key` so both directions of a conversation agree.
pub fn canonicalize(key: &FlowKey) -> Canonical {
    if endpoint_le(key.src, key.sport, key.dst, key.dport) {
        Canonical { key: *key, direction: Direction::Forward }
    } else {
        let flipped = FlowKey {
            src: key.dst,
            dst: key.src,
            sport: key.dport,
            dport: key.sport,
            flow_id: key.flow_id,
            proto: key.proto,
        };
        Canonical { key: flipped, direction: Direction::Reverse }
    }
}

/// Whether two keys are the two directions of one conversation.
pub fn same_conversation(a: &FlowKey, b: &FlowKey) -> bool {
    canonicalize(a).key == canonicalize(b).key
}

/// A heuristic guess of which endpoint initiated, based on the well-known port
/// convention: the side using the *lower* port is usually the server.
pub fn likely_server_is_dst(key: &FlowKey) -> bool {
    key.dport < key.sport
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_share_canonical() {
        let fwd = FlowKey::new(0x0a00_0001, 0x0a00_0002, 1, 1000, 80, 6);
        let rev = FlowKey::new(0x0a00_0002, 0x0a00_0001, 1, 80, 1000, 6);
        assert!(same_conversation(&fwd, &rev));
        assert_eq!(canonicalize(&fwd).direction, Direction::Forward);
        assert_eq!(canonicalize(&rev).direction, Direction::Reverse);
    }

    #[test]
    fn server_heuristic() {
        let k = FlowKey::new(1, 2, 0, 40000, 443, 6);
        assert!(likely_server_is_dst(&k));
    }
}
