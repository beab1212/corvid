//! Zig-zag delta coding for monotone-ish integer columns.
//!
//! Timestamps and counters change by small amounts between adjacent records, so
//! we store the first value verbatim and subsequent values as zig-zag-encoded
//! varint deltas. This is the transform behind delta-typed columns.

use crate::error::{Error, Result};
use crate::util::varint;

#[inline]
fn zigzag_encode(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

#[inline]
fn zigzag_decode(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

/// Encode a column of u64 values as delta+zigzag+varint.
pub fn encode(values: &[u64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() + 8);
    varint::encode(values.len() as u64, &mut out);
    let mut prev: i64 = 0;
    for &v in values {
        let cur = v as i64;
        let delta = cur.wrapping_sub(prev);
        varint::encode(zigzag_encode(delta), &mut out);
        prev = cur;
    }
    out
}

/// Decode a delta-coded column, refusing to produce more than `max_values`.
pub fn decode(input: &[u8], max_values: usize) -> Result<Vec<u64>> {
    let mut pos = 0;
    let count = varint::decode(input, &mut pos)? as usize;
    if count > max_values {
        return Err(Error::limit("delta column too long").with_context(count as u64));
    }
    let mut out = Vec::with_capacity(count);
    let mut prev: i64 = 0;
    for _ in 0..count {
        let z = varint::decode(input, &mut pos)?;
        let delta = zigzag_decode(z);
        prev = prev.wrapping_add(delta);
        out.push(prev as u64);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_symmetry() {
        for v in [-5i64, -1, 0, 1, 2, 1000, i64::MIN, i64::MAX] {
            assert_eq!(zigzag_decode(zigzag_encode(v)), v);
        }
    }

    #[test]
    fn column_roundtrip() {
        let vals = vec![1000u64, 1001, 1002, 999, 5000, 5000];
        let enc = encode(&vals);
        assert_eq!(decode(&enc, 1024).unwrap(), vals);
    }

    #[test]
    fn respects_max() {
        let enc = encode(&vec![0u64; 50]);
        assert!(decode(&enc, 10).is_err());
    }
}
