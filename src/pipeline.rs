//! A batteries-included ingestion pipeline.
//!
//! Wires a [`Broker`] to an [`analytics::FlowSummary`] and an optional filter,
//! so a caller can feed capture files or raw streams and get an aggregate
//! report out. This is what the `corvidctl run` subcommand drives.

use crate::analytics::FlowSummary;
use crate::config::Config;
use crate::error::Result;
use crate::export::{Format, FlowWriter};
use crate::filter::Filter;
use crate::flow::FlowRecord;
use crate::io::CaptureReader;
use crate::session::Broker;

pub struct Pipeline {
    broker: Broker,
    summary: FlowSummary,
    filter: Option<Filter>,
    format: Format,
    writer: FlowWriter,
    streams_in: u64,
    matched: u64,
}

impl Pipeline {
    pub fn new(cfg: Config, format: Format) -> Pipeline {
        Pipeline {
            broker: Broker::new(cfg),
            summary: FlowSummary::new(16),
            filter: None,
            format,
            writer: FlowWriter::new(format),
            streams_in: 0,
            matched: 0,
        }
    }

    pub fn with_filter(mut self, expr: &str) -> Result<Pipeline> {
        self.filter = Some(Filter::compile(expr)?);
        Ok(self)
    }

    pub fn streams_in(&self) -> u64 {
        self.streams_in
    }

    pub fn matched(&self) -> u64 {
        self.matched
    }

    /// Feed a single CVWP stream tagged with a transport id.
    pub fn feed_stream(&mut self, transport_id: u32, stream: &[u8]) -> Result<()> {
        self.streams_in += 1;
        self.broker.route(transport_id, stream)
    }

    /// Feed an entire capture file.
    pub fn feed_capture(&mut self, data: &[u8]) -> Result<()> {
        let reader = CaptureReader::open(data)?;
        let mut tid = 0u32;
        for rec in reader.read_all()? {
            tid = tid.wrapping_add(1);
            self.streams_in += 1;
            // A malformed stream inside a capture is not fatal to the whole
            // capture; skip it and keep going.
            let _ = self.broker.route(tid, &rec.stream);
        }
        Ok(())
    }

    /// Record a flow into the summary and, if it matches the filter, the output.
    pub fn observe(&mut self, rec: &FlowRecord) {
        self.summary.observe(rec);
        let keep = match &self.filter {
            Some(f) => f.matches(&crate::filter::FlowView::new(rec)),
            None => true,
        };
        if keep {
            self.matched += 1;
            self.writer.push(rec);
        }
    }

    pub fn report(&mut self) -> String {
        let agg = self.broker.aggregate_metrics().summary();
        format!("{}\n{}", agg, self.summary.report())
    }

    pub fn into_output(self) -> String {
        self.writer.finish()
    }

    pub fn format(&self) -> Format {
        self.format
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowKey;

    #[test]
    fn observe_respects_filter() {
        let mut p = Pipeline::new(Config::compact(), Format::Csv)
            .with_filter("octets > 100")
            .unwrap();
        let mut small = FlowRecord::new(FlowKey::from_flow_id(1), 0);
        small.accumulate(50, 1, 0);
        let mut big = FlowRecord::new(FlowKey::from_flow_id(2), 0);
        big.accumulate(500, 1, 0);
        p.observe(&small);
        p.observe(&big);
        assert_eq!(p.matched(), 1);
    }
}
