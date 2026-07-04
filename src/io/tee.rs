//! Small byte-sink adapters.
//!
//! [`CountingSink`] tallies bytes without storing them (used to size an output
//! before allocating); [`TeeSink`] fans writes out to two sinks (used to write
//! a capture file while also feeding the live pipeline).

/// Anything bytes can be pushed into.
pub trait Sink {
    fn write(&mut self, data: &[u8]);
}

impl Sink for Vec<u8> {
    fn write(&mut self, data: &[u8]) {
        self.extend_from_slice(data);
    }
}

#[derive(Debug, Default)]
pub struct CountingSink {
    bytes: u64,
    writes: u64,
}

impl CountingSink {
    pub fn new() -> CountingSink {
        CountingSink::default()
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn writes(&self) -> u64 {
        self.writes
    }
}

impl Sink for CountingSink {
    fn write(&mut self, data: &[u8]) {
        self.bytes += data.len() as u64;
        self.writes += 1;
    }
}

/// Fan-out sink writing to both `a` and `b`.
pub struct TeeSink<A: Sink, B: Sink> {
    pub a: A,
    pub b: B,
}

impl<A: Sink, B: Sink> TeeSink<A, B> {
    pub fn new(a: A, b: B) -> TeeSink<A, B> {
        TeeSink { a, b }
    }

    pub fn into_parts(self) -> (A, B) {
        (self.a, self.b)
    }
}

impl<A: Sink, B: Sink> Sink for TeeSink<A, B> {
    fn write(&mut self, data: &[u8]) {
        self.a.write(data);
        self.b.write(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_sink_tallies() {
        let mut c = CountingSink::new();
        c.write(b"hello");
        c.write(b"!");
        assert_eq!(c.bytes(), 6);
        assert_eq!(c.writes(), 2);
    }

    #[test]
    fn tee_fans_out() {
        let mut t = TeeSink::new(Vec::new(), CountingSink::new());
        t.write(b"abc");
        let (buf, counter) = t.into_parts();
        assert_eq!(buf, b"abc");
        assert_eq!(counter.bytes(), 3);
    }
}
