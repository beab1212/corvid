//! A growable big-endian byte writer used by the encoder and the seed-gen
//! tooling. Nothing here is on the hot path, so it favours clarity over
//! cleverness.

#[derive(Debug, Default, Clone)]
pub struct ByteWriter {
    buf: Vec<u8>,
}

impl ByteWriter {
    pub fn new() -> Self {
        ByteWriter { buf: Vec::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        ByteWriter { buf: Vec::with_capacity(cap) }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(b);
        self
    }

    /// Reserve a 4-byte big-endian length slot and return its index so the
    /// caller can back-patch it once the body length is known.
    pub fn reserve_len32(&mut self) -> usize {
        let at = self.buf.len();
        self.buf.extend_from_slice(&[0, 0, 0, 0]);
        at
    }

    pub fn patch_len32(&mut self, at: usize, value: u32) {
        self.buf[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backpatch_length() {
        let mut w = ByteWriter::new();
        w.u8(0xAA);
        let slot = w.reserve_len32();
        w.bytes(&[1, 2, 3]);
        w.patch_len32(slot, 3);
        let v = w.into_vec();
        assert_eq!(v[0], 0xAA);
        assert_eq!(&v[1..5], &[0, 0, 0, 3]);
        assert_eq!(&v[5..], &[1, 2, 3]);
    }
}
