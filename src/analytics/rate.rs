//! Rate meters.
//!
//! [`EwmaRate`] tracks an exponentially-weighted moving average of a per-second
//! rate, the way load averages are computed. [`RateWindow`] keeps a small ring
//! of recent buckets for an exact windowed count.

/// Exponentially-weighted moving average rate meter.
#[derive(Debug, Clone)]
pub struct EwmaRate {
    alpha: f64,
    rate: f64,
    last_ms: u64,
    accum: u64,
    initialised: bool,
}

impl EwmaRate {
    /// `half_life_ms` controls how quickly old samples decay.
    pub fn new(half_life_ms: u64) -> EwmaRate {
        let hl = half_life_ms.max(1) as f64;
        // alpha for a 1-second tick given the requested half life.
        let alpha = 1.0 - (-(1000.0f64) * std::f64::consts::LN_2 / hl).exp();
        EwmaRate { alpha, rate: 0.0, last_ms: 0, accum: 0, initialised: false }
    }

    pub fn observe(&mut self, count: u64, now_ms: u64) {
        if !self.initialised {
            self.last_ms = now_ms;
            self.initialised = true;
        }
        self.accum = self.accum.saturating_add(count);
        if now_ms.saturating_sub(self.last_ms) >= 1000 {
            let elapsed = (now_ms - self.last_ms) as f64 / 1000.0;
            let instant = self.accum as f64 / elapsed.max(1e-9);
            self.rate += self.alpha * (instant - self.rate);
            self.accum = 0;
            self.last_ms = now_ms;
        }
    }

    pub fn per_second(&self) -> f64 {
        self.rate
    }
}

/// Exact windowed counter over a ring of one-second buckets.
#[derive(Debug, Clone)]
pub struct RateWindow {
    buckets: Vec<u64>,
    base_sec: u64,
    cursor: usize,
    initialised: bool,
}

impl RateWindow {
    pub fn new(window_secs: usize) -> RateWindow {
        RateWindow {
            buckets: vec![0; window_secs.max(1)],
            base_sec: 0,
            cursor: 0,
            initialised: false,
        }
    }

    pub fn observe(&mut self, count: u64, now_ms: u64) {
        let sec = now_ms / 1000;
        if !self.initialised {
            self.base_sec = sec;
            self.initialised = true;
        }
        let advance = sec.saturating_sub(self.base_sec);
        for _ in 0..advance.min(self.buckets.len() as u64) {
            self.cursor = (self.cursor + 1) % self.buckets.len();
            self.buckets[self.cursor] = 0;
        }
        if advance >= self.buckets.len() as u64 {
            for b in self.buckets.iter_mut() {
                *b = 0;
            }
        }
        self.base_sec = sec;
        self.buckets[self.cursor] = self.buckets[self.cursor].saturating_add(count);
    }

    pub fn total(&self) -> u64 {
        self.buckets.iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ewma_converges_upward() {
        let mut m = EwmaRate::new(2000);
        for s in 1..=30 {
            m.observe(100, s * 1000);
        }
        assert!(m.per_second() > 50.0);
    }

    #[test]
    fn window_sums_recent() {
        let mut w = RateWindow::new(3);
        w.observe(5, 0);
        w.observe(5, 1000);
        assert_eq!(w.total(), 10);
    }
}
