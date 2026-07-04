#![no_main]
//! Drives a full session from arbitrary bytes.
//!
//! This is the deepest harness: a single input can define schemas and
//! templates, open flows, push data records and fragments, load VM modules and
//! call symbols, all against one accumulating [`Session`]. State therefore
//! carries across messages within an input, which is where the interesting
//! multi-message interactions live.

use libfuzzer_sys::fuzz_target;

use corvid::session::Session;
use corvid::Config;

fuzz_target!(|data: &[u8]| {
    // A compact config keeps per-iteration allocation small so the fuzzer runs
    // hot; behaviour is identical to the default config, just smaller ceilings.
    let mut session = Session::with_config(Config::compact());
    let _ = session.process_stream(data);
    std::hint::black_box(session.flow_count());
});
