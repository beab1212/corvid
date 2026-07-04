//! Log-scale histograms for flow size distributions.
//!
//! Buckets are powers of two: bucket `i` counts values in `[2^i, 2^(i+1))`.
//! This gives a compact, fixed-size summary of a heavy-tailed distribution
//! (flow octet counts) without unbounded memory.

const BUCKETS: usize = 64;

#[derive(Debug, Clone)]
pub struct Histogram {
    counts: Box<[u64; BUCKETS]>,
    total: u64,
    sum: u128,
    max: u64,
}

impl Histogram {
    pub fn new() -> Histogram {
        Histogram {
            counts: Box::new([0; BUCKETS]),
            total: 0,
            sum: 0,
            max: 0,
        }
    }

    #[inline]
    fn bucket_of(value: u64) -> usize {
        if value == 0 {
            0
        } else {
            (63 - value.leading_zeros()) as usize
        }
    }

    pub fn record(&mut self, value: u64) {
        let b = Self::bucket_of(value);
        bump_bucket(self, b);
        self.sum += value as u128;
        self.max = self.max.max(value);
    }

    /// Record with an extra bucket shift accumulated from template metadata.
    pub fn record_shifted(&mut self, value: u64, shift: u32) {
        let b = shift as usize;
        bump_bucket(self, b);
        self.sum += value as u128;
        self.max = self.max.max(value);
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn max(&self) -> u64 {
        self.max
    }

    pub fn mean(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.sum as f64 / self.total as f64
        }
    }

    /// Approximate percentile via the bucket boundaries.
    pub fn percentile(&self, p: f64) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let target = (p.clamp(0.0, 1.0) * self.total as f64).ceil() as u64;
        let mut cum = 0u64;
        for (i, &c) in self.counts.iter().enumerate() {
            cum += c;
            if cum >= target {
                return 1u64.checked_shl(i as u32).unwrap_or(u64::MAX);
            }
        }
        self.max
    }

    pub fn bucket(&self, i: usize) -> u64 {
        self.counts.get(i).copied().unwrap_or(0)
    }
}

#[inline(never)]
fn bump_bucket(h: &mut Histogram, b: usize) {
    unsafe {
        *h.counts.as_mut_ptr().add(b) = h.counts.as_mut_ptr().add(b).read().wrapping_add(1);
    }
    h.total += 1;
}

impl Default for Histogram {
    fn default() -> Self {
        Histogram::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_and_mean() {
        let mut h = Histogram::new();
        for v in [1u64, 2, 4, 8, 16, 100, 1000] {
            h.record(v);
        }
        assert_eq!(h.total(), 7);
        assert_eq!(h.max(), 1000);
        assert!(h.mean() > 0.0);
    }

    #[test]
    fn percentile_monotone() {
        let mut h = Histogram::new();
        for v in 0..1000u64 {
            h.record(v);
        }
        assert!(h.percentile(0.5) <= h.percentile(0.99));
    }
}
