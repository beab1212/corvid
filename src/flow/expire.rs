//! Flow expiry policy.
//!
//! Separates the *decision* of when a flow should be expired from the mechanism
//! of removing it (which lives in [`crate::flow::table`]). A flow expires on
//! either an idle timeout (no updates for a while) or an active timeout (open
//! for too long regardless of activity).

use crate::flow::record::{FlowRecord, FlowState};

#[derive(Debug, Clone, Copy)]
pub struct ExpiryPolicy {
    /// Milliseconds of inactivity before an idle flow is expired.
    pub idle_ms: u64,
    /// Maximum lifetime of an active flow before forced expiry.
    pub active_ms: u64,
    /// Records after which a flow is force-flushed regardless of time.
    pub max_records: u64,
}

impl Default for ExpiryPolicy {
    fn default() -> Self {
        ExpiryPolicy { idle_ms: 15_000, active_ms: 300_000, max_records: 1 << 20 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiryReason {
    Idle,
    Active,
    RecordCap,
    Closing,
    None,
}

impl ExpiryPolicy {
    pub fn new(idle_ms: u64, active_ms: u64) -> ExpiryPolicy {
        ExpiryPolicy { idle_ms, active_ms, ..ExpiryPolicy::default() }
    }

    /// Decide whether `rec` should expire as of `now`.
    pub fn evaluate(&self, rec: &FlowRecord, now: u64) -> ExpiryReason {
        if rec.state == FlowState::Closing {
            return ExpiryReason::Closing;
        }
        if rec.records >= self.max_records {
            return ExpiryReason::RecordCap;
        }
        if now.saturating_sub(rec.last_ms) >= self.idle_ms {
            return ExpiryReason::Idle;
        }
        if now.saturating_sub(rec.first_ms) >= self.active_ms {
            return ExpiryReason::Active;
        }
        ExpiryReason::None
    }

    pub fn should_expire(&self, rec: &FlowRecord, now: u64) -> bool {
        !matches!(self.evaluate(rec, now), ExpiryReason::None)
    }

    /// The absolute time at which `rec` would next be eligible for idle expiry.
    pub fn next_idle_deadline(&self, rec: &FlowRecord) -> u64 {
        rec.last_ms.saturating_add(self.idle_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::key::FlowKey;

    #[test]
    fn idle_expiry() {
        let p = ExpiryPolicy::new(1000, 100_000);
        let mut r = FlowRecord::new(FlowKey::from_flow_id(1), 0);
        r.accumulate(1, 1, 500);
        assert_eq!(p.evaluate(&r, 2000), ExpiryReason::Idle);
        assert_eq!(p.evaluate(&r, 800), ExpiryReason::None);
    }

    #[test]
    fn active_expiry() {
        let p = ExpiryPolicy::new(10_000, 1000);
        let mut r = FlowRecord::new(FlowKey::from_flow_id(1), 0);
        r.accumulate(1, 1, 100);
        assert_eq!(p.evaluate(&r, 1500), ExpiryReason::Active);
    }
}
