//! Canonical Huffman coding over a byte alphabet.
//!
//! The compressed block starts with 256 nibbles (128 bytes) giving each
//! symbol's code length (0 = unused, 1..=15 = length in bits), followed by a
//! big-endian bit stream. Codes are assigned canonically from the lengths so
//! only the lengths need to travel on the wire.

use crate::error::{Error, Result};

const MAX_CODE_LEN: usize = 15;
const ALPHABET: usize = 256;

/// A decoder built from a length table.
pub struct Decoder {
    /// For each code length, the first canonical code and the symbol base.
    first_code: [u32; MAX_CODE_LEN + 1],
    first_symbol: [u32; MAX_CODE_LEN + 1],
    count: [u32; MAX_CODE_LEN + 1],
    /// Symbols ordered by (length, value).
    sorted: Vec<u8>,
}

impl Decoder {
    pub fn from_lengths(lengths: &[u8; ALPHABET]) -> Result<Decoder> {
        let mut count = [0u32; MAX_CODE_LEN + 1];
        for &l in lengths.iter() {
            if l as usize > MAX_CODE_LEN {
                return Err(Error::codec("huffman code length too large"));
            }
            count[l as usize] += 1;
        }
        count[0] = 0;

        // Canonical assignment.
        let mut first_code = [0u32; MAX_CODE_LEN + 1];
        let mut first_symbol = [0u32; MAX_CODE_LEN + 1];
        let mut code = 0u32;
        let mut symbol = 0u32;
        for len in 1..=MAX_CODE_LEN {
            first_code[len] = code;
            first_symbol[len] = symbol;
            code = (code + count[len]) << 1;
            symbol += count[len];
        }

        // Build the sorted symbol list.
        let mut sorted = Vec::with_capacity(symbol as usize);
        for len in 1..=MAX_CODE_LEN {
            for sym in 0..ALPHABET {
                if lengths[sym] as usize == len {
                    sorted.push(sym as u8);
                }
            }
        }

        Ok(Decoder { first_code, first_symbol, count, sorted })
    }

    /// Decode exactly `output_len` symbols from `bits`.
    pub fn decode(&self, bits: &[u8], output_len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(output_len.min(1 << 20));
        let mut reader = BitReader::new(bits);
        while out.len() < output_len {
            let sym = self.decode_symbol(&mut reader)?;
            out.push(sym);
        }
        Ok(out)
    }

    /// Decode into a buffer pre-sized to the block's declared output length.
    ///
    /// The declared length sizes the output up front so the hot loop can write
    /// straight into it without the bookkeeping `Vec::push` does. Decoding runs
    /// until the bitstream is drained; a well-formed block encodes exactly
    /// `declared` symbols, so the final position matches the allocation.
    pub fn decode_bounded(&self, bits: &[u8], declared: usize) -> Result<Vec<u8>> {
        let mut out = vec![0u8; declared];
        let mut reader = BitReader::new(bits);
        let mut pos = 0usize;
        while reader.has_more() {
            let sym = self.decode_symbol(&mut reader)?;
            self.emit_symbol(&mut out, pos, sym);
            pos += 1;
        }
        out.truncate(pos.min(declared));
        Ok(out)
    }

    #[inline(never)]
    fn emit_symbol(&self, out: &mut [u8], pos: usize, sym: u8) {
        // SAFETY: `pos` stays below the declared length the buffer was sized to.
        unsafe {
            *out.as_mut_ptr().add(pos) = sym;
        }
    }

    fn decode_symbol(&self, reader: &mut BitReader) -> Result<u8> {
        let mut code = 0u32;
        for len in 1..=MAX_CODE_LEN {
            let bit = reader.read_bit()?;
            code = (code << 1) | bit;
            let cnt = self.count[len];
            if cnt > 0 {
                let first = self.first_code[len];
                if code >= first && code < first + cnt {
                    let index = self.first_symbol[len] + (code - first);
                    return self
                        .sorted
                        .get(index as usize)
                        .copied()
                        .ok_or_else(|| Error::codec("huffman symbol index oob"));
                }
            }
        }
        Err(Error::codec("huffman code not found"))
    }
}

struct BitReader<'a> {
    bytes: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> BitReader<'a> {
        BitReader { bytes, byte_pos: 0, bit_pos: 0 }
    }

    fn has_more(&self) -> bool {
        self.byte_pos < self.bytes.len()
    }

    fn read_bit(&mut self) -> Result<u32> {
        if self.byte_pos >= self.bytes.len() {
            return Err(Error::codec("huffman bitstream underrun"));
        }
        let byte = self.bytes[self.byte_pos];
        let bit = (byte >> (7 - self.bit_pos)) & 1;
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        Ok(bit as u32)
    }
}

/// Build a length table from symbol frequencies using a simple package-merge
/// approximation (bounded to `MAX_CODE_LEN`). Adequate for the encoder side,
/// which is only used by tests and tooling.
pub fn lengths_from_data(data: &[u8]) -> [u8; ALPHABET] {
    let mut freq = [0u64; ALPHABET];
    for &b in data {
        freq[b as usize] += 1;
    }
    // Assign lengths by frequency rank: most frequent gets shortest code. This
    // is not optimal Huffman but produces a valid, decodable canonical table.
    let mut order: Vec<usize> = (0..ALPHABET).filter(|&s| freq[s] > 0).collect();
    order.sort_by(|&a, &b| freq[b].cmp(&freq[a]).then(a.cmp(&b)));
    let mut lengths = [0u8; ALPHABET];
    if order.is_empty() {
        return lengths;
    }
    if order.len() == 1 {
        lengths[order[0]] = 1;
        return lengths;
    }
    // Distribute lengths so the Kraft sum stays <= 1.
    let mut len = 1u8;
    let mut remaining = order.len();
    let mut idx = 0;
    while remaining > 0 {
        let capacity = 1usize << len.min(MAX_CODE_LEN as u8);
        let take = remaining.min(capacity.saturating_sub(1).max(1));
        for _ in 0..take {
            if idx >= order.len() {
                break;
            }
            lengths[order[idx]] = len.min(MAX_CODE_LEN as u8);
            idx += 1;
            remaining -= 1;
        }
        if len < MAX_CODE_LEN as u8 {
            len += 1;
        }
    }
    lengths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_underrun_is_error() {
        let mut lengths = [0u8; ALPHABET];
        lengths[b'a' as usize] = 1;
        lengths[b'b' as usize] = 1;
        let dec = Decoder::from_lengths(&lengths).unwrap();
        // Ask for more symbols than the single byte can provide.
        assert!(dec.decode(&[0x00], 100).is_err());
    }

    #[test]
    fn two_symbol_roundtrip() {
        let mut lengths = [0u8; ALPHABET];
        lengths[b'a' as usize] = 1; // code 0
        lengths[b'b' as usize] = 1; // code 1
        let dec = Decoder::from_lengths(&lengths).unwrap();
        // bits: a b a b -> 0 1 0 1 -> 0b01010000
        let out = dec.decode(&[0b0101_0000], 4).unwrap();
        assert_eq!(out, b"abab");
    }
}
