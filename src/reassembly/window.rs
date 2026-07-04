//! A fixed-size sliding window over a byte stream.
//!
//! Stream payloads are placed into a circular buffer indexed by sequence
//! number modulo the window size. Readers pull contiguous ranges back out. The
//! window is backed by an owned `Vec<u8>`; all indexing is bounds-checked
//! against the window length.

use crate::error::{Error, Result};

pub struct Window {
    buf: Vec<u8>,
    size: usize,
    /// Highest sequence number written, for lag diagnostics.
    high_seq: u64,
}

impl Window {
    pub fn new(size: usize) -> Window {
        let size = size.next_power_of_two().max(64);
        Window { buf: vec![0u8; size], size, high_seq: 0 }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn high_seq(&self) -> u64 {
        self.high_seq
    }

    #[inline]
    fn slot(&self, seq: u64) -> usize {
        (seq % self.size as u64) as usize
    }

    /// Place `data` at `seq`, wrapping within the window. Data that would wrap
    /// past the physical end of the buffer is split across the boundary.
    pub fn place(&mut self, seq: u64, data: &[u8]) -> Result<()> {
        if data.len() > self.size {
            return Err(Error::limit("packet larger than window"));
        }
        self.high_seq = self.high_seq.max(seq + data.len() as u64);
        let start = self.slot(seq);
        let first = (self.size - start).min(data.len());
        self.buf[start..start + first].copy_from_slice(&data[..first]);
        if first < data.len() {
            let rest = data.len() - first;
            self.buf[..rest].copy_from_slice(&data[first..]);
        }
        Ok(())
    }

    /// Read `len` bytes starting at sequence `seq`, following the wrap.
    pub fn read(&self, seq: u64, len: usize) -> Result<Vec<u8>> {
        if len > self.size {
            return Err(Error::limit("read larger than window").with_context(len as u64));
        }
        let start = self.slot(seq);
        let mut out = Vec::with_capacity(len);
        let first = (self.size - start).min(len);
        out.extend_from_slice(&self.buf[start..start + first]);
        if first < len {
            let rest = len - first;
            out.extend_from_slice(&self.buf[..rest]);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_and_read_wrapping() {
        let mut w = Window::new(64);
        // Write near the end so it wraps.
        w.place(60, b"abcdef").unwrap();
        let out = w.read(60, 6).unwrap();
        assert_eq!(&out, b"abcdef");
    }

    #[test]
    fn oversize_rejected() {
        let mut w = Window::new(64);
        assert!(w.place(0, &vec![0u8; 65]).is_err());
        assert!(w.read(0, 65).is_err());
    }
}
