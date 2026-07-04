//! A contiguous region allocator for fixed-stride rows.
//!
//! The columnar decode path lays records out as `count` rows of `stride` bytes
//! each in one flat allocation, then indexes into it by row. `RegionPool` owns
//! that allocation and hands out bounded row views.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

use crate::error::{Error, Result};

pub struct RegionPool {
    base: NonNull<u8>,
    count: usize,
    stride: usize,
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
        if total == 0 {
            return Err(Error::malformed("empty region"));
        }
        let layout = Layout::from_size_align(total, 16).map_err(|_| Error::limit("bad layout"))?;
        // SAFETY: total is non-zero; layout is valid.
        let raw = unsafe { alloc_zeroed(layout) };
        let base = NonNull::new(raw).ok_or_else(|| Error::exhausted("region OOM"))?;
        Ok(RegionPool { base, count, stride, layout })
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn total_bytes(&self) -> usize {
        self.count * self.stride
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
}
