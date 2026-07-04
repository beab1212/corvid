#![no_main]
//! Exercises the stream framer end to end. The framer is the first thing every
//! byte on the wire hits, so it needs to survive arbitrary input without ever
//! reading out of bounds.

use libfuzzer_sys::fuzz_target;

use corvid::parser::FrameParser;

fuzz_target!(|data: &[u8]| {
    let mut parser = FrameParser::new();
    if let Ok(messages) = parser.parse_all(data) {
        // Touch each message so the optimiser cannot elide the parse.
        let mut acc = 0usize;
        for m in &messages {
            acc = acc.wrapping_add(m.payload.len()).wrapping_add(m.ty as usize);
        }
        std::hint::black_box(acc);
    }
});
