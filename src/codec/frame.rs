//! A self-describing compressed-frame container.
//!
//! Bundles a codec selector, an integrity checksum and the original length with
//! the compressed bytes so a decoder can pick the right algorithm and verify
//! the result:
//!
//! ```text
//! codec    : 1 byte
//! raw_len  : 4 bytes (big-endian) original length
//! adler    : 4 bytes (big-endian) Adler-32 of the *decompressed* bytes
//! payload  : remaining bytes (compressed)
//! ```

use crate::codec::{compress::inflate_block, Codec};
use crate::error::{Error, Result};
use crate::util::checksum::adler32;
use crate::util::{ByteReader, ByteWriter};

/// Wrap `raw` under `codec`, producing a container frame. (The compression step
/// itself is performed by the caller; this only frames the already-compressed
/// payload with metadata computed over `raw`.)
pub fn wrap(codec: Codec, raw: &[u8], compressed: &[u8]) -> Vec<u8> {
    let mut w = ByteWriter::new();
    w.u8(codec.code());
    w.u32(raw.len() as u32);
    w.u32(adler32(raw));
    w.bytes(compressed);
    w.into_vec()
}

/// Decode a container frame, decompress it and verify the checksum.
pub fn unwrap(frame: &[u8], output_limit: usize) -> Result<Vec<u8>> {
    let mut r = ByteReader::new(frame);
    let codec = Codec::from_code(r.u8()?)?;
    let raw_len = r.u32()? as usize;
    let expect_adler = r.u32()?;
    let payload = r.rest();

    if raw_len > output_limit {
        return Err(Error::limit("frame raw_len over limit").with_context(raw_len as u64));
    }
    let out = inflate_block(codec, payload, output_limit)?;
    if out.len() != raw_len {
        return Err(Error::codec("frame length mismatch").with_context(out.len() as u64));
    }
    if adler32(&out) != expect_adler {
        return Err(Error::codec("frame checksum mismatch"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::rle;

    #[test]
    fn wrap_unwrap_rle() {
        let raw = vec![5u8; 40];
        let compressed = rle::encode(&raw);
        let frame = wrap(Codec::Rle, &raw, &compressed);
        let out = unwrap(&frame, 4096).unwrap();
        assert_eq!(out, raw);
    }

    #[test]
    fn checksum_mismatch_detected() {
        let raw = vec![1u8, 2, 3, 4];
        let compressed = crate::codec::rle::encode(&raw);
        let mut frame = wrap(Codec::Rle, &raw, &compressed);
        frame[5] ^= 0xFF; // corrupt the adler field
        assert!(unwrap(&frame, 4096).is_err());
    }
}
