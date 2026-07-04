//! Shannon-entropy estimation over a byte stream.
//!
//! Used to flag likely-encrypted or already-compressed payloads (high entropy)
//! versus structured data (low entropy), which the pipeline uses to decide
//! whether re-compressing is worthwhile.

#[derive(Debug, Clone)]
pub struct EntropyMeter {
    counts: [u64; 256],
    total: u64,
}

impl Default for EntropyMeter {
    fn default() -> Self {
        EntropyMeter { counts: [0; 256], total: 0 }
    }
}

impl EntropyMeter {
    pub fn new() -> EntropyMeter {
        EntropyMeter::default()
    }

    pub fn observe(&mut self, data: &[u8]) {
        for &b in data {
            self.counts[b as usize] += 1;
        }
        self.total += data.len() as u64;
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    /// Bits of entropy per byte in `[0, 8]`.
    pub fn bits_per_byte(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        let total = self.total as f64;
        let mut h = 0.0;
        for &c in self.counts.iter() {
            if c == 0 {
                continue;
            }
            let p = c as f64 / total;
            h -= p * p.log2();
        }
        h
    }

    /// A crude "looks random" flag: within 3% of the 8-bit maximum.
    pub fn is_high_entropy(&self) -> bool {
        self.bits_per_byte() >= 7.76
    }

    pub fn distinct_bytes(&self) -> usize {
        self.counts.iter().filter(|&&c| c > 0).count()
    }
}

/// One-shot entropy of a slice.
pub fn shannon_bits(data: &[u8]) -> f64 {
    let mut m = EntropyMeter::new();
    m.observe(data);
    m.bits_per_byte()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_is_max_entropy() {
        let data: Vec<u8> = (0..=255u16).map(|b| b as u8).collect();
        let bits = shannon_bits(&data);
        assert!(bits > 7.99);
    }

    #[test]
    fn constant_is_zero_entropy() {
        assert_eq!(shannon_bits(&[7u8; 100]), 0.0);
    }
}
