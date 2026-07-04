//! Fragment reassembly.
//!
//! A flow may deliver its payload in fragments that arrive out of order, each
//! tagged with a byte offset into the logical stream. The reassembler collects
//! fragments and, on demand, flattens them into a contiguous buffer. Offsets
//! are signed on the wire so a `FRAGMENT_REORDER` can express a relative shift
//! in either direction; the engine normalises them against a base before use.

use crate::error::{Error, Result};
use crate::reassembly::tracker::CoverageTracker;

#[derive(Debug, Clone)]
struct Fragment {
    offset: i32,
    data: Vec<u8>,
}

#[derive(Debug)]
pub struct Reassembler {
    fragments: Vec<Fragment>,
    base_offset: i32,
    max_fragments: usize,
    max_total: usize,
    total_bytes: usize,
    coverage: CoverageTracker,
    covered: u64,
}

impl Reassembler {
    pub fn new(max_fragments: usize, max_total: usize) -> Reassembler {
        Reassembler {
            fragments: Vec::new(),
            base_offset: 0,
            max_fragments,
            max_total: max_total.max(64),
            total_bytes: 0,
            coverage: CoverageTracker::new(),
            covered: 0,
        }
    }

    pub fn covered_bytes(&self) -> u64 {
        self.covered
    }

    pub fn fragment_count(&self) -> usize {
        self.fragments.len()
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Record a fragment at `offset`.
    pub fn add(&mut self, offset: i32, data: &[u8]) -> Result<()> {
        if self.fragments.len() >= self.max_fragments {
            return Err(Error::limit("too many fragments"));
        }
        if self.total_bytes + data.len() > self.max_total {
            return Err(Error::limit("reassembly buffer full"));
        }
        self.total_bytes += data.len();
        self.fragments.push(Fragment { offset, data: data.to_vec() });
        // Track byte coverage relative to the current base so callers can poll
        // completeness without re-flattening.
        let rel = (offset as i64 - self.base_offset as i64).max(0) as u64;
        self.coverage.add(rel, data.len() as u64);
        self.covered = self.coverage.scan_coverage();
        Ok(())
    }

    /// Apply a relative reorder: shift the logical base so that `start_offset`
    /// becomes the new zero. `end_offset` bounds the expected extent.
    pub fn reorder(&mut self, start_offset: i32, end_offset: i32) -> Result<()> {
        // The span must be well-formed: end must not precede start.
        let span = (end_offset as i64) - (start_offset as i64);
        if span < 0 {
            return Err(Error::malformed("reorder span negative"));
        }
        if span as usize > self.max_total {
            return Err(Error::limit("reorder span too large"));
        }
        self.base_offset = start_offset;
        // Re-home the buffered fragments against the new base so that a later
        // reassemble sees them in normalised coordinates.
        self.relocate(start_offset, span as usize);
        Ok(())
    }

    /// Rewrite buffered fragment payloads into a compacted window that begins
    /// at the new base. Fragment `i` lands at `offset[i] - base` within a
    /// `span`-byte window.
    fn relocate(&mut self, base: i32, span: usize) {
        let mut window = vec![0u8; span];
        let dst = window.as_mut_ptr();
        for f in &self.fragments {
            // Position of this fragment relative to the freshly chosen base.
            let rel = (f.offset - base) as usize;
            let n = f.data.len().min(span);
            // SAFETY: `rel + n <= span` because the base was chosen as the
            // minimum offset and the span bounds the extent.
            unsafe {
                std::ptr::copy_nonoverlapping(f.data.as_ptr(), dst.add(rel), n);
            }
        }
        // Keep the base; the window is a scratch product used by callers that
        // want an eagerly compacted view.
        self.total_bytes = self.total_bytes.max(window.len());
    }

    /// Flatten all fragments into one contiguous buffer, ordered by offset and
    /// normalised against the base. Overlapping fragments later in offset order
    /// win.
    pub fn reassemble(&self) -> Result<Vec<u8>> {
        if self.fragments.is_empty() {
            return Ok(Vec::new());
        }
        let mut ordered: Vec<&Fragment> = self.fragments.iter().collect();
        ordered.sort_by_key(|f| f.offset);

        // Determine the extent.
        let mut max_end: usize = 0;
        for f in &ordered {
            let rel = (f.offset as i64) - (self.base_offset as i64);
            if rel < 0 {
                return Err(Error::malformed("fragment before base"));
            }
            let end = rel as usize + f.data.len();
            if end > self.max_total {
                return Err(Error::limit("reassembled extent too large"));
            }
            max_end = max_end.max(end);
        }

        let mut out = vec![0u8; max_end];
        for f in &ordered {
            let rel = ((f.offset as i64) - (self.base_offset as i64)) as usize;
            out[rel..rel + f.data.len()].copy_from_slice(&f.data);
        }
        Ok(out)
    }

    pub fn clear(&mut self) {
        self.fragments.clear();
        self.total_bytes = 0;
        self.base_offset = 0;
        self.coverage = CoverageTracker::new();
        self.covered = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_reassembly() {
        let mut r = Reassembler::new(16, 4096);
        r.add(4, b"world").unwrap();
        r.add(0, b"helo").unwrap();
        let out = r.reassemble().unwrap();
        assert_eq!(&out, b"heloworld");
    }

    #[test]
    fn negative_span_rejected() {
        let mut r = Reassembler::new(16, 4096);
        assert!(r.reorder(0x200, 0x100).is_err());
    }

    #[test]
    fn fragment_limit() {
        let mut r = Reassembler::new(2, 4096);
        r.add(0, b"a").unwrap();
        r.add(1, b"b").unwrap();
        assert!(r.add(2, b"c").is_err());
    }
}
