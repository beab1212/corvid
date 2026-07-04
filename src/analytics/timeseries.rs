//! A fixed-width bucketed time series.
//!
//! Samples are placed into contiguous time buckets of a configured resolution.
//! The series keeps a bounded number of buckets; once time advances past the
//! end the oldest buckets are recycled, giving a rolling view of recent
//! activity suitable for the metrics endpoint.

#[derive(Debug, Clone, Copy, Default)]
pub struct Bucket {
    pub start_ms: u64,
    pub count: u64,
    pub sum: u64,
    pub max: u64,
}

impl Bucket {
    fn reset(&mut self, start_ms: u64) {
        self.start_ms = start_ms;
        self.count = 0;
        self.sum = 0;
        self.max = 0;
    }

    fn add(&mut self, value: u64) {
        self.count += 1;
        self.sum = self.sum.wrapping_add(value);
        if value > self.max {
            self.max = value;
        }
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum as f64 / self.count as f64
        }
    }
}

pub struct TimeSeries {
    resolution_ms: u64,
    buckets: Vec<Bucket>,
    cursor: usize,
    current_start: u64,
    initialised: bool,
}

impl TimeSeries {
    pub fn new(resolution_ms: u64, bucket_count: usize) -> TimeSeries {
        TimeSeries {
            resolution_ms: resolution_ms.max(1),
            buckets: vec![Bucket::default(); bucket_count.max(1)],
            cursor: 0,
            current_start: 0,
            initialised: false,
        }
    }

    fn bucket_start(&self, ts_ms: u64) -> u64 {
        ts_ms - (ts_ms % self.resolution_ms)
    }

    pub fn observe(&mut self, ts_ms: u64, value: u64) {
        let start = self.bucket_start(ts_ms);
        if !self.initialised {
            self.current_start = start;
            self.buckets[self.cursor].reset(start);
            self.initialised = true;
        }
        while start > self.current_start {
            self.cursor = (self.cursor + 1) % self.buckets.len();
            self.current_start += self.resolution_ms;
            self.buckets[self.cursor].reset(self.current_start);
        }
        if start == self.current_start {
            self.buckets[self.cursor].add(value);
        }
        // Samples older than the current bucket are dropped.
    }

    /// The buckets in chronological order, oldest first.
    pub fn snapshot(&self) -> Vec<Bucket> {
        if !self.initialised {
            return Vec::new();
        }
        let n = self.buckets.len();
        let mut out = Vec::with_capacity(n);
        for i in 1..=n {
            let idx = (self.cursor + i) % n;
            let b = self.buckets[idx];
            if b.count > 0 || b.start_ms != 0 {
                out.push(b);
            }
        }
        out
    }

    pub fn total_count(&self) -> u64 {
        self.buckets.iter().map(|b| b.count).sum()
    }

    pub fn rate_per_sec(&self) -> f64 {
        let span = self.buckets.len() as f64 * self.resolution_ms as f64 / 1000.0;
        if span <= 0.0 {
            0.0
        } else {
            self.total_count() as f64 / span
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_advance() {
        let mut ts = TimeSeries::new(1000, 4);
        ts.observe(0, 10);
        ts.observe(500, 20);
        ts.observe(1000, 5);
        assert_eq!(ts.total_count(), 3);
        let snap = ts.snapshot();
        assert!(snap.iter().any(|b| b.count == 2));
    }

    #[test]
    fn old_samples_recycle() {
        let mut ts = TimeSeries::new(1000, 2);
        ts.observe(0, 1);
        ts.observe(5000, 1); // far in the future, recycles buckets
        assert!(ts.total_count() >= 1);
    }
}
