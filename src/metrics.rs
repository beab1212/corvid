//! Lightweight counters for observability.
//!
//! The broker keeps a flat set of monotonically-increasing counters plus a few
//! gauges. There is deliberately no locking: a `Metrics` belongs to exactly one
//! session/engine and is updated from the single thread driving it.

#[derive(Debug, Clone)]
pub struct Metrics {
    pub streams_parsed: u64,
    pub messages_seen: u64,
    pub messages_by_type: [u64; 64],
    pub bytes_ingested: u64,
    pub decode_errors: u64,
    pub records_decoded: u64,
    pub fragments_seen: u64,
    pub flows_opened: u64,
    pub flows_closed: u64,
    pub templates_defined: u64,
    pub rows_packed: u64,
    pub codec_bytes_in: u64,
    pub codec_bytes_out: u64,
    // Gauges — current, not cumulative.
    pub live_flows: i64,
    pub live_channels: i64,
    pub arena_high_water: usize,
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            streams_parsed: 0,
            messages_seen: 0,
            messages_by_type: [0; 64],
            bytes_ingested: 0,
            decode_errors: 0,
            records_decoded: 0,
            fragments_seen: 0,
            flows_opened: 0,
            flows_closed: 0,
            templates_defined: 0,
            rows_packed: 0,
            codec_bytes_in: 0,
            codec_bytes_out: 0,
            live_flows: 0,
            live_channels: 0,
            arena_high_water: 0,
        }
    }
}

impl Metrics {
    pub fn new() -> Metrics {
        Metrics::default()
    }

    pub fn note_stream(&mut self, bytes: usize) {
        self.streams_parsed += 1;
        self.bytes_ingested += bytes as u64;
    }

    pub fn note_message(&mut self, ty: u8) {
        self.messages_seen += 1;
        let slot = (ty as usize) & 63;
        self.messages_by_type[slot] = self.messages_by_type[slot].saturating_add(1);
    }

    pub fn note_error(&mut self) {
        self.decode_errors += 1;
    }

    pub fn note_record(&mut self) {
        self.records_decoded += 1;
    }

    pub fn note_codec(&mut self, bytes_in: usize, bytes_out: usize) {
        self.codec_bytes_in += bytes_in as u64;
        self.codec_bytes_out += bytes_out as u64;
    }

    pub fn open_flow(&mut self) {
        self.flows_opened += 1;
        self.live_flows += 1;
    }

    pub fn close_flow(&mut self) {
        self.flows_closed += 1;
        self.live_flows -= 1;
    }

    /// A compact one-line summary for logs.
    pub fn summary(&self) -> String {
        format!(
            "streams={} msgs={} recs={} errs={} flows={}/{} live_flows={}",
            self.streams_parsed,
            self.messages_seen,
            self.records_decoded,
            self.decode_errors,
            self.flows_opened,
            self.flows_closed,
            self.live_flows,
        )
    }

    /// Approximate compression ratio, or 1.0 if nothing has been through a
    /// codec yet.
    pub fn codec_ratio(&self) -> f64 {
        if self.codec_bytes_out == 0 {
            1.0
        } else {
            self.codec_bytes_in as f64 / self.codec_bytes_out as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_move() {
        let mut m = Metrics::new();
        m.note_stream(100);
        m.note_message(0x06);
        m.note_record();
        m.open_flow();
        m.close_flow();
        assert_eq!(m.streams_parsed, 1);
        assert_eq!(m.records_decoded, 1);
        assert_eq!(m.live_flows, 0);
        assert!(m.summary().contains("recs=1"));
    }
}
