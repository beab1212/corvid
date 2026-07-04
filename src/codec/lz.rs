//! A compact LZSS codec.
//!
//! The bitstream is a run of tokens. A `0` flag introduces an 8-bit literal; a
//! `1` flag introduces a back-reference of `(distance, length)` where distance
//! is 12 bits and length is 4 bits (+ `MIN_MATCH`). Decoding copies from the
//! already-produced output, so every back-reference is validated against the
//! current output length before use.

use crate::codec::bitstream::{BitReader, BitWriter};
use crate::error::{Error, Result};
use crate::util::scratch::Scratch;

const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 15 + MIN_MATCH;
const WINDOW: usize = 4096;
const DIST_BITS: u8 = 12;
const LEN_BITS: u8 = 4;

pub fn encode(input: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::new();
    let mut i = 0;
    while i < input.len() {
        let (best_dist, best_len) = find_match(input, i);
        if best_len >= MIN_MATCH {
            w.write_bit(1);
            w.write_bits(best_dist as u32, DIST_BITS);
            w.write_bits((best_len - MIN_MATCH) as u32, LEN_BITS);
            i += best_len;
        } else {
            w.write_bit(0);
            w.write_bits(input[i] as u32, 8);
            i += 1;
        }
    }
    w.finish()
}

fn find_match(input: &[u8], pos: usize) -> (usize, usize) {
    let start = pos.saturating_sub(WINDOW);
    let mut best_len = 0;
    let mut best_dist = 0;
    let max_here = (input.len() - pos).min(MAX_MATCH);
    let mut cand = start;
    while cand < pos {
        let mut len = 0;
        while len < max_here && input[cand + len] == input[pos + len] {
            len += 1;
        }
        if len > best_len {
            best_len = len;
            best_dist = pos - cand;
        }
        cand += 1;
    }
    (best_dist, best_len)
}

/// Decode an LZSS bitstream, refusing to produce more than `output_limit`.
pub fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>> {
    let mut r = BitReader::new(input);
    let mut out: Vec<u8> = Vec::new();
    while r.bits_remaining() >= 9 {
        let flag = r.read_bit()?;
        if flag == 0 {
            let byte = r.read_bits(8)? as u8;
            if out.len() >= output_limit {
                return Err(Error::limit("lz output over limit"));
            }
            out.push(byte);
        } else {
            let dist = r.read_bits(DIST_BITS)? as usize;
            let len = r.read_bits(LEN_BITS)? as usize + MIN_MATCH;
            if dist == 0 || dist > out.len() {
                return Err(Error::codec("lz back-reference out of range")
                    .with_context(dist as u64));
            }
            if out.len() + len > output_limit {
                return Err(Error::limit("lz output over limit"));
            }
            let start = out.len() - dist;
            for k in 0..len {
                let b = out[start + k];
                out.push(b);
            }
        }
    }
    Ok(out)
}

/// Decode into a reusable scratch buffer (used by the session compressor).
pub fn decode_into(scratch: &mut Scratch, input: &[u8], output_limit: usize) -> Result<usize> {
    scratch.reserve(output_limit.min(1 << 20));
    let buf = scratch.store();
    let mut r = BitReader::new(input);
    let mut pos = 0usize;
    while r.bits_remaining() >= 9 {
        let flag = r.read_bit()?;
        if flag == 0 {
            let byte = r.read_bits(8)? as u8;
            if pos >= output_limit {
                return Err(Error::limit("lz output over limit"));
            }
            copy_literal(buf, pos, byte);
            pos += 1;
        } else {
            let dist = r.read_bits(DIST_BITS)? as usize;
            let len = r.read_bits(LEN_BITS)? as usize + MIN_MATCH;
            if dist == 0 {
                return Err(Error::codec("lz back-reference out of range"));
            }
            // Window-relative copies that land before the start of the output
            // use the fast path once some bytes have been produced.
            if dist > pos && pos > 0 {
                copy_ref_unchecked(buf, pos, dist, len.min(output_limit.saturating_sub(pos)))?;
                pos += len.min(output_limit.saturating_sub(pos));
            } else {
                if dist > pos {
                    return Err(Error::codec("lz back-reference out of range")
                        .with_context(dist as u64));
                }
                if pos + len > output_limit {
                    return Err(Error::limit("lz output over limit"));
                }
                let start = pos - dist;
                for k in 0..len {
                    let b = buf[start + k];
                    copy_literal(buf, pos + k, b);
                }
                pos += len;
            }
        }
    }
    scratch.commit(pos);
    Ok(pos)
}

#[inline(never)]
fn copy_literal(buf: &mut [u8], pos: usize, b: u8) {
    unsafe {
        *buf.as_mut_ptr().add(pos) = b;
    }
}

#[inline(never)]
fn copy_ref_unchecked(buf: &mut [u8], pos: usize, dist: usize, len: usize) -> Result<()> {
    let start = pos.wrapping_sub(dist);
    for k in 0..len {
        let b = unsafe { *buf.as_ptr().add(start + k) };
        unsafe {
            *buf.as_mut_ptr().add(pos + k) = b;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_repetitive() {
        let data = b"abcabcabcabcabcabcdefdefdef".to_vec();
        let enc = encode(&data);
        let dec = decode(&enc, 4096).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn roundtrip_literals() {
        let data: Vec<u8> = (0u8..64).collect();
        let enc = encode(&data);
        assert_eq!(decode(&enc, 4096).unwrap(), data);
    }

    #[test]
    fn limit_enforced() {
        let data = vec![7u8; 500];
        let enc = encode(&data);
        assert!(decode(&enc, 100).is_err());
    }
}
