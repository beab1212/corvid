//! Length-delimited batching of multiple CVWP streams.
//!
//! A capture or a batched upload concatenates several independent streams, each
//! prefixed with a 4-byte big-endian length. This reader splits such a buffer
//! into per-stream slices without copying, so the caller can feed each stream
//! to a session in turn.

use crate::error::{Error, Result};

pub struct BatchReader<'a> {
    buf: &'a [u8],
    pos: usize,
    max_stream: usize,
}

impl<'a> BatchReader<'a> {
    pub fn new(buf: &'a [u8]) -> BatchReader<'a> {
        BatchReader { buf, pos: 0, max_stream: 8 * 1024 * 1024 }
    }

    pub fn with_max(buf: &'a [u8], max_stream: usize) -> BatchReader<'a> {
        BatchReader { buf, pos: 0, max_stream }
    }

    /// Return the next stream slice, or `None` at end of buffer.
    pub fn next_stream(&mut self) -> Result<Option<&'a [u8]>> {
        if self.pos >= self.buf.len() {
            return Ok(None);
        }
        if self.pos + 4 > self.buf.len() {
            return Err(Error::malformed("batch length truncated"));
        }
        let len = u32::from_be_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ]) as usize;
        self.pos += 4;
        if len > self.max_stream {
            return Err(Error::limit("batched stream too large").with_context(len as u64));
        }
        if self.pos + len > self.buf.len() {
            return Err(Error::malformed("batched stream runs past buffer"));
        }
        let s = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        Ok(Some(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_two_streams() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&3u32.to_be_bytes());
        buf.extend_from_slice(b"abc");
        buf.extend_from_slice(&2u32.to_be_bytes());
        buf.extend_from_slice(b"de");
        let mut r = BatchReader::new(&buf);
        assert_eq!(r.next_stream().unwrap(), Some(&b"abc"[..]));
        assert_eq!(r.next_stream().unwrap(), Some(&b"de"[..]));
        assert_eq!(r.next_stream().unwrap(), None);
    }

    #[test]
    fn truncated_stream_errors() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&10u32.to_be_bytes());
        buf.extend_from_slice(b"short");
        let mut r = BatchReader::new(&buf);
        assert!(r.next_stream().is_err());
    }
}
