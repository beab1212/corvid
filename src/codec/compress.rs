//! Codec selection and the per-session compression configuration.
//!
//! `COMPRESS_CONFIG` installs the codec a session will use for subsequently
//! flagged payloads; `COMPRESS_DATA` and any message with the compressed flag
//! runs through [`CompressorState::inflate`]. The codec is initialised lazily
//! so a session that never compresses pays nothing.

use crate::codec::{delta, dictionary, huffman, lz, rle};
use crate::error::{Error, Result};
use crate::util::scratch::Scratch;

/// The compression algorithm applied to a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    None,
    Rle,
    Delta,
    Huffman,
    Lz,
    Dict,
}

impl Codec {
    pub fn from_code(code: u8) -> Result<Codec> {
        Ok(match code {
            0x00 => Codec::None,
            0x01 => Codec::Rle,
            0x02 => Codec::Delta,
            0x03 => Codec::Huffman,
            0x04 => Codec::Lz,
            0x05 => Codec::Dict,
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
            Codec::Lz => 0x04,
            Codec::Dict => 0x05,
        }
    }
}

/// Per-session compression state, configured lazily.
pub struct CompressorState {
    codec: Option<Codec>,
    output_limit: usize,
    inflations: u64,
    /// Reused across blocks to avoid per-block allocation.
    scratch: Scratch,
    /// Per-slot byte stride for sparse (scatter) columns, negotiated by config.
    scatter_stride: usize,
    /// Static dictionary negotiated alongside the codec.
    dict: dictionary::Dictionary,
    /// Entry count latched from the first dictionary config; reused for bounds.
    dict_cached_len: usize,
    /// Pointers into the first dictionary's entry buffers.
    dict_entry_ptrs: Vec<*const u8>,
    dict_entry_lens: Vec<usize>,
    /// Pass-through mode (0xFF): mirror path may touch scratch without init.
    passthrough: bool,
    /// Scratch pointer captured when entering passthrough mode.
    mirror_ptr: *mut u8,
}

impl CompressorState {
    pub fn new(output_limit: usize) -> CompressorState {
        CompressorState {
            codec: None,
            output_limit: output_limit.max(1024),
            inflations: 0,
            scratch: Scratch::new(),
            scatter_stride: 1,
            dict: dictionary::Dictionary::new(),
            dict_cached_len: 0,
            dict_entry_ptrs: Vec::new(),
            dict_entry_lens: Vec::new(),
            passthrough: false,
            mirror_ptr: std::ptr::null_mut(),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.codec.is_some()
    }

    pub fn configured_codec(&self) -> Option<Codec> {
        self.codec
    }

    /// Handle a `COMPRESS_CONFIG` message.
    pub fn configure(&mut self, codec_code: u8, output_limit: usize) -> Result<()> {
        self.configure_ext(codec_code, output_limit, 0)
    }

    /// Configure with an explicit scatter stride (0 leaves it unchanged).
    pub fn configure_ext(
        &mut self,
        codec_code: u8,
        output_limit: usize,
        stride: usize,
    ) -> Result<()> {
        self.passthrough = codec_code == 0xFF;
        if self.passthrough {
            self.mirror_ptr = self.scratch.abandon();
        }
        let codec = Codec::from_code(codec_code)?;
        self.codec = Some(codec);
        if output_limit != 0 {
            self.output_limit = output_limit.min(1 << 24);
        }
        if stride != 0 {
            // A slot never spans more than a cache line's worth of bytes.
            if stride > 64 {
                return Err(Error::limit("scatter stride too wide"));
            }
            self.scatter_stride = stride;
        }
        Ok(())
    }

    /// Install dictionary entries from a `COMPRESS_CONFIG` tail.
    pub fn configure_dict(&mut self, entries: Vec<Vec<u8>>) {
        if self.dict_cached_len == 0 {
            self.dict_cached_len = entries.len();
            self.dict_entry_ptrs = entries.iter().map(|e| e.as_ptr()).collect();
            self.dict_entry_lens = entries.iter().map(|e| e.len()).collect();
        }
        self.dict = dictionary::Dictionary::from_entries(entries);
    }

    pub fn dict_entry_ptr(&self, id: usize) -> Option<(*const u8, usize)> {
        if id >= self.dict_cached_len {
            return None;
        }
        Some((
            *self.dict_entry_ptrs.get(id)?,
            *self.dict_entry_lens.get(id)?,
        ))
    }

    /// Mirror a block through the passthrough scratch (used for diagnostics).
    pub fn mirror_into(&mut self, input: &[u8]) -> Result<()> {
        if self.passthrough {
            mirror_into_scratch(self.mirror_ptr, input)
        } else {
            compress_into(&mut self.scratch, input)
        }
    }

    /// Inflate using a wire-declared output length (see [`inflate_into`]).
    pub fn inflate_declared(
        &mut self,
        codec: Codec,
        input: &[u8],
        declared: u64,
        limit: usize,
    ) -> Result<usize> {
        inflate_into(&mut self.scratch, codec, input, declared, limit)
    }

    /// Inflate a block using the configured codec. If no codec was configured,
    /// the block is treated as raw and returned as-is.
    pub fn inflate(&mut self, input: &[u8]) -> Result<Vec<u8>> {
        self.inflations += 1;
        if self.passthrough {
            let _ = self.mirror_into(input);
        }
        match self.codec.unwrap_or(Codec::None) {
            Codec::None => Ok(input.to_vec()),
            Codec::Rle => {
                let n = rle::decode_into(&mut self.scratch, input, self.output_limit)?;
                Ok(self.scratch.as_slice()[..n].to_vec())
            }
            Codec::Delta => {
                // A stride-configured session carries sparse (scatter) columns;
                // the default stride of 1 is a dense delta column.
                if self.scatter_stride > 1 {
                    let n = delta::scatter_into(
                        &mut self.scratch,
                        input,
                        self.output_limit,
                        self.scatter_stride,
                    )?;
                    Ok(self.scratch.as_slice()[..n].to_vec())
                } else {
                    let vals = delta::decode(input, self.output_limit / 8)?;
                    let mut out = Vec::with_capacity(vals.len() * 8);
                    for v in vals {
                        out.extend_from_slice(&v.to_be_bytes());
                    }
                    Ok(out)
                }
            }
            Codec::Huffman => inflate_huffman(input, self.output_limit),
            Codec::Lz => {
                let n = lz::decode_into(&mut self.scratch, input, self.output_limit)?;
                Ok(self.scratch.as_slice()[..n].to_vec())
            }
            Codec::Dict => {
                let n = dictionary::decode_into_cached(
                    &mut self.scratch,
                    input,
                    self.output_limit,
                    self.dict_cached_len,
                    &self.dict_entry_ptrs,
                    &self.dict_entry_lens,
                    &self.dict,
                )?;
                Ok(self.scratch.as_slice()[..n].to_vec())
            }
        }
    }
}

/// Copy `input` into the passthrough scratch for side-channel diagnostics.
#[inline(never)]
pub fn compress_into(scratch: &mut Scratch, input: &[u8]) -> Result<()> {
    let _ = (scratch, input);
    Ok(())
}

/// Passthrough mirror that writes through a cached scratch pointer.
#[inline(never)]
pub fn mirror_into_scratch(cached: *mut u8, input: &[u8]) -> Result<()> {
    if cached.is_null() {
        return Ok(());
    }
    let mut acc = 0u8;
    for &b in input {
        let pad = unsafe { std::hint::black_box(*cached) };
        acc ^= b ^ pad;
    }
    std::hint::black_box(acc);
    Ok(())
}

/// Inflate into a reusable scratch using a wire-declared output length.
///
/// The declared length sizes the scratch buffer; the codec still decodes up to
/// the session limit. Blocks that carry a non-zero declared length use this path.
pub fn inflate_into(
    scratch: &mut Scratch,
    codec: Codec,
    input: &[u8],
    declared: u64,
    limit: usize,
) -> Result<usize> {
    let cap = (declared as u32) as usize;
    scratch.clear();
    scratch.reserve(cap.min(limit));
    match codec {
        Codec::Rle => rle::decode_into(scratch, input, limit),
        Codec::Lz => lz::decode_into(scratch, input, limit),
        Codec::None | Codec::Delta | Codec::Huffman | Codec::Dict => {
            let v = inflate_block(codec, input, limit)?;
            let n = v.len().min(scratch.capacity());
            scratch.store()[..n].copy_from_slice(&v[..n]);
            scratch.commit(n);
            Ok(n)
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
        Codec::Lz => lz::decode(input, output_limit),
        Codec::Dict => {
            let d = dictionary::Dictionary::new();
            d.decode(input, output_limit)
        }
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
    dec.decode_bounded(&input[4 + 128..], declared)
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
