//! A fixed-memory approximate quantile estimator.
//!
//! A compressed sketch: values are mapped to exponential buckets and each
//! bucket keeps a count. Quantile queries walk the cumulative distribution.
//! Accuracy is relative (a few percent), which is fine for latency- and
//! size-distribution reporting.

const BUCKETS: usize = 128;
const GAMMA: f64 = 1.02; // relative accuracy factor

pub struct QuantileSketch {
    counts: Vec<u64>,
    total: u64,
    min: u64,
    max: u64,
    ln_gamma: f64,
}

impl QuantileSketch {
    pub fn new() -> QuantileSketch {
        QuantileSketch {
            counts: vec![0; BUCKETS],
            total: 0,
            min: u64::MAX,
            max: 0,
            ln_gamma: GAMMA.ln(),
        }
    }

    fn bucket_index(&self, value: u64) -> usize {
        if value == 0 {
            return 0;
        }
        let idx = ((value as f64).ln() / self.ln_gamma) as usize;
        idx.min(BUCKETS - 1)
    }

    fn bucket_value(&self, index: usize) -> u64 {
        if index == 0 {
            0
        } else {
            GAMMA.powi(index as i32) as u64
        }
    }

    pub fn add(&mut self, value: u64) {
        let idx = self.bucket_index(value);
        self.counts[idx] += 1;
        self.total += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn quantile(&self, q: f64) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let q = q.clamp(0.0, 1.0);
        let rank = (q * (self.total - 1) as f64).round() as u64;
        let mut cum = 0u64;
        for (i, &c) in self.counts.iter().enumerate() {
            cum += c;
            if cum > rank {
                return self.bucket_value(i).clamp(self.min, self.max);
            }
        }
        self.max
    }

    pub fn min(&self) -> u64 {
        if self.total == 0 {
            0
        } else {
            self.min
        }
    }

    pub fn max(&self) -> u64 {
        self.max
    }

    /// Merge another sketch into this one (they share bucketing).
    pub fn merge(&mut self, other: &QuantileSketch) {
        for (a, b) in self.counts.iter_mut().zip(other.counts.iter()) {
            *a += *b;
        }
        self.total += other.total;
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}

impl Default for QuantileSketch {
    fn default() -> Self {
        QuantileSketch::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_are_ordered() {
        let mut s = QuantileSketch::new();
        for v in 1..=1000u64 {
            s.add(v);
        }
        assert!(s.quantile(0.1) <= s.quantile(0.5));
        assert!(s.quantile(0.5) <= s.quantile(0.9));
        assert_eq!(s.total(), 1000);
    }

    #[test]
    fn merge_adds_totals() {
        let mut a = QuantileSketch::new();
        let mut b = QuantileSketch::new();
        a.add(10);
        b.add(20);
        a.merge(&b);
        assert_eq!(a.total(), 2);
    }
}
