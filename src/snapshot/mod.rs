//! Persisting and restoring flow-table state.

pub mod format;
pub mod index;

pub use format::{decode, encode};
pub use index::SnapshotIndex;

use crate::analytics::FlowSummary;
use crate::error::Result;
use crate::flow::FlowRecord;

/// Replay a snapshot into an analytics summary, reconstructing the aggregate
/// picture without needing the original stream.
pub fn replay_into_summary(snapshot: &[u8], top_n: usize) -> Result<FlowSummary> {
    let records = decode(snapshot)?;
    let mut summary = FlowSummary::new(top_n);
    for rec in &records {
        summary.observe(rec);
    }
    Ok(summary)
}

/// Merge two snapshots, summing counters for flows that share a key.
pub fn merge(a: &[u8], b: &[u8]) -> Result<Vec<u8>> {
    use std::collections::HashMap;
    let mut acc: HashMap<_, FlowRecord> = HashMap::new();
    for rec in decode(a)?.into_iter().chain(decode(b)?) {
        acc.entry((rec.key.src, rec.key.dst, rec.key.flow_id))
            .and_modify(|e| {
                e.octets = e.octets.wrapping_add(rec.octets);
                e.packets = e.packets.wrapping_add(rec.packets);
                e.records = e.records.wrapping_add(rec.records);
                e.last_ms = e.last_ms.max(rec.last_ms);
            })
            .or_insert(rec);
    }
    let merged: Vec<FlowRecord> = acc.into_values().collect();
    Ok(encode(&merged))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowKey;

    #[test]
    fn merge_sums_counters() {
        let mut r = FlowRecord::new(FlowKey::new(1, 2, 3, 0, 0, 6), 0);
        r.accumulate(100, 1, 0);
        let a = encode(&[r]);
        let mut r2 = FlowRecord::new(FlowKey::new(1, 2, 3, 0, 0, 6), 0);
        r2.accumulate(50, 1, 0);
        let b = encode(&[r2]);
        let merged = decode(&merge(&a, &b).unwrap()).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].octets, 150);
    }

    #[test]
    fn replay_builds_summary() {
        let mut r = FlowRecord::new(FlowKey::from_flow_id(1), 0);
        r.accumulate(1000, 5, 0);
        let snap = encode(&[r]);
        let summary = replay_into_summary(&snap, 4).unwrap();
        assert_eq!(summary.flows_seen(), 1);
    }
}
