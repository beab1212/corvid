//! A small query layer over collected flow records.
//!
//! A [`Query`] pairs a compiled [`Filter`] with a projection (which fields to
//! emit), an ordering and a row limit. Running it over a slice of
//! [`FlowRecord`]s yields an ordered [`ResultSet`] plus roll-up aggregates. This
//! is what the CLI's `query` verb and the pipeline's reporting path use.

pub mod agg;

pub use agg::{Aggregate, AggregateKind};

use crate::filter::{Filter, FlowView};
use crate::flow::FlowRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Octets,
    Packets,
    Records,
    Duration,
    FlowId,
}

impl SortKey {
    pub fn parse(s: &str) -> Option<SortKey> {
        Some(match s {
            "octets" => SortKey::Octets,
            "packets" => SortKey::Packets,
            "records" => SortKey::Records,
            "duration" => SortKey::Duration,
            "flow" | "flow_id" => SortKey::FlowId,
            _ => return None,
        })
    }

    fn extract(self, r: &FlowRecord) -> u64 {
        match self {
            SortKey::Octets => r.octets,
            SortKey::Packets => r.packets,
            SortKey::Records => r.records,
            SortKey::Duration => r.last_ms.saturating_sub(r.first_ms),
            SortKey::FlowId => r.key.flow_id as u64,
        }
    }
}

pub struct Query {
    filter: Option<Filter>,
    sort: SortKey,
    descending: bool,
    limit: usize,
    aggregates: Vec<AggregateKind>,
}

impl Query {
    pub fn new() -> Query {
        Query {
            filter: None,
            sort: SortKey::Octets,
            descending: true,
            limit: usize::MAX,
            aggregates: Vec::new(),
        }
    }

    pub fn where_clause(mut self, expr: &str) -> crate::error::Result<Query> {
        self.filter = Some(Filter::compile(expr)?);
        Ok(self)
    }

    pub fn order_by(mut self, key: SortKey, descending: bool) -> Query {
        self.sort = key;
        self.descending = descending;
        self
    }

    pub fn limit(mut self, n: usize) -> Query {
        self.limit = n;
        self
    }

    pub fn aggregate(mut self, kind: AggregateKind) -> Query {
        self.aggregates.push(kind);
        self
    }

    /// Execute against `flows`.
    pub fn run(&self, flows: &[FlowRecord]) -> ResultSet {
        let mut matched: Vec<FlowRecord> = Vec::new();
        let mut aggs: Vec<Aggregate> =
            self.aggregates.iter().map(|k| Aggregate::new(*k)).collect();

        for r in flows {
            let keep = match &self.filter {
                Some(f) => f.matches(&FlowView::new(r)),
                None => true,
            };
            if !keep {
                continue;
            }
            for a in aggs.iter_mut() {
                a.observe(r);
            }
            matched.push(*r);
        }

        let key = self.sort;
        matched.sort_by(|a, b| key.extract(a).cmp(&key.extract(b)));
        if self.descending {
            matched.reverse();
        }
        let scanned = flows.len();
        let total_matched = matched.len();
        matched.truncate(self.limit);

        ResultSet { rows: matched, aggregates: aggs, scanned, total_matched }
    }
}

impl Default for Query {
    fn default() -> Self {
        Query::new()
    }
}

pub struct ResultSet {
    pub rows: Vec<FlowRecord>,
    pub aggregates: Vec<Aggregate>,
    pub scanned: usize,
    pub total_matched: usize,
}

impl ResultSet {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn aggregate(&self, kind: AggregateKind) -> Option<&Aggregate> {
        self.aggregates.iter().find(|a| a.kind() == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowKey;

    fn flow(id: u32, octets: u64) -> FlowRecord {
        let mut r = FlowRecord::new(FlowKey::from_flow_id(id), 0);
        r.accumulate(octets, 1, 100);
        r
    }

    #[test]
    fn filter_sort_limit() {
        let flows = vec![flow(1, 50), flow(2, 500), flow(3, 150)];
        let q = Query::new()
            .where_clause("octets >= 100")
            .unwrap()
            .order_by(SortKey::Octets, true)
            .limit(1);
        let rs = q.run(&flows);
        assert_eq!(rs.total_matched, 2);
        assert_eq!(rs.rows.len(), 1);
        assert_eq!(rs.rows[0].key.flow_id, 2);
    }

    #[test]
    fn aggregate_sum() {
        let flows = vec![flow(1, 50), flow(2, 500)];
        let q = Query::new().aggregate(AggregateKind::SumOctets);
        let rs = q.run(&flows);
        assert_eq!(rs.aggregate(AggregateKind::SumOctets).unwrap().value(), 550);
    }
}
