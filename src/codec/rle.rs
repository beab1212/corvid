//! Byte-oriented run-length coding.
//!
//! Wire form is a sequence of `(control, byte...)` groups:
//! * control `0x00..=0x7f` → a literal run of `control + 1` raw bytes follows.
//! * control `0x80..=0xff` → a repeat of the single following byte,
//!   `control - 0x80 + 2` times (so a repeat always denotes at least 2).
//!
//! Decoding is bounded by an explicit output limit supplied by the caller so a
//! hostile stream cannot make us allocate without limit.

use crate::error::{Error, Result};

const MAX_LITERAL: usize = 0x80;
const MIN_REPEAT: usize = 2;

pub fn encode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 2 + 1);
    let mut i = 0;
    while i < input.len() {
        // Count a run of the same byte.
        let b = input[i];
        let mut run = 1;
        while i + run < input.len() && input[i + run] == b && run < 0x81 {
            run += 1;
        }
        if run >= MIN_REPEAT {
            out.push(0x80 + (run - MIN_REPEAT) as u8);
            out.push(b);
            i += run;
        } else {
            // Emit a literal run up to the next repeat or the cap.
            let start = i;
            let mut lit = 0;
            while i < input.len() && lit < MAX_LITERAL {
                let c = input[i];
                let mut ahead = 1;
                while i + ahead < input.len() && input[i + ahead] == c && ahead < MIN_REPEAT {
                    ahead += 1;
                }
                if ahead >= MIN_REPEAT {
                    break;
                }
                i += 1;
                lit += 1;
            }
            out.push((lit - 1) as u8);
            out.extend_from_slice(&input[start..start + lit]);
        }
    }
    out
}

/// Decode into a freshly allocated vector, refusing to exceed `output_limit`.
pub fn decode(input: &[u8], output_limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let control = input[i];
        i += 1;
        if control < 0x80 {
            let n = control as usize + 1;
            if i + n > input.len() {
                return Err(Error::codec("rle literal past end"));
            }
            if out.len() + n > output_limit {
                return Err(Error::limit("rle output over limit"));
            }
            out.extend_from_slice(&input[i..i + n]);
            i += n;
        } else {
            let n = (control - 0x80) as usize + MIN_REPEAT;
            if i >= input.len() {
                return Err(Error::codec("rle repeat missing byte"));
            }
            let b = input[i];
            i += 1;
            if out.len() + n > output_limit {
                return Err(Error::limit("rle output over limit"));
            }
            out.resize(out.len() + n, b);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_mixed() {
        let data = b"aaaaabcdefggggggggh".to_vec();
        let enc = encode(&data);
        let dec = decode(&enc, 4096).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn roundtrip_random_ish() {
        let mut data = Vec::new();
        for i in 0..500u32 {
            data.push((i.wrapping_mul(31) >> 3) as u8);
        }
        let enc = encode(&data);
        assert_eq!(decode(&enc, 4096).unwrap(), data);
    }

    #[test]
    fn output_limit_enforced() {
        let enc = encode(&vec![7u8; 1000]);
        assert!(decode(&enc, 100).is_err());
    }
}
