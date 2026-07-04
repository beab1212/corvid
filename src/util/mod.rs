//! Small, dependency-free helpers shared across subsystems.

pub mod base64;
pub mod bitset;
pub mod checksum;
pub mod hex;
pub mod interner;
pub mod lifetime;
pub mod reader;
pub mod ringbuf;
pub mod time;
pub mod varint;
pub mod writer;

pub use reader::ByteReader;
pub use writer::ByteWriter;

/// FNV-1a over a byte slice. Used for the schema/template lookup maps where we
/// want a cheap, stable, non-cryptographic hash and don't care about DoS
/// resistance (inputs are already length-bounded upstream).
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Mix two 32-bit ids into a single 64-bit key (domain + object id).
pub fn mix_key(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | (lo as u64)
}

/// Render a byte slice as a spaced hex string, truncating long inputs. Used by
/// the debug logger and the `corvidctl inspect` subcommand.
pub fn hex_preview(bytes: &[u8], max: usize) -> String {
    let mut s = String::with_capacity(max.min(bytes.len()) * 3);
    for (i, b) in bytes.iter().take(max).enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    if bytes.len() > max {
        s.push_str(" …");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(fnv1a(b"a"), fnv1a(b"b"));
    }

    #[test]
    fn key_mix_is_injective_enough() {
        assert_ne!(mix_key(1, 2), mix_key(2, 1));
    }

    #[test]
    fn hex_truncates() {
        assert!(hex_preview(&[0xde, 0xad], 1).contains('…'));
        assert_eq!(hex_preview(&[0xde, 0xad], 8), "de ad");
    }
}
