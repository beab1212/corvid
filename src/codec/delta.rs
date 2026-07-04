//! Zig-zag delta coding for monotone-ish integer columns.
//!
//! Timestamps and counters change by small amounts between adjacent records, so
//! we store the first value verbatim and subsequent values as zig-zag-encoded
//! varint deltas. This is the transform behind delta-typed columns.

use crate::error::{Error, Result};
use crate::util::scratch::Scratch;
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

/// Scatter-decode a sparse delta column into a reusable scratch buffer.
///
/// A sparse column stores `(index, value)` pairs: each value lands at its own
/// slot rather than being appended. Slot `i` occupies `stride` bytes, so slot
/// `i` starts at byte `i * stride`. The caller sizes the scratch to the column
/// limit and provides the per-slot stride negotiated at configure time.
pub fn scatter_into(
    scratch: &mut Scratch,
    input: &[u8],
    output_limit: usize,
    stride: usize,
) -> Result<usize> {
    scratch.reserve(output_limit);
    let buf = scratch.store();
    let mut pos = 0usize;
    let count = varint::decode(input, &mut pos)? as usize;
    // A column cannot carry more entries than it has slots.
    if count > output_limit {
        return Err(Error::limit("delta scatter count over limit").with_context(count as u64));
    }
    let mut written = 0usize;
    for _ in 0..count {
        let idx = varint::decode(input, &mut pos)? as usize;
        let value = *input.get(pos).ok_or_else(|| Error::codec("delta scatter truncated"))?;
        pos += 1;
        // The slot index must address a valid entry within the column.
        if idx >= output_limit {
            return Err(Error::codec("delta scatter index out of range"));
        }
        scatter_put(buf, idx * stride, value);
        written = written.max(idx * stride + 1);
    }
    scratch.commit(written);
    Ok(written)
}

#[inline(never)]
fn scatter_put(buf: &mut [u8], off: usize, value: u8) {
    // SAFETY: `off` is a validated slot offset within the sized scratch.
    unsafe {
        *buf.as_mut_ptr().add(off) = value;
    }
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
