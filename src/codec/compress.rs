//! Codec selection and the per-session compression configuration.
//!
//! `COMPRESS_CONFIG` installs the codec a session will use for subsequently
//! flagged payloads; `COMPRESS_DATA` and any message with the compressed flag
//! runs through [`CompressorState::inflate`]. The codec is initialised lazily
//! so a session that never compresses pays nothing.

use crate::codec::{delta, huffman, rle};
use crate::error::{Error, Result};

/// The compression algorithm applied to a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    None,
    Rle,
    Delta,
    Huffman,
}

impl Codec {
    pub fn from_code(code: u8) -> Result<Codec> {
        Ok(match code {
            0x00 => Codec::None,
            0x01 => Codec::Rle,
            0x02 => Codec::Delta,
            0x03 => Codec::Huffman,
            // 0xFF is a valid "pass-through" configuration: the peer announces
            // that it *may* compress but this session should treat blocks as
            // raw until told otherwise.
            0xFF => Codec::None,
            other => return Err(Error::codec("unknown codec code").with_context(other as u64)),
        })
    }

    pub fn code(self) -> u8 {
        match self {
            Codec::None => 0x00,
            Codec::Rle => 0x01,
            Codec::Delta => 0x02,
            Codec::Huffman => 0x03,
        }
    }
}

/// Per-session compression state, configured lazily.
pub struct CompressorState {
    codec: Option<Codec>,
    output_limit: usize,
    inflations: u64,
}

impl CompressorState {
    pub fn new(output_limit: usize) -> CompressorState {
        CompressorState { codec: None, output_limit: output_limit.max(1024), inflations: 0 }
    }

    pub fn is_configured(&self) -> bool {
        self.codec.is_some()
    }

    pub fn configured_codec(&self) -> Option<Codec> {
        self.codec
    }

    /// Handle a `COMPRESS_CONFIG` message.
    pub fn configure(&mut self, codec_code: u8, output_limit: usize) -> Result<()> {
        let codec = Codec::from_code(codec_code)?;
        self.codec = Some(codec);
        if output_limit != 0 {
            self.output_limit = output_limit.min(1 << 24);
        }
        Ok(())
    }

    /// Inflate a block using the configured codec. If no codec was configured,
    /// the block is treated as raw and returned as-is.
    pub fn inflate(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        self.inflations += 1;
        match self.codec.unwrap_or(Codec::None) {
            Codec::None => Ok(input.to_vec()),
            Codec::Rle => rle::decode(input, self.output_limit),
            Codec::Delta => {
                let vals = delta::decode(input, self.output_limit / 8)?;
                let mut out = Vec::with_capacity(vals.len() * 8);
                for v in vals {
                    out.extend_from_slice(&v.to_be_bytes());
                }
                Ok(out)
            }
            Codec::Huffman => inflate_huffman(input, self.output_limit),
        }
    }
}

/// Standalone codec dispatch used by `COMPRESSED_BLOCK`, where the codec is
/// named in the message itself rather than the session config.
pub fn inflate_block(codec: Codec, input: &[u8], output_limit: usize) -> Result<Vec<u8>> {
    match codec {
        Codec::None => Ok(input.to_vec()),
        Codec::Rle => rle::decode(input, output_limit),
        Codec::Delta => {
            let vals = delta::decode(input, output_limit / 8)?;
            let mut out = Vec::with_capacity(vals.len() * 8);
            for v in vals {
                out.extend_from_slice(&v.to_be_bytes());
            }
            Ok(out)
        }
        Codec::Huffman => inflate_huffman(input, output_limit),
    }
}

fn inflate_huffman(input: &[u8], output_limit: usize) -> Result<Vec<u8>> {
    // Layout: [4B declared output len][128B length nibbles][bitstream].
    if input.len() < 4 + 128 {
        return Err(Error::codec("huffman block too short"));
    }
    let declared = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if declared > output_limit {
        return Err(Error::limit("huffman output over limit").with_context(declared as u64));
    }
    let mut lengths = [0u8; 256];
    let nibbles = &input[4..4 + 128];
    for (i, &packed) in nibbles.iter().enumerate() {
        lengths[i * 2] = packed >> 4;
        lengths[i * 2 + 1] = packed & 0x0f;
    }
    let dec = huffman::Decoder::from_lengths(&lengths)?;
    dec.decode(&input[4 + 128..], declared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_without_config() {
        let mut c = CompressorState::new(4096);
        assert!(!c.is_configured());
        assert_eq!(c.inflate(b"hello").unwrap(), b"hello");
    }

    #[test]
    fn rle_via_config() {
        let mut c = CompressorState::new(4096);
        c.configure(Codec::Rle.code(), 0).unwrap();
        let enc = rle::encode(&vec![9u8; 20]);
        assert_eq!(c.inflate(&enc).unwrap(), vec![9u8; 20]);
    }

    #[test]
    fn passthrough_mode_0xff() {
        let mut c = CompressorState::new(4096);
        c.configure(0xFF, 0).unwrap();
        assert_eq!(c.configured_codec(), Some(Codec::None));
        assert_eq!(c.inflate(b"raw").unwrap(), b"raw");
    }
}
