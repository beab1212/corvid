//! MSB-first bit reader and writer.
//!
//! Shared by the Huffman and LZ codecs. The reader reports an error on
//! underrun rather than returning zero bits, so a truncated stream is a clean
//! failure instead of silent corruption.

use crate::error::{Error, Result};

pub struct BitWriter {
    bytes: Vec<u8>,
    cur: u8,
    nbits: u8,
}

impl BitWriter {
    pub fn new() -> BitWriter {
        BitWriter { bytes: Vec::new(), cur: 0, nbits: 0 }
    }

    pub fn write_bit(&mut self, bit: u32) {
        self.cur = (self.cur << 1) | (bit as u8 & 1);
        self.nbits += 1;
        if self.nbits == 8 {
            self.bytes.push(self.cur);
            self.cur = 0;
            self.nbits = 0;
        }
    }

    pub fn write_bits(&mut self, value: u32, count: u8) {
        for i in (0..count).rev() {
            self.write_bit((value >> i) & 1);
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        if self.nbits > 0 {
            self.cur <<= 8 - self.nbits;
            self.bytes.push(self.cur);
        }
        self.bytes
    }
}

impl Default for BitWriter {
    fn default() -> Self {
        BitWriter::new()
    }
}

pub struct BitReader<'a> {
    bytes: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> BitReader<'a> {
        BitReader { bytes, byte_pos: 0, bit_pos: 0 }
    }

    pub fn bits_remaining(&self) -> usize {
        (self.bytes.len() - self.byte_pos) * 8 - self.bit_pos as usize
    }

    pub fn read_bit(&mut self) -> Result<u32> {
        if self.byte_pos >= self.bytes.len() {
            return Err(Error::codec("bit reader underrun"));
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

    pub fn read_bits(&mut self, count: u8) -> Result<u32> {
        let mut v = 0u32;
        for _ in 0..count {
            v = (v << 1) | self.read_bit()?;
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_roundtrip() {
        let mut w = BitWriter::new();
        w.write_bits(0b101, 3);
        w.write_bits(0b1111_0000, 8);
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_bits(8).unwrap(), 0b1111_0000);
    }

    #[test]
    fn underrun_errors() {
        let mut r = BitReader::new(&[0x00]);
        assert!(r.read_bits(9).is_err());
    }
}
