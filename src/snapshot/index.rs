//! An in-memory index over the flows contained in a snapshot.
//!
//! Decoding a large snapshot just to answer "does flow X exist" or "what are
//! the ten heaviest flows" is wasteful. The index decodes once and offers cheap
//! lookups and top-k over the result.

use crate::error::Result;
use crate::flow::FlowRecord;
use crate::snapshot::format::decode;

pub struct SnapshotIndex {
    records: Vec<FlowRecord>,
}

impl SnapshotIndex {
    pub fn build(snapshot: &[u8]) -> Result<SnapshotIndex> {
        Ok(SnapshotIndex { records: decode(snapshot)? })
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn records(&self) -> &[FlowRecord] {
        &self.records
    }

    /// Find a flow by its logical flow id.
    pub fn find_flow(&self, flow_id: u32) -> Option<&FlowRecord> {
        self.records.iter().find(|r| r.key.flow_id == flow_id)
    }

    /// Total octets across all flows.
    pub fn total_octets(&self) -> u64 {
        self.records.iter().fold(0u64, |a, r| a.wrapping_add(r.octets))
    }

    /// The `k` heaviest flows by octet count, descending.
    pub fn heaviest(&self, k: usize) -> Vec<&FlowRecord> {
        let mut refs: Vec<&FlowRecord> = self.records.iter().collect();
        refs.sort_by(|a, b| b.octets.cmp(&a.octets));
        refs.truncate(k);
        refs
    }

    /// Flows whose last activity was at or before `cutoff_ms`.
    pub fn idle_before(&self, cutoff_ms: u64) -> Vec<&FlowRecord> {
        self.records.iter().filter(|r| r.last_ms <= cutoff_ms).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowKey;
    use crate::snapshot::format::encode;

    #[test]
    fn build_and_query() {
        let mut a = FlowRecord::new(FlowKey::from_flow_id(1), 0);
        a.accumulate(100, 1, 10);
        let mut b = FlowRecord::new(FlowKey::from_flow_id(2), 0);
        b.accumulate(900, 1, 20);
        let snap = encode(&[a, b]);
        let idx = SnapshotIndex::build(&snap).unwrap();
        assert_eq!(idx.len(), 2);
        assert_eq!(idx.total_octets(), 1000);
        assert_eq!(idx.heaviest(1)[0].key.flow_id, 2);
        assert!(idx.find_flow(1).is_some());
    }
}
