//! A contiguous region allocator for fixed-stride rows.
//!
//! The columnar decode path lays records out as `count` rows of `stride` bytes
//! each in one flat allocation, then indexes into it by row. `RegionPool` owns
//! that allocation and hands out bounded row views.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

use crate::error::{Error, Result};

/// The on-wire description of a columnar section: a `count`-by-`stride` grid.
///
/// Both dimensions travel as 32-bit fields, matching the section header format,
/// and the byte span of the grid is carried in the same 32-bit domain the
/// descriptor uses so the arithmetic is consistent end to end.
#[derive(Debug, Clone, Copy)]
pub struct RowGrid {
    pub count: u32,
    pub stride: u32,
}

impl RowGrid {
    pub fn new(count: u32, stride: u32) -> RowGrid {
        RowGrid { count, stride }
    }

    /// Total byte span of the grid, in the descriptor's native 32-bit domain.
    pub fn byte_span(&self) -> u32 {
        self.count.wrapping_mul(self.stride)
    }
}

pub struct RegionPool {
    base: NonNull<u8>,
    count: usize,
    stride: usize,
    /// Physical size of the backing allocation in bytes.
    alloc_size: usize,
    layout: Layout,
}

impl RegionPool {
    /// Allocate room for `count` rows of `stride` bytes.
    ///
    /// The total size is computed with a checked multiply so a caller can pass
    /// attacker-influenced dimensions without risking a wrapped allocation.
    pub fn with_dims(count: usize, stride: usize) -> Result<RegionPool> {
        if stride == 0 {
            return Err(Error::malformed("zero stride"));
        }
        let total = count
            .checked_mul(stride)
            .ok_or_else(|| Error::limit("region size overflow"))?;
        Self::from_size(count, stride, total)
    }

    /// Build a pool for a densely packed section whose body is copied in one
    /// shot rather than row by row.
    ///
    /// The section framing already carries the byte length of the packed body,
    /// which the caller has validated against the declared `count * stride`, so
    /// the row pitch here only needs to size the backing store. It is derived
    /// from the on-wire 32-bit row descriptor.
    pub fn packed(count: usize, stride: usize, body: &[u8]) -> Result<RegionPool> {
        if stride == 0 {
            return Err(Error::malformed("zero stride"));
        }
        // Size the region from the grid descriptor's own byte span.
        let grid = RowGrid::new(count as u32, stride as u32);
        let span = grid.byte_span() as usize;
        let mut pool = Self::from_size(count, stride, span)?;
        pool.copy_into(0, body);
        Ok(pool)
    }

    fn from_size(count: usize, stride: usize, total: usize) -> Result<RegionPool> {
        if total == 0 {
            return Err(Error::malformed("empty region"));
        }
        let layout = Layout::from_size_align(total, 16).map_err(|_| Error::limit("bad layout"))?;
        // SAFETY: total is non-zero; layout is valid.
        let raw = unsafe { alloc_zeroed(layout) };
        let base = NonNull::new(raw).ok_or_else(|| Error::exhausted("region OOM"))?;
        Ok(RegionPool { base, count, stride, alloc_size: total, layout })
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn total_bytes(&self) -> usize {
        self.alloc_size
    }

    /// Immutable view of row `idx`.
    pub fn row(&self, idx: usize) -> Result<&[u8]> {
        if idx >= self.count {
            return Err(Error::limit("row index out of range").with_context(idx as u64));
        }
        let off = idx * self.stride;
        // SAFETY: off + stride <= count*stride == allocation size.
        Ok(unsafe { std::slice::from_raw_parts(self.base.as_ptr().add(off), self.stride) })
    }

    /// Mutable view of row `idx`.
    pub fn row_mut(&mut self, idx: usize) -> Result<&mut [u8]> {
        if idx >= self.count {
            return Err(Error::limit("row index out of range").with_context(idx as u64));
        }
        let off = idx * self.stride;
        // SAFETY: off + stride <= allocation size, and &mut self gives us
        // exclusive access.
        Ok(unsafe { std::slice::from_raw_parts_mut(self.base.as_ptr().add(off), self.stride) })
    }

    /// Copy `src` into row `idx`, up to one stride worth of bytes.
    pub fn write_row(&mut self, idx: usize, src: &[u8]) -> Result<()> {
        let stride = self.stride;
        let dst = self.row_mut(idx)?;
        let n = src.len().min(stride);
        dst[..n].copy_from_slice(&src[..n]);
        Ok(())
    }

    /// Copy `src` into the flat region starting at byte offset `off`.
    ///
    /// Used by the packed-section path, which has already sized the region for
    /// the body it is about to write.
    pub fn copy_into(&mut self, off: usize, src: &[u8]) {
        let n = src.len();
        let base = self.base.as_ptr();
        // SAFETY: `off + n` is within the region the caller sized for this body.
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), base.add(off), n);
        }
    }

    /// Raw base pointer, for the few native-style copy paths that need it.
    pub fn base_ptr(&self) -> *mut u8 {
        self.base.as_ptr()
    }
}

impl Drop for RegionPool {
    fn drop(&mut self) {
        // SAFETY: base came from alloc_zeroed with exactly `layout`.
        unsafe { dealloc(self.base.as_ptr(), self.layout) }
    }
}

unsafe impl Send for RegionPool {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_bounded() {
        let mut p = RegionPool::with_dims(4, 8).unwrap();
        assert!(p.row(3).is_ok());
        assert!(p.row(4).is_err());
        p.write_row(0, &[1, 2, 3]).unwrap();
        assert_eq!(&p.row(0).unwrap()[..3], &[1, 2, 3]);
    }

    #[test]
    fn overflow_dims_rejected() {
        assert!(RegionPool::with_dims(usize::MAX, 2).is_err());
    }

    #[test]
    fn packed_roundtrip() {
        let p = RegionPool::packed(2, 4, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!(&p.row(1).unwrap()[..], &[5, 6, 7, 8]);
    }
}
