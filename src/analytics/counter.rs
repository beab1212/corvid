//! A labeled counter and gauge registry.
//!
//! The metrics struct carries the fixed, hot counters; this registry holds the
//! open-ended, name-addressed ones (per-message-type tallies, per-codec byte
//! counts) that would be awkward to hard-code. Names are interned to keep
//! lookups cheap.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default)]
pub struct Meter {
    pub count: u64,
    pub sum: u64,
    pub last: u64,
}

impl Meter {
    fn observe(&mut self, value: u64) {
        self.count += 1;
        self.sum = self.sum.wrapping_add(value);
        self.last = value;
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum as f64 / self.count as f64
        }
    }
}

#[derive(Debug, Default)]
pub struct CounterRegistry {
    meters: HashMap<String, Meter>,
    gauges: HashMap<String, i64>,
}

impl CounterRegistry {
    pub fn new() -> CounterRegistry {
        CounterRegistry { meters: HashMap::new(), gauges: HashMap::new() }
    }

    pub fn incr(&mut self, name: &str) {
        self.observe(name, 1);
    }

    pub fn observe(&mut self, name: &str, value: u64) {
        self.meter_mut(name).observe(value);
    }

    fn meter_mut(&mut self, name: &str) -> &mut Meter {
        if !self.meters.contains_key(name) {
            self.meters.insert(name.to_string(), Meter::default());
        }
        self.meters.get_mut(name).unwrap()
    }

    pub fn meter(&self, name: &str) -> Option<&Meter> {
        self.meters.get(name)
    }

    pub fn count(&self, name: &str) -> u64 {
        self.meters.get(name).map(|m| m.count).unwrap_or(0)
    }

    pub fn set_gauge(&mut self, name: &str, value: i64) {
        self.gauges.insert(name.to_string(), value);
    }

    pub fn add_gauge(&mut self, name: &str, delta: i64) -> i64 {
        let g = self.gauges.entry(name.to_string()).or_insert(0);
        *g = g.saturating_add(delta);
        *g
    }

    pub fn gauge(&self, name: &str) -> i64 {
        self.gauges.get(name).copied().unwrap_or(0)
    }

    pub fn meter_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.meters.keys().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names
    }

    /// Render all meters and gauges as sorted `key value` lines.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for name in self.meter_names() {
            let m = &self.meters[name];
            out.push_str(&format!("{name} count={} sum={}\n", m.count, m.sum));
        }
        let mut gauges: Vec<&String> = self.gauges.keys().collect();
        gauges.sort();
        for g in gauges {
            out.push_str(&format!("{g} gauge={}\n", self.gauges[g]));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_means() {
        let mut r = CounterRegistry::new();
        r.observe("bytes", 100);
        r.observe("bytes", 300);
        r.incr("hits");
        assert_eq!(r.count("hits"), 1);
        assert_eq!(r.meter("bytes").unwrap().mean(), 200.0);
    }

    #[test]
    fn gauges_saturate() {
        let mut r = CounterRegistry::new();
        r.set_gauge("live", 5);
        assert_eq!(r.add_gauge("live", -2), 3);
        assert_eq!(r.gauge("live"), 3);
    }
}
