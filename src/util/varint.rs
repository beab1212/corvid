//! LEB128-style variable-length integer coding.
//!
//! CVWP uses varints for record field lengths and for delta-coded columns so
//! that the common small values cost a single byte. Decoding is bounded: a
//! varint may not exceed ten bytes (enough for a full u64) and may not run past
//! the supplied slice.

use crate::error::{Error, Result};

const MAX_VARINT_BYTES: usize = 10;

/// Decode a varint starting at `buf[*pos]`, advancing `*pos` past it.
pub fn decode(buf: &[u8], pos: &mut usize) -> Result<u64> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut consumed = 0usize;

    loop {
        if *pos >= buf.len() {
            return Err(Error::malformed("varint truncated"));
        }
        if consumed >= MAX_VARINT_BYTES {
            return Err(Error::malformed("varint too long"));
        }
        let byte = buf[*pos];
        *pos += 1;
        consumed += 1;

        // The final byte of a ten-byte varint may only carry one meaningful
        // bit; anything more would overflow a u64.
        if shift >= 64 {
            if byte != 0 {
                return Err(Error::malformed("varint overflow"));
            }
            return Ok(result);
        }

        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
}

/// Encode `value` as a varint, appending to `out`.
pub fn encode(value: u64, out: &mut Vec<u8>) {
    let mut v = value;
    loop {
        let mut byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if v == 0 {
            break;
        }
    }
}

/// Number of bytes `value` would occupy when varint-encoded.
pub fn encoded_len(value: u64) -> usize {
    let mut v = value;
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for &v in &[0u64, 1, 127, 128, 300, 16384, u32::MAX as u64, u64::MAX] {
            let mut out = Vec::new();
            encode(v, &mut out);
            assert_eq!(out.len(), encoded_len(v));
            let mut pos = 0;
            assert_eq!(decode(&out, &mut pos).unwrap(), v);
            assert_eq!(pos, out.len());
        }
    }

    #[test]
    fn truncated_is_error() {
        let buf = [0x80, 0x80];
        let mut pos = 0;
        assert!(decode(&buf, &mut pos).is_err());
    }
}
