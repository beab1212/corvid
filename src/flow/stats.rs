//! Derived per-flow statistics.
//!
//! [`FlowRecord`] stores raw counters; these helpers turn them into the rates
//! and ratios operators actually look at — bitrate, packet rate, mean packet
//! size and a crude burstiness score. All arithmetic saturates so a malformed
//! record with a zero or inverted duration cannot panic.

use crate::flow::record::FlowRecord;

#[derive(Debug, Clone, Copy)]
pub struct FlowStats {
    pub duration_ms: u64,
    pub bits_per_sec: u64,
    pub packets_per_sec: u64,
    pub mean_packet_bytes: u64,
    pub records: u64,
}

impl FlowStats {
    pub fn of(rec: &FlowRecord) -> FlowStats {
        let duration_ms = rec.last_ms.saturating_sub(rec.first_ms);
        let secs = (duration_ms.max(1)) as f64 / 1000.0;
        let bits_per_sec = ((rec.octets.saturating_mul(8)) as f64 / secs) as u64;
        let packets_per_sec = (rec.packets as f64 / secs) as u64;
        let mean_packet_bytes = if rec.packets == 0 {
            0
        } else {
            rec.octets / rec.packets
        };
        FlowStats {
            duration_ms,
            bits_per_sec,
            packets_per_sec,
            mean_packet_bytes,
            records: rec.records,
        }
    }

    /// A 0-100 burstiness score: high when many bytes arrived in a short window.
    pub fn burstiness(&self) -> u8 {
        if self.duration_ms == 0 {
            return 100;
        }
        let per_record = self.bits_per_sec / self.records.max(1);
        let score = (per_record / 1024).min(100);
        score as u8
    }

    pub fn is_elephant(&self, bps_threshold: u64) -> bool {
        self.bits_per_sec >= bps_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowKey;

    fn rec(octets: u64, packets: u64, dur: u64) -> FlowRecord {
        let mut r = FlowRecord::new(FlowKey::from_flow_id(1), 0);
        r.accumulate(octets, packets, dur);
        r.first_ms = 0;
        r.last_ms = dur;
        r
    }

    #[test]
    fn rates_computed() {
        let r = rec(1000, 10, 1000);
        let s = FlowStats::of(&r);
        assert_eq!(s.duration_ms, 1000);
        assert_eq!(s.mean_packet_bytes, 100);
        assert_eq!(s.bits_per_sec, 8000);
    }

    #[test]
    fn zero_packets_safe() {
        let mut r = rec(0, 0, 0);
        r.last_ms = 0;
        let s = FlowStats::of(&r);
        assert_eq!(s.mean_packet_bytes, 0);
        assert_eq!(s.burstiness(), 100);
    }
}
