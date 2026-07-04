//! CRC-32 (IEEE) and Adler-32 checksums.
//!
//! The stream trailer and the capture-file format both carry a CRC-32 over the
//! preceding bytes. Adler-32 is used as a cheaper integrity check on individual
//! compressed blocks.

/// Precomputed CRC-32 (IEEE 802.3, reflected) lookup table.
static CRC32_TABLE: [u32; 256] = build_crc32_table();

const fn build_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// Compute CRC-32 over `data`.
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xFFFF_FFFF, data) ^ 0xFFFF_FFFF
}

/// Incremental CRC-32 update. Pass `0xFFFFFFFF` as the initial `crc` and XOR the
/// final result with `0xFFFFFFFF`.
pub fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        let idx = ((crc ^ b as u32) & 0xFF) as usize;
        crc = CRC32_TABLE[idx] ^ (crc >> 8);
    }
    crc
}

const ADLER_MOD: u32 = 65521;

/// Compute Adler-32 over `data`.
pub fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    // Process in chunks to defer the modulo, matching the zlib approach.
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= ADLER_MOD;
        b %= ADLER_MOD;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vectors() {
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn adler_known() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn crc_incremental_matches() {
        let one = crc32(b"hello world");
        let mut c = 0xFFFF_FFFFu32;
        c = crc32_update(c, b"hello ");
        c = crc32_update(c, b"world");
        assert_eq!(c ^ 0xFFFF_FFFF, one);
    }
}
