//! A small static-dictionary substitution coder.
//!
//! Repeated tokens (column names, well-known tag values) are replaced by short
//! dictionary references. Unlike the LZ coder this uses a *shared* dictionary
//! negotiated at session open, so it compresses even the first occurrence of a
//! token. The wire form is a stream of tagged runs:
//!
//! ```text
//! 0b0xxxxxxx            : literal run of (x+1) bytes, followed by the bytes
//! 0b1xxxxxxx yyyyyyyy   : dictionary reference, entry ((x<<8)|y)
//! ```

use crate::error::{Error, Result};

pub struct Dictionary {
    entries: Vec<Vec<u8>>,
}

impl Dictionary {
    pub fn new() -> Dictionary {
        Dictionary { entries: Vec::new() }
    }

    pub fn from_entries(entries: Vec<Vec<u8>>) -> Dictionary {
        Dictionary { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add(&mut self, token: &[u8]) -> u16 {
        let id = self.entries.len() as u16;
        self.entries.push(token.to_vec());
        id
    }

    fn find(&self, hay: &[u8]) -> Option<(u16, usize)> {
        // Greedy longest match at the head of `hay`.
        let mut best: Option<(u16, usize)> = None;
        for (i, e) in self.entries.iter().enumerate() {
            if !e.is_empty() && hay.starts_with(e) {
                match best {
                    Some((_, blen)) if e.len() <= blen => {}
                    _ => best = Some((i as u16, e.len())),
                }
            }
        }
        best
    }

    pub fn encode(&self, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut literal: Vec<u8> = Vec::new();
        let mut pos = 0;
        while pos < input.len() {
            if let Some((id, len)) = self.find(&input[pos..]) {
                flush_literal(&mut out, &mut literal);
                out.push(0x80 | ((id >> 8) as u8 & 0x7F));
                out.push((id & 0xFF) as u8);
                pos += len;
            } else {
                literal.push(input[pos]);
                pos += 1;
                if literal.len() == 128 {
                    flush_literal(&mut out, &mut literal);
                }
            }
        }
        flush_literal(&mut out, &mut literal);
        out
    }

    pub fn decode(&self, input: &[u8], output_limit: usize) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < input.len() {
            let tag = input[i];
            i += 1;
            if tag & 0x80 == 0 {
                let run = (tag as usize) + 1;
                if i + run > input.len() {
                    return Err(Error::codec("dict: truncated literal run"));
                }
                if out.len() + run > output_limit {
                    return Err(Error::limit("dict: output over limit"));
                }
                out.extend_from_slice(&input[i..i + run]);
                i += run;
            } else {
                if i >= input.len() {
                    return Err(Error::codec("dict: truncated reference"));
                }
                let id = (((tag & 0x7F) as usize) << 8) | input[i] as usize;
                i += 1;
                let entry = self
                    .entries
                    .get(id)
                    .ok_or_else(|| Error::codec("dict: unknown entry").with_context(id as u64))?;
                if out.len() + entry.len() > output_limit {
                    return Err(Error::limit("dict: output over limit"));
                }
                out.extend_from_slice(entry);
            }
        }
        Ok(out)
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Dictionary::new()
    }
}

fn flush_literal(out: &mut Vec<u8>, literal: &mut Vec<u8>) {
    if literal.is_empty() {
        return;
    }
    out.push((literal.len() - 1) as u8 & 0x7F);
    out.extend_from_slice(literal);
    literal.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_dict() {
        let mut d = Dictionary::new();
        d.add(b"protocol=");
        d.add(b"tcp");
        let input = b"protocol=tcp payload".to_vec();
        let enc = d.encode(&input);
        assert!(enc.len() <= input.len());
        let dec = d.decode(&enc, 4096).unwrap();
        assert_eq!(dec, input);
    }

    #[test]
    fn unknown_entry_errors() {
        let d = Dictionary::new();
        // 0x80,0x00 references entry 0 which does not exist.
        assert!(d.decode(&[0x80, 0x00], 64).is_err());
    }

    #[test]
    fn pure_literal_roundtrip() {
        let d = Dictionary::new();
        let input: Vec<u8> = (0..200u16).map(|x| x as u8).collect();
        let enc = d.encode(&input);
        assert_eq!(d.decode(&enc, 4096).unwrap(), input);
    }
}
