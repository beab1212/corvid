//! Biflow pairing.
//!
//! A bidirectional conversation shows up as two unidirectional flows whose
//! keys are mirror images (src/dst and sport/dport swapped). The pairer folds
//! the reverse direction into a single biflow record so reports can show
//! request/response byte counts side by side.

use std::collections::HashMap;

use crate::flow::{FlowKey, FlowRecord};

/// A paired conversation.
#[derive(Debug, Clone)]
pub struct Biflow {
    pub key: FlowKey,
    pub fwd_octets: u64,
    pub rev_octets: u64,
    pub fwd_packets: u64,
    pub rev_packets: u64,
}

impl Biflow {
    pub fn ratio(&self) -> f64 {
        if self.rev_octets == 0 {
            f64::INFINITY
        } else {
            self.fwd_octets as f64 / self.rev_octets as f64
        }
    }

    pub fn total_octets(&self) -> u64 {
        self.fwd_octets.wrapping_add(self.rev_octets)
    }
}

fn reverse_key(k: &FlowKey) -> FlowKey {
    FlowKey {
        src: k.dst,
        dst: k.src,
        flow_id: k.flow_id,
        sport: k.dport,
        dport: k.sport,
        proto: k.proto,
    }
}

/// Choose a canonical orientation for a conversation so the two directions map
/// to the same slot: the endpoint with the lower (addr, port) is "forward".
fn canonical(k: &FlowKey) -> (FlowKey, bool) {
    let rev = reverse_key(k);
    if (k.src, k.sport) <= (k.dst, k.dport) {
        (*k, true)
    } else {
        (rev, false)
    }
}

#[derive(Debug, Default)]
pub struct BiflowPairer {
    table: HashMap<(u32, u32, u16, u16, u8), Biflow>,
}

impl BiflowPairer {
    pub fn new() -> BiflowPairer {
        BiflowPairer { table: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub fn observe(&mut self, rec: &FlowRecord) {
        let (canon, is_forward) = canonical(&rec.key);
        let slot = (canon.src, canon.dst, canon.sport, canon.dport, canon.proto);
        let entry = self.table.entry(slot).or_insert_with(|| Biflow {
            key: canon,
            fwd_octets: 0,
            rev_octets: 0,
            fwd_packets: 0,
            rev_packets: 0,
        });
        if is_forward {
            entry.fwd_octets = entry.fwd_octets.wrapping_add(rec.octets);
            entry.fwd_packets = entry.fwd_packets.wrapping_add(rec.packets);
        } else {
            entry.rev_octets = entry.rev_octets.wrapping_add(rec.octets);
            entry.rev_packets = entry.rev_packets.wrapping_add(rec.packets);
        }
    }

    pub fn biflows(&self) -> impl Iterator<Item = &Biflow> {
        self.table.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_mirror_flows() {
        let mut p = BiflowPairer::new();
        let mut fwd = FlowRecord::new(FlowKey::new(1, 2, 0, 100, 200, 6), 0);
        fwd.accumulate(1000, 10, 0);
        let mut rev = FlowRecord::new(FlowKey::new(2, 1, 0, 200, 100, 6), 0);
        rev.accumulate(500, 8, 0);
        p.observe(&fwd);
        p.observe(&rev);
        assert_eq!(p.len(), 1);
        let bf = p.biflows().next().unwrap();
        assert_eq!(bf.total_octets(), 1500);
    }
}
