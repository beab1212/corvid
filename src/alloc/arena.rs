//! A chunked bump allocator.
//!
//! The decode path allocates a great many short-lived records with the same
//! lifetime: everything derived from one inbound batch is thrown away together.
//! A bump arena is a natural fit — allocation is a pointer increment and
//! reclamation is resetting a chunk's watermark.
//!
//! Memory is handed out as raw byte ranges; higher layers (`flow`, `schema`)
//! carve typed values out of those ranges. The arena grows by allocating fresh
//! chunks and never moves an existing chunk, so a pointer returned by
//! [`Arena::alloc`] stays valid until the owning chunk is reset or the arena is
//! dropped.

use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;

const DEFAULT_CHUNK: usize = 64 * 1024;

struct Chunk {
    base: NonNull<u8>,
    cap: usize,
    used: usize,
    layout: Layout,
}

impl Chunk {
    fn new(cap: usize) -> Chunk {
        let layout = Layout::from_size_align(cap, 16).expect("arena chunk layout");
        // SAFETY: cap is non-zero and the layout is valid.
        let raw = unsafe { alloc(layout) };
        let base = NonNull::new(raw).expect("arena chunk OOM");
        Chunk { base, cap, used: 0, layout }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.cap - self.used
    }

    /// Attempt to carve `size` bytes aligned to `align` out of this chunk.
    fn try_alloc(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        let aligned = (self.used + (align - 1)) & !(align - 1);
        if aligned + size > self.cap {
            return None;
        }
        // SAFETY: aligned + size <= cap, so the offset is in bounds.
        let ptr = unsafe { self.base.as_ptr().add(aligned) };
        self.used = aligned + size;
        Some(NonNull::new(ptr).unwrap())
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        // SAFETY: base came from `alloc` with exactly `layout`.
        unsafe { dealloc(self.base.as_ptr(), self.layout) }
    }
}

/// A bump allocator made of one or more fixed-size chunks.
pub struct Arena {
    chunks: Vec<Chunk>,
    chunk_size: usize,
    /// Running total of live bytes across all chunks, for metrics.
    high_water: usize,
}

impl Arena {
    pub fn new() -> Arena {
        Arena::with_chunk_size(DEFAULT_CHUNK)
    }

    pub fn with_chunk_size(chunk_size: usize) -> Arena {
        let chunk_size = chunk_size.max(4096);
        Arena { chunks: vec![Chunk::new(chunk_size)], chunk_size, high_water: 0 }
    }

    /// Number of chunks currently backing the arena.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn high_water(&self) -> usize {
        self.high_water
    }

    /// Allocate `size` bytes with `align`, returning a pointer valid until the
    /// containing chunk is reset. Returns the chunk index alongside the pointer
    /// so callers that track chunk lifetime can associate the two.
    pub fn alloc(&mut self, size: usize, align: usize) -> (NonNull<u8>, usize) {
        debug_assert!(align.is_power_of_two());
        // Try the most-recently-added chunk first (typical bump behaviour).
        let last = self.chunks.len() - 1;
        if let Some(p) = self.chunks[last].try_alloc(size, align) {
            self.high_water += size;
            return (p, last);
        }
        // Need a new chunk large enough for this request.
        let cap = self.chunk_size.max(size + align);
        self.chunks.push(Chunk::new(cap));
        let idx = self.chunks.len() - 1;
        let p = self.chunks[idx].try_alloc(size, align).expect("fresh chunk too small");
        self.high_water += size;
        (p, idx)
    }

    /// Copy `bytes` into the arena and return a pointer to the copy.
    pub fn alloc_slice(&mut self, bytes: &[u8]) -> (NonNull<u8>, usize) {
        let (p, idx) = self.alloc(bytes.len().max(1), 1);
        // SAFETY: we just reserved `bytes.len()` writable bytes at `p`.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.as_ptr(), bytes.len()) }
        (p, idx)
    }

    /// Reset a single chunk's watermark, logically freeing everything in it.
    /// The backing memory is retained for reuse. Any pointer previously handed
    /// out of this chunk is invalid after this call.
    pub fn reset_chunk(&mut self, idx: usize) {
        if let Some(c) = self.chunks.get_mut(idx) {
            self.high_water = self.high_water.saturating_sub(c.used);
            c.used = 0;
        }
    }

    /// Reset every chunk. Keeps the first chunk, drops the rest to release
    /// memory back to the system between large batches.
    pub fn reset_all(&mut self) {
        self.chunks.truncate(1);
        self.chunks[0].used = 0;
        self.high_water = 0;
    }

    /// Bytes still available in the newest chunk without growing.
    pub fn tail_remaining(&self) -> usize {
        self.chunks.last().map(|c| c.remaining()).unwrap_or(0)
    }
}

impl Default for Arena {
    fn default() -> Self {
        Arena::new()
    }
}

// The arena owns its chunks exclusively; sending it between threads is fine as
// long as callers uphold the usual &/&mut discipline.
unsafe impl Send for Arena {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_is_aligned() {
        let mut a = Arena::with_chunk_size(4096);
        let (p, _) = a.alloc(8, 8);
        assert_eq!(p.as_ptr() as usize % 8, 0);
    }

    #[test]
    fn grows_into_new_chunk() {
        let mut a = Arena::with_chunk_size(4096);
        let before = a.chunk_count();
        let _ = a.alloc(4096, 1);
        let _ = a.alloc(4096, 1);
        assert!(a.chunk_count() > before);
    }

    #[test]
    fn slice_roundtrip() {
        let mut a = Arena::new();
        let data = [1u8, 2, 3, 4, 5];
        let (p, _) = a.alloc_slice(&data);
        let view = unsafe { std::slice::from_raw_parts(p.as_ptr(), data.len()) };
        assert_eq!(view, &data);
    }

    #[test]
    fn reset_reclaims_space() {
        let mut a = Arena::with_chunk_size(4096);
        let _ = a.alloc(2048, 1);
        a.reset_chunk(0);
        assert_eq!(a.high_water(), 0);
    }
}
