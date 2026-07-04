#![no_main]
//! Fuzzes the decompression codecs directly.
//!
//! Decoders are the classic place for out-of-bounds writes: a back-reference or
//! run length taken from attacker-controlled bytes drives how much is copied and
//! from where. The first input byte selects a codec so a single corpus exercises
//! all of them; the rest is the compressed payload.

use libfuzzer_sys::fuzz_target;

use corvid::codec::{compress::inflate_block, Codec, dictionary::Dictionary, lz, rle};

const OUTPUT_LIMIT: usize = 1 << 20;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let (selector, payload) = (data[0], &data[1..]);
    match selector % 5 {
        0 => {
            let _ = rle::decode(payload, OUTPUT_LIMIT);
        }
        1 => {
            let _ = lz::decode(payload, OUTPUT_LIMIT);
        }
        2 => {
            if let Ok(c) = Codec::from_code(selector >> 3) {
                let _ = inflate_block(c, payload, OUTPUT_LIMIT);
            }
        }
        3 => {
            // Build a dictionary from a slice of the payload, then decode the rest.
            let mut dict = Dictionary::new();
            if !payload.is_empty() {
                let split = payload.len() / 2;
                dict.add(&payload[..split]);
                let _ = dict.decode(&payload[split..], OUTPUT_LIMIT);
            }
        }
        _ => {
            // Round-trip: encode then decode should never crash.
            let enc = rle::encode(payload);
            let _ = rle::decode(&enc, OUTPUT_LIMIT);
        }
    }
});
