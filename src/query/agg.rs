//! Aggregate accumulators used by the query layer.

use crate::flow::FlowRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateKind {
    Count,
    SumOctets,
    SumPackets,
    MaxOctets,
    MinOctets,
    DistinctSources,
}

/// A running aggregate over the matched flow set.
#[derive(Debug, Clone)]
pub struct Aggregate {
    kind: AggregateKind,
    acc: u64,
    count: u64,
    seen: std::collections::HashSet<u32>,
    seeded: bool,
}

impl Aggregate {
    pub fn new(kind: AggregateKind) -> Aggregate {
        Aggregate { kind, acc: 0, count: 0, seen: Default::default(), seeded: false }
    }

    pub fn kind(&self) -> AggregateKind {
        self.kind
    }

    pub fn observe(&mut self, r: &FlowRecord) {
        self.count += 1;
        match self.kind {
            AggregateKind::Count => self.acc = self.count,
            AggregateKind::SumOctets => self.acc = self.acc.wrapping_add(r.octets),
            AggregateKind::SumPackets => self.acc = self.acc.wrapping_add(r.packets),
            AggregateKind::MaxOctets => {
                if !self.seeded || r.octets > self.acc {
                    self.acc = r.octets;
                }
            }
            AggregateKind::MinOctets => {
                if !self.seeded || r.octets < self.acc {
                    self.acc = r.octets;
                }
            }
            AggregateKind::DistinctSources => {
                self.seen.insert(r.key.src);
                self.acc = self.seen.len() as u64;
            }
        }
        self.seeded = true;
    }

    pub fn value(&self) -> u64 {
        self.acc
    }

    pub fn label(&self) -> &'static str {
        match self.kind {
            AggregateKind::Count => "count",
            AggregateKind::SumOctets => "sum_octets",
            AggregateKind::SumPackets => "sum_packets",
            AggregateKind::MaxOctets => "max_octets",
            AggregateKind::MinOctets => "min_octets",
            AggregateKind::DistinctSources => "distinct_sources",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowKey;

    #[test]
    fn min_max() {
        let mut mn = Aggregate::new(AggregateKind::MinOctets);
        let mut mx = Aggregate::new(AggregateKind::MaxOctets);
        for o in [30u64, 10, 90] {
            let mut r = FlowRecord::new(FlowKey::from_flow_id(1), 0);
            r.accumulate(o, 1, 0);
            mn.observe(&r);
            mx.observe(&r);
        }
        assert_eq!(mn.value(), 10);
        assert_eq!(mx.value(), 90);
    }
}
