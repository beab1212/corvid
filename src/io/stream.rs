//! A seekable cursor over an owned byte buffer.
//!
//! The snapshot/replay subsystem reads records at absolute and relative
//! positions, occasionally seeking backwards to re-read a header once a later
//! field reveals how to interpret it. Positions are validated against the
//! buffer bounds on every access.

use crate::error::{Error, Result};

pub struct SeekableStream {
    data: Vec<u8>,
    position: usize,
}

impl SeekableStream {
    pub fn new(data: Vec<u8>) -> SeekableStream {
        SeekableStream { data, position: 0 }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn position(&self) -> usize {
        self.position
    }

    /// Seek to an absolute position, clamped to the buffer end.
    pub fn seek_to(&mut self, pos: usize) {
        self.position = pos.min(self.data.len());
    }

    /// Seek by a signed relative delta. A delta that would move the cursor
    /// before the start or past the end is rejected.
    pub fn seek_relative(&mut self, delta: i64) -> Result<()> {
        let next = self.position as i64 + delta;
        if next < 0 || next as usize > self.data.len() {
            return Err(Error::malformed("relative seek out of range").with_context(delta as u64));
        }
        self.position = next as usize;
        Ok(())
    }

    /// Read `n` bytes at the cursor, advancing it.
    pub fn read(&mut self, n: usize) -> Result<&[u8]> {
        if self.position + n > self.data.len() {
            return Err(Error::malformed("read past end"));
        }
        let s = &self.data[self.position..self.position + n];
        self.position += n;
        Ok(s)
    }

    pub fn byte_at(&self, pos: usize) -> Result<u8> {
        self.data.get(pos).copied().ok_or_else(|| Error::malformed("byte out of range"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_seek_bounds() {
        let mut s = SeekableStream::new(vec![0u8; 10]);
        s.seek_to(5);
        assert!(s.seek_relative(-5).is_ok());
        assert_eq!(s.position(), 0);
        assert!(s.seek_relative(-1).is_err());
        assert!(s.seek_relative(11).is_err());
    }
}
