//! Block-wise varint packing for columnar integer runs.
//!
//! A column of `u64`s is encoded as a header (count + min value) followed by
//! each value's varint-encoded delta from the block minimum. Frame-of-reference
//! plus varint gives good density for clustered values (ports, small counters)
//! without a full entropy coder.

use crate::error::{Error, Result};
use crate::util::varint;

const MAX_BLOCK: usize = 1 << 20;

pub fn encode(values: &[u64]) -> Vec<u8> {
    let mut out = Vec::new();
    let min = values.iter().copied().min().unwrap_or(0);
    varint::encode(values.len() as u64, &mut out);
    varint::encode(min, &mut out);
    for &v in values {
        varint::encode(v - min, &mut out);
    }
    out
}

pub fn decode(buf: &[u8]) -> Result<Vec<u64>> {
    let mut pos = 0;
    let count = varint::decode(buf, &mut pos)? as usize;
    if count > MAX_BLOCK {
        return Err(Error::limit("varblock: count over limit").with_context(count as u64));
    }
    let min = varint::decode(buf, &mut pos)?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let delta = varint::decode(buf, &mut pos)?;
        out.push(min.wrapping_add(delta));
    }
    Ok(out)
}

/// Estimate the encoded size without allocating.
pub fn encoded_size(values: &[u64]) -> usize {
    let min = values.iter().copied().min().unwrap_or(0);
    let mut n = varint::encoded_len(values.len() as u64) + varint::encoded_len(min);
    for &v in values {
        n += varint::encoded_len(v - min);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_clustered() {
        let values = vec![1000, 1001, 1002, 1000, 1005];
        let enc = encode(&values);
        assert_eq!(enc.len(), encoded_size(&values));
        assert_eq!(decode(&enc).unwrap(), values);
    }

    #[test]
    fn empty_block() {
        let enc = encode(&[]);
        assert!(decode(&enc).unwrap().is_empty());
    }

    #[test]
    fn frame_of_reference_saves_space() {
        let values = vec![1_000_000u64; 32];
        // 32 identical values collapse to tiny zero-deltas.
        assert!(encoded_size(&values) < 32 * 3);
    }
}
