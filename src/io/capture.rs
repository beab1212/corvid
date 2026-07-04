//! A simple capture-file format for recording CVWP streams for replay.
//!
//! ```text
//! header:
//!   magic     : 4 bytes = "CVCP"
//!   version   : 1 byte
//!   reserved  : 3 bytes
//! record (repeated):
//!   timestamp : 8 bytes (big-endian, ms)
//!   length    : 4 bytes (big-endian)
//!   stream    : length bytes (a full CVWP stream)
//! ```

use crate::error::{Error, Result};
use crate::util::{ByteReader, ByteWriter};

pub const CAP_MAGIC: [u8; 4] = *b"CVCP";
pub const CAP_VERSION: u8 = 1;

/// A single captured stream with its arrival timestamp.
#[derive(Debug, Clone)]
pub struct CaptureRecord {
    pub timestamp_ms: u64,
    pub stream: Vec<u8>,
}

/// Write a capture file from a set of records.
pub fn write(records: &[CaptureRecord]) -> Vec<u8> {
    let mut w = ByteWriter::new();
    w.bytes(&CAP_MAGIC).u8(CAP_VERSION).bytes(&[0, 0, 0]);
    for r in records {
        w.u64(r.timestamp_ms).u32(r.stream.len() as u32).bytes(&r.stream);
    }
    w.into_vec()
}

/// An iterator-style reader over a capture file.
pub struct CaptureReader<'a> {
    r: ByteReader<'a>,
    max_stream: usize,
}

impl<'a> CaptureReader<'a> {
    pub fn open(data: &'a [u8]) -> Result<CaptureReader<'a>> {
        if data.len() < 8 {
            return Err(Error::malformed("capture file too short"));
        }
        if data[0..4] != CAP_MAGIC {
            return Err(Error::malformed("bad capture magic"));
        }
        if data[4] != CAP_VERSION {
            return Err(Error::malformed("unsupported capture version"));
        }
        let mut r = ByteReader::new(data);
        r.skip(8)?;
        Ok(CaptureReader { r, max_stream: 8 * 1024 * 1024 })
    }

    /// Read the next record, or `None` at end of file.
    pub fn next_record(&mut self) -> Result<Option<CaptureRecord>> {
        if self.r.is_empty() {
            return Ok(None);
        }
        let timestamp_ms = self.r.u64()?;
        let len = self.r.u32()? as usize;
        if len > self.max_stream {
            return Err(Error::limit("captured stream too large").with_context(len as u64));
        }
        let stream = self.r.take(len)?.to_vec();
        Ok(Some(CaptureRecord { timestamp_ms, stream }))
    }

    /// Collect all records into a vector.
    pub fn read_all(mut self) -> Result<Vec<CaptureRecord>> {
        let mut out = Vec::new();
        while let Some(rec) = self.next_record()? {
            out.push(rec);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_roundtrip() {
        let recs = vec![
            CaptureRecord { timestamp_ms: 100, stream: vec![1, 2, 3] },
            CaptureRecord { timestamp_ms: 200, stream: vec![4, 5] },
        ];
        let buf = write(&recs);
        let back = CaptureReader::open(&buf).unwrap().read_all().unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].timestamp_ms, 200);
        assert_eq!(back[0].stream, vec![1, 2, 3]);
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(CaptureReader::open(b"XXXX\x01\x00\x00\x00").is_err());
    }
}
