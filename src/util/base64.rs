//! Standard base64 (RFC 4648) encode/decode.
//!
//! The JSON exporter uses this to render opaque byte fields; keeping a local
//! implementation avoids a dependency for such a small, stable codec.

use crate::error::{Error, Result};

const ENC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const PAD: u8 = b'=';

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ENC[((n >> 18) & 0x3F) as usize] as char);
        out.push(ENC[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(ENC[((n >> 6) & 0x3F) as usize] as char);
        } else {
            out.push(PAD as char);
        }
        if chunk.len() > 2 {
            out.push(ENC[(n & 0x3F) as usize] as char);
        } else {
            out.push(PAD as char);
        }
    }
    out
}

fn dec_byte(c: u8) -> Result<u32> {
    Ok(match c {
        b'A'..=b'Z' => (c - b'A') as u32,
        b'a'..=b'z' => (c - b'a' + 26) as u32,
        b'0'..=b'9' => (c - b'0' + 52) as u32,
        b'+' => 62,
        b'/' => 63,
        _ => return Err(Error::malformed("base64: bad char")),
    })
}

pub fn decode(input: &str) -> Result<Vec<u8>> {
    let bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if bytes.len() % 4 != 0 {
        return Err(Error::malformed("base64: length not a multiple of 4"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pads = chunk.iter().filter(|&&c| c == PAD).count();
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let v = if c == PAD { 0 } else { dec_byte(c)? };
            n |= v << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if pads < 2 {
            out.push((n >> 8) as u8);
        }
        if pads < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let enc = encode(s);
            assert_eq!(decode(&enc).unwrap(), s);
        }
    }

    #[test]
    fn known_vector() {
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn rejects_bad() {
        assert!(decode("###").is_err());
    }
}
