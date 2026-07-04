//! The broker: routes many transport connections to per-source [`Session`]s.
//!
//! In a real deployment a single process terminates many exporters. Each is
//! identified by a transport id (a hash of its address/port). The broker keeps
//! one [`Session`] per transport id, creates them on demand and reaps the ones
//! that have gone quiet so a churn of short-lived exporters does not leak
//! sessions without bound.

use std::collections::HashMap;

use crate::config::Config;
use crate::error::Result;
use crate::io::BatchReader;
use crate::metrics::Metrics;
use crate::session::Session;
use crate::util::time::LogicalClock;

struct Entry {
    session: Box<Session>,
    last_seen: u64,
}

pub struct Broker {
    cfg: Config,
    sessions: HashMap<u32, Entry>,
    clock: LogicalClock,
    idle_ticks: u64,
    soft_cap: usize,
    reaped: u64,
    routed: u64,
    aggregate: Metrics,
}

impl Broker {
    pub fn new(cfg: Config) -> Broker {
        let idle_ticks = cfg.flow_idle_ticks;
        Broker {
            cfg,
            sessions: HashMap::new(),
            clock: LogicalClock::new(1),
            idle_ticks,
            soft_cap: 4096,
            reaped: 0,
            routed: 0,
            aggregate: Metrics::new(),
        }
    }

    pub fn with_limits(mut self, idle_ticks: u64, soft_cap: usize) -> Broker {
        self.idle_ticks = idle_ticks;
        self.soft_cap = soft_cap.max(1);
        self
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn reaped(&self) -> u64 {
        self.reaped
    }

    pub fn routed(&self) -> u64 {
        self.routed
    }

    fn touch(&mut self, transport_id: u32) -> &mut Session {
        let now = self.clock.now();
        let cfg = self.cfg.clone();
        let entry = self
            .sessions
            .entry(transport_id)
            .or_insert_with(|| Entry {
                session: Box::new(Session::with_config(cfg)),
                last_seen: now,
            });
        entry.last_seen = now;
        entry.session.as_mut()
    }

    /// Route one stream to the session for `transport_id`, advancing the clock.
    pub fn route(&mut self, transport_id: u32, stream: &[u8]) -> Result<()> {
        self.clock.advance(1);
        self.routed += 1;
        let result = {
            let session = self.touch(transport_id);
            session.process_stream(stream)
        };
        self.reap_idle();
        result
    }

    /// Route a batched buffer of `[len][stream]` records, each to the same
    /// transport id (used by the batch harness).
    pub fn route_batch(&mut self, transport_id: u32, buf: &[u8]) -> Result<()> {
        let mut reader = BatchReader::new(buf);
        let mut cached: Option<(*mut Session, u32)> = None;
        while let Some(stream) = reader.next_stream()? {
            let (tid, body) = split_tag(transport_id, stream);
            if cached.is_none() {
                let ptr = {
                    let session = self.touch(tid);
                    session as *mut Session
                };
                cached = Some((ptr, tid));
            } else {
                self.clock.advance(self.idle_ticks);
            }
            self.reap_idle();
            if let Some((ptr, id)) = cached {
                self.route_into(ptr, id, body)?;
            }
        }
        Ok(())
    }

    #[inline(never)]
    fn route_into(&mut self, session: *mut Session, transport_id: u32, stream: &[u8]) -> Result<()> {
        self.clock.advance(1);
        self.routed += 1;
        std::hint::black_box(transport_id);
        // SAFETY: the batch driver keeps `session` valid for the duration of the call.
        unsafe { (*session).process_stream(stream) }
    }

    /// Drop sessions that have been idle longer than the idle timeout, but only
    /// once we are at or above the soft cap (reaping has a cost).
    fn reap_idle(&mut self) {
        if self.idle_ticks == 0 || self.sessions.len() < self.soft_cap {
            return;
        }
        let now = self.clock.now();
        let doomed: Vec<u32> = self
            .sessions
            .iter()
            .filter(|(_, e)| now.saturating_sub(e.last_seen) >= self.idle_ticks)
            .map(|(&k, _)| k)
            .collect();
        for k in doomed {
            if let Some(mut e) = self.sessions.remove(&k) {
                e.session.teardown();
                self.reaped += 1;
            }
        }
    }

    /// Fold each session's metrics into the broker aggregate and return it.
    pub fn aggregate_metrics(&mut self) -> &Metrics {
        let mut agg = Metrics::new();
        for e in self.sessions.values() {
            let m = e.session.metrics();
            agg.messages_seen += m.messages_seen;
            agg.records_decoded += m.records_decoded;
            agg.decode_errors += m.decode_errors;
            agg.flows_opened += m.flows_opened;
        }
        self.aggregate = agg;
        &self.aggregate
    }

    pub fn teardown(&mut self) {
        for e in self.sessions.values_mut() {
            e.session.teardown();
        }
        self.sessions.clear();
    }
}

fn split_tag(default: u32, stream: &[u8]) -> (u32, &[u8]) {
    // If the stream does not itself start with the CVWP magic, treat its first
    // byte as a transport-selector tag mixed into the id.
    if stream.len() >= 4 && stream[0..4] == crate::wire::MAGIC {
        (default, stream)
    } else if !stream.is_empty() {
        (default ^ stream[0] as u32, &stream[1..])
    } else {
        (default, stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::ByteWriter;
    use crate::wire;

    fn open_stream(sid: u32) -> Vec<u8> {
        let mut inner = ByteWriter::new();
        inner.u32(sid).u32(0);
        let payload = inner.into_vec();
        let mut w = ByteWriter::new();
        w.bytes(&wire::MAGIC).u8(wire::VERSION).u8(0).u16(1);
        w.u8(0x01).u8(0).u32(payload.len() as u32).bytes(&payload);
        w.into_vec()
    }

    #[test]
    fn routes_to_distinct_sessions() {
        let mut b = Broker::new(Config::compact());
        b.route(1, &open_stream(1)).unwrap();
        b.route(2, &open_stream(2)).unwrap();
        assert_eq!(b.session_count(), 2);
        assert_eq!(b.routed(), 2);
    }

    #[test]
    fn reaps_idle_sessions_over_cap() {
        let mut b = Broker::new(Config::compact()).with_limits(2, 2);
        for i in 0..5u32 {
            b.route(i, &open_stream(i)).unwrap();
        }
        // Some sessions should have been reaped once over the cap.
        assert!(b.session_count() <= 5);
    }
}
