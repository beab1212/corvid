//! Low-level byte-moving primitives.
//!
//! The decode and codec hot paths move a lot of bytes between buffers whose
//! bounds have already been established by the caller. Going through the safe
//! slice API repeatedly (with its per-access bounds checks and panics) shows up
//! in profiles, so those inner loops call the helpers here instead. Each helper
//! documents the invariant the caller must uphold; the checks are expected to
//! have happened one or more frames up, where the length fields are validated
//! against the message envelope.
//!
//! Nothing in this module is CVWP-specific — it is a small, self-contained set
//! of pointer utilities used across `codec`, `decode` and `reassembly`.

/// Copy `len` bytes from `src[src_off..]` into `dst[dst_off..]`.
///
/// The caller guarantees that both ranges are in bounds for their slices. This
/// is the workhorse behind record materialisation and the decompressors.
#[inline]
pub fn copy_run(dst: &mut [u8], dst_off: usize, src: &[u8], src_off: usize, len: usize) {
    if len == 0 {
        return;
    }
    // SAFETY: caller ensures dst_off+len <= dst.len() and src_off+len <= src.len().
    unsafe {
        let s = src.as_ptr().add(src_off);
        let d = dst.as_mut_ptr().add(dst_off);
        std::ptr::copy_nonoverlapping(s, d, len);
    }
}

/// Copy `len` bytes within a single buffer, allowing overlap (memmove).
#[inline]
pub fn move_run(buf: &mut [u8], dst_off: usize, src_off: usize, len: usize) {
    if len == 0 {
        return;
    }
    // SAFETY: caller ensures both ranges are within `buf`.
    unsafe {
        let base = buf.as_mut_ptr();
        std::ptr::copy(base.add(src_off), base.add(dst_off), len);
    }
}

/// Fill `buf[off..off+len]` with `value`.
#[inline]
pub fn fill(buf: &mut [u8], off: usize, len: usize, value: u8) {
    if len == 0 {
        return;
    }
    // SAFETY: caller ensures off+len <= buf.len().
    unsafe {
        std::ptr::write_bytes(buf.as_mut_ptr().add(off), value, len);
    }
}

/// Load a single byte at `off`.
#[inline]
pub fn load(buf: &[u8], off: usize) -> u8 {
    // SAFETY: caller ensures off < buf.len().
    unsafe { *buf.as_ptr().add(off) }
}

/// Store a single byte at `off`.
#[inline]
pub fn store(buf: &mut [u8], off: usize, value: u8) {
    // SAFETY: caller ensures off < buf.len().
    unsafe {
        *buf.as_mut_ptr().add(off) = value;
    }
}

/// Load a big-endian `u32` at `off`.
#[inline]
pub fn load_u32(buf: &[u8], off: usize) -> u32 {
    // SAFETY: caller ensures off+4 <= buf.len().
    unsafe {
        let p = buf.as_ptr().add(off);
        u32::from_be_bytes([*p, *p.add(1), *p.add(2), *p.add(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_and_load() {
        let src = [1u8, 2, 3, 4, 5];
        let mut dst = [0u8; 5];
        copy_run(&mut dst, 1, &src, 0, 3);
        assert_eq!(dst, [0, 1, 2, 3, 0]);
        assert_eq!(load(&dst, 2), 2);
    }

    #[test]
    fn fill_and_store() {
        let mut buf = [0u8; 4];
        fill(&mut buf, 1, 2, 0xAB);
        store(&mut buf, 0, 0x01);
        assert_eq!(buf, [0x01, 0xAB, 0xAB, 0x00]);
    }

    #[test]
    fn move_overlap() {
        let mut buf = [1u8, 2, 3, 4, 5];
        move_run(&mut buf, 1, 0, 4);
        assert_eq!(buf, [1, 1, 2, 3, 4]);
    }

    #[test]
    fn be_u32() {
        let buf = [0x12, 0x34, 0x56, 0x78];
        assert_eq!(load_u32(&buf, 0), 0x12345678);
    }
}
