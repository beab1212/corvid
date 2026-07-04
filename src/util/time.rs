//! Logical time helpers.
//!
//! The engine runs on a logical millisecond clock advanced by the caller rather
//! than wall-clock time, so replays and fuzzing are deterministic. These
//! helpers convert between the compact on-wire timestamp encodings and the
//! internal `u64` millisecond representation.

/// Milliseconds since the CVWP epoch (2020-01-01T00:00:00Z), which keeps
/// timestamps inside 40 bits for a couple of centuries.
pub const EPOCH_UNIX_SECS: u64 = 1_577_836_800;

/// Convert Unix seconds to the internal millisecond clock.
pub fn unix_secs_to_ms(secs: u64) -> u64 {
    secs.saturating_sub(EPOCH_UNIX_SECS).saturating_mul(1000)
}

/// Convert the internal millisecond clock back to Unix seconds.
pub fn ms_to_unix_secs(ms: u64) -> u64 {
    ms / 1000 + EPOCH_UNIX_SECS
}

/// Decode a compact 6-byte big-endian millisecond timestamp.
pub fn decode_ts48(bytes: &[u8]) -> Option<u64> {
    if bytes.len() < 6 {
        return None;
    }
    let mut v = 0u64;
    for &b in &bytes[..6] {
        v = (v << 8) | b as u64;
    }
    Some(v)
}

/// Encode a millisecond timestamp into 6 big-endian bytes.
pub fn encode_ts48(ms: u64) -> [u8; 6] {
    let mut out = [0u8; 6];
    let mut v = ms & 0xFFFF_FFFF_FFFF;
    for i in (0..6).rev() {
        out[i] = (v & 0xFF) as u8;
        v >>= 8;
    }
    out
}

/// A monotone logical clock that never goes backwards.
#[derive(Debug, Default, Clone)]
pub struct LogicalClock {
    now: u64,
}

impl LogicalClock {
    pub fn new(start: u64) -> LogicalClock {
        LogicalClock { now: start }
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    /// Advance by `delta` ms, saturating.
    pub fn advance(&mut self, delta: u64) -> u64 {
        self.now = self.now.saturating_add(delta);
        self.now
    }

    /// Move to `t` if it is in the future; otherwise stay put.
    pub fn observe(&mut self, t: u64) {
        if t > self.now {
            self.now = t;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts48_roundtrip() {
        let ms = 0x0000_1234_5678_9ABC & 0xFFFF_FFFF_FFFF;
        assert_eq!(decode_ts48(&encode_ts48(ms)), Some(ms));
    }

    #[test]
    fn clock_is_monotone() {
        let mut c = LogicalClock::new(100);
        c.observe(50);
        assert_eq!(c.now(), 100);
        c.advance(10);
        assert_eq!(c.now(), 110);
    }
}
