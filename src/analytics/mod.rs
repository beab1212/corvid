//! Aggregate analytics over flows: size histograms and top-talker tracking.

pub mod counter;
pub mod entropy;
pub mod histogram;
pub mod quantile;
pub mod rate;
pub mod timeseries;
pub mod topn;

pub use counter::{CounterRegistry, Meter};
pub use entropy::EntropyMeter;
pub use histogram::Histogram;
pub use quantile::QuantileSketch;
pub use rate::{EwmaRate, RateWindow};
pub use timeseries::{Bucket, TimeSeries};
pub use topn::TopN;

use crate::flow::FlowRecord;

/// A rolling summary combining a size histogram and a top-talkers table.
pub struct FlowSummary {
    pub octet_hist: Histogram,
    pub packet_hist: Histogram,
    pub top_sources: TopN,
    pub top_dests: TopN,
    flows_seen: u64,
}

impl FlowSummary {
    pub fn new(top_n: usize) -> FlowSummary {
        FlowSummary {
            octet_hist: Histogram::new(),
            packet_hist: Histogram::new(),
            top_sources: TopN::new(top_n),
            top_dests: TopN::new(top_n),
            flows_seen: 0,
        }
    }

    pub fn observe(&mut self, rec: &FlowRecord) {
        self.octet_hist.record(rec.octets);
        self.packet_hist.record(rec.packets);
        self.top_sources.add(rec.key.src as u64, rec.octets);
        self.top_dests.add(rec.key.dst as u64, rec.octets);
        self.flows_seen += 1;
    }

    pub fn flows_seen(&self) -> u64 {
        self.flows_seen
    }

    pub fn report(&self) -> String {
        let mut s = format!(
            "flows={} mean_octets={:.1} p99_octets={}\n",
            self.flows_seen,
            self.octet_hist.mean(),
            self.octet_hist.percentile(0.99),
        );
        s.push_str("top sources:\n");
        for (k, w) in self.top_sources.ranked() {
            s.push_str(&format!("  {k:#010x}: {w}\n"));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowKey;

    #[test]
    fn summary_observes() {
        let mut sum = FlowSummary::new(4);
        for i in 0..10u32 {
            let mut r = FlowRecord::new(FlowKey::new(i % 3, 0, i, 0, 0, 6), 0);
            r.accumulate((i as u64 + 1) * 100, 1, 0);
            sum.observe(&r);
        }
        assert_eq!(sum.flows_seen(), 10);
        assert!(!sum.report().is_empty());
    }
}
