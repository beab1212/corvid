//! MAC (EUI-48) address helpers.

use crate::error::{Error, Result};

/// Format 6 bytes as `aa:bb:cc:dd:ee:ff`.
pub fn fmt_mac(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(17);
    for (i, b) in bytes.iter().take(6).enumerate() {
        if i > 0 {
            s.push(':');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Parse `aa:bb:cc:dd:ee:ff` (or `-` separated) into 6 bytes.
pub fn parse_mac(s: &str) -> Result<[u8; 6]> {
    let mut out = [0u8; 6];
    let mut n = 0;
    for part in s.split([':', '-']) {
        if n >= 6 {
            return Err(Error::malformed("mac: too many octets"));
        }
        out[n] = u8::from_str_radix(part.trim(), 16)
            .map_err(|_| Error::malformed("mac: bad octet"))?;
        n += 1;
    }
    if n != 6 {
        return Err(Error::malformed("mac: need six octets"));
    }
    Ok(out)
}

/// Whether the address is locally administered (U/L bit set).
pub fn is_local(bytes: &[u8]) -> bool {
    !bytes.is_empty() && (bytes[0] & 0x02) != 0
}

/// Whether the address is a group/multicast address (I/G bit set).
pub fn is_multicast(bytes: &[u8]) -> bool {
    !bytes.is_empty() && (bytes[0] & 0x01) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let m = parse_mac("01:23:45:67:89:ab").unwrap();
        assert_eq!(fmt_mac(&m), "01:23:45:67:89:ab");
        assert!(is_multicast(&m));
    }

    #[test]
    fn rejects_short() {
        assert!(parse_mac("01:23:45").is_err());
    }
}
