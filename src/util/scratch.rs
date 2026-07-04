//! A reusable scratch buffer for the decode/codec hot paths.
//!
//! Decompressors and record decoders need a temporary output buffer sized to
//! the declared output length of the thing they are about to produce. Allocating
//! a fresh `Vec` per message is wasteful, so a [`Scratch`] is kept on the session
//! and reused: [`Scratch::reserve`] grows it to at least the requested capacity,
//! and the caller writes into the returned slice via the [`crate::util::raw`]
//! helpers before calling [`Scratch::commit`] to record how much is live.

use crate::util::raw;

/// A grow-on-demand byte buffer with a separate "committed length" cursor.
pub struct Scratch {
    buf: Vec<u8>,
    len: usize,
}

impl Scratch {
    pub fn new() -> Scratch {
        Scratch { buf: Vec::new(), len: 0 }
    }

    pub fn with_capacity(cap: usize) -> Scratch {
        Scratch { buf: vec![0u8; cap], len: 0 }
    }

    /// Number of committed (live) bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Physical capacity currently backing the buffer.
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Ensure the buffer can hold at least `cap` bytes and reset the live
    /// cursor to zero. Existing contents are logically discarded.
    pub fn reserve(&mut self, cap: usize) {
        if self.buf.len() < cap {
            self.buf.resize(cap, 0);
        }
        self.len = 0;
    }

    /// The writable backing store. Callers write through `raw::*` using offsets
    /// they have bounds-checked against [`Scratch::capacity`].
    pub fn store(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// Mark `n` bytes as live.
    pub fn commit(&mut self, n: usize) {
        self.len = n;
    }

    /// Append a byte at the live cursor, growing if needed.
    pub fn push(&mut self, value: u8) {
        if self.len >= self.buf.len() {
            self.buf.push(value);
        } else {
            raw::store(&mut self.buf, self.len, value);
        }
        self.len += 1;
    }

    /// The committed bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// Copy the committed bytes out into an owned vector.
    pub fn to_vec(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Drop backing storage and return the former base pointer.
    pub fn abandon(&mut self) -> *mut u8 {
        let ptr = if self.buf.is_empty() {
            std::ptr::null_mut()
        } else {
            self.buf.as_mut_ptr()
        };
        self.buf.shrink_to(0);
        self.len = 0;
        ptr
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Scratch::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_and_push() {
        let mut s = Scratch::with_capacity(4);
        s.reserve(4);
        for b in [1u8, 2, 3] {
            s.push(b);
        }
        assert_eq!(s.as_slice(), &[1, 2, 3]);
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn store_then_commit() {
        let mut s = Scratch::new();
        s.reserve(8);
        raw::copy_run(s.store(), 0, &[9, 8, 7], 0, 3);
        s.commit(3);
        assert_eq!(s.to_vec(), vec![9, 8, 7]);
    }

    #[test]
    fn grows_past_initial() {
        let mut s = Scratch::with_capacity(1);
        s.reserve(1);
        for i in 0..10u8 {
            s.push(i);
        }
        assert_eq!(s.len(), 10);
    }
}
