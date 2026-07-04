//! A bounds-checked cursor over a byte slice.
//!
//! The wire code reads big-endian integers all over the place; funnelling that
//! through one type keeps the "did I check the length first" question in a
//! single, auditable location. Every accessor returns a `Result` rather than
//! panicking so a truncated frame is an ordinary error, not a crash.

use crate::error::{Error, Kind, Result};

#[derive(Debug, Clone)]
pub struct ByteReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        ByteReader { buf, pos: 0 }
    }

    pub fn at(buf: &'a [u8], pos: usize) -> Self {
        ByteReader { buf, pos: pos.min(buf.len()) }
    }

    #[inline]
    pub fn position(&self) -> usize {
        self.pos
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    #[inline]
    pub fn total_len(&self) -> usize {
        self.buf.len()
    }

    /// Reposition the cursor absolutely. Clamps to the buffer end.
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.buf.len());
    }

    /// Advance by `n`, erroring if that would run past the end.
    pub fn skip(&mut self, n: usize) -> Result<()> {
        if self.remaining() < n {
            return Err(Error::malformed("skip past end").with_context(n as u64));
        }
        self.pos += n;
        Ok(())
    }

    fn need(&self, n: usize) -> Result<()> {
        if self.remaining() < n {
            Err(Error::new(Kind::Malformed, "short read").with_context(n as u64))
        } else {
            Ok(())
        }
    }

    pub fn u8(&mut self) -> Result<u8> {
        self.need(1)?;
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub fn u16(&mut self) -> Result<u16> {
        self.need(2)?;
        let v = u16::from_be_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.need(4)?;
        let s = &self.buf[self.pos..self.pos + 4];
        let v = u32::from_be_bytes([s[0], s[1], s[2], s[3]]);
        self.pos += 4;
        Ok(v)
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64> {
        self.need(8)?;
        let s = &self.buf[self.pos..self.pos + 8];
        let mut b = [0u8; 8];
        b.copy_from_slice(s);
        let v = u64::from_be_bytes(b);
        self.pos += 8;
        Ok(v)
    }

    /// Borrow `n` bytes without copying, advancing the cursor.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n)?;
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// Borrow all remaining bytes.
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }

    /// Peek at the next byte without consuming it.
    pub fn peek_u8(&self) -> Result<u8> {
        self.need(1)?;
        Ok(self.buf[self.pos])
    }

    /// Borrow the whole underlying buffer (used by absolute-offset readers).
    pub fn underlying(&self) -> &'a [u8] {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_big_endian() {
        let data = [0x00, 0x01, 0x00, 0x00, 0x00, 0x02];
        let mut r = ByteReader::new(&data);
        assert_eq!(r.u16().unwrap(), 1);
        assert_eq!(r.u32().unwrap(), 2);
        assert!(r.is_empty());
    }

    #[test]
    fn short_read_errors() {
        let data = [0x00];
        let mut r = ByteReader::new(&data);
        assert!(r.u32().is_err());
    }

    #[test]
    fn take_and_rest() {
        let data = [1, 2, 3, 4, 5];
        let mut r = ByteReader::new(&data);
        assert_eq!(r.take(2).unwrap(), &[1, 2]);
        assert_eq!(r.rest(), &[3, 4, 5]);
    }
}
