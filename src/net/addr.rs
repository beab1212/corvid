//! IPv4 / IPv6 address helpers.
//!
//! Flow keys carry addresses as raw integers; these helpers render and parse
//! the textual forms used by the CLI and the JSON/text exporters.

use crate::error::{Error, Result};

/// Format a big-endian IPv4 address (as a `u32`) in dotted-quad notation.
pub fn fmt_ipv4(addr: u32) -> String {
    format!(
        "{}.{}.{}.{}",
        (addr >> 24) & 0xFF,
        (addr >> 16) & 0xFF,
        (addr >> 8) & 0xFF,
        addr & 0xFF
    )
}

/// Parse a dotted-quad IPv4 address into a big-endian `u32`.
pub fn parse_ipv4(s: &str) -> Result<u32> {
    let mut octets = [0u32; 4];
    let mut count = 0;
    for part in s.split('.') {
        if count >= 4 {
            return Err(Error::malformed("ipv4: too many octets"));
        }
        let v: u32 = part.parse().map_err(|_| Error::malformed("ipv4: bad octet"))?;
        if v > 255 {
            return Err(Error::malformed("ipv4: octet out of range"));
        }
        octets[count] = v;
        count += 1;
    }
    if count != 4 {
        return Err(Error::malformed("ipv4: need four octets"));
    }
    Ok((octets[0] << 24) | (octets[1] << 16) | (octets[2] << 8) | octets[3])
}

/// Format 16 bytes as an IPv6 address with `::` zero-run compression.
pub fn fmt_ipv6(bytes: &[u8; 16]) -> String {
    let groups: [u16; 8] = std::array::from_fn(|i| {
        u16::from_be_bytes([bytes[i * 2], bytes[i * 2 + 1]])
    });

    // Find the longest run of zero groups.
    let mut best_start = 0usize;
    let mut best_len = 0usize;
    let mut cur_start = 0usize;
    let mut cur_len = 0usize;
    for (i, &g) in groups.iter().enumerate() {
        if g == 0 {
            if cur_len == 0 {
                cur_start = i;
            }
            cur_len += 1;
            if cur_len > best_len {
                best_len = cur_len;
                best_start = cur_start;
            }
        } else {
            cur_len = 0;
        }
    }

    if best_len < 2 {
        return groups.iter().map(|g| format!("{g:x}")).collect::<Vec<_>>().join(":");
    }

    let mut out = String::new();
    let mut i = 0;
    while i < 8 {
        if i == best_start {
            out.push_str("::");
            i += best_len;
            continue;
        }
        if !out.ends_with(':') && !out.is_empty() {
            out.push(':');
        }
        out.push_str(&format!("{:x}", groups[i]));
        i += 1;
    }
    out
}

/// The well-known transport protocol numbers we name in output.
pub fn proto_name(proto: u8) -> &'static str {
    match proto {
        1 => "icmp",
        6 => "tcp",
        17 => "udp",
        47 => "gre",
        50 => "esp",
        58 => "icm6",
        132 => "sctp",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_roundtrip() {
        let a = parse_ipv4("10.0.0.1").unwrap();
        assert_eq!(a, 0x0a000001);
        assert_eq!(fmt_ipv4(a), "10.0.0.1");
    }

    #[test]
    fn ipv4_rejects_bad() {
        assert!(parse_ipv4("1.2.3").is_err());
        assert!(parse_ipv4("256.0.0.1").is_err());
    }

    #[test]
    fn ipv6_compression() {
        let mut b = [0u8; 16];
        b[0] = 0x20;
        b[1] = 0x01;
        b[15] = 1;
        assert_eq!(fmt_ipv6(&b), "2001::1");
    }
}
