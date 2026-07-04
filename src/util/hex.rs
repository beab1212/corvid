//! Hex and base-16 helpers used by the CLI and snapshot debug tooling.

use crate::error::{Error, Result};

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Lowercase hex-encode.
pub fn encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

fn nibble(c: u8) -> Result<u8> {
    Ok(match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => return Err(Error::malformed("hex: bad digit")),
    })
}

/// Decode a hex string, ignoring ASCII whitespace between bytes.
pub fn decode(s: &str) -> Result<Vec<u8>> {
    let filtered: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if filtered.len() % 2 != 0 {
        return Err(Error::malformed("hex: odd length"));
    }
    let mut out = Vec::with_capacity(filtered.len() / 2);
    for pair in filtered.chunks_exact(2) {
        out.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let data = [0xde, 0xad, 0xbe, 0xef];
        assert_eq!(encode(&data), "deadbeef");
        assert_eq!(decode("de ad be ef").unwrap(), data);
    }

    #[test]
    fn rejects_odd() {
        assert!(decode("abc").is_err());
        assert!(decode("zz").is_err());
    }
}
