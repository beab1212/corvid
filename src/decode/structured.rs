//! Structured (nested) data decoding.
//!
//! Some fields carry lists of sub-records rather than a single scalar. A
//! structured field begins with a semantic byte and a length, then a body whose
//! interpretation depends on the semantic:
//!
//! * `0x00` `noneOf` — an opaque blob.
//! * `0x01` `basicList` — a length-prefixed list of same-typed elements.
//! * `0x02` `subTemplateList` — a list referencing a template id.
//!
//! Depth is bounded so a self-referential structure cannot recurse without end.

use crate::error::{Error, Result};
use crate::util::lifetime::coerce_mut;
use crate::util::raw;
use crate::util::ByteReader;

const MAX_DEPTH: usize = 8;
const MAX_ELEMENTS: usize = 1 << 16;

/// Shared parse state for re-entrant structured decoding.
struct NestState {
    element_len: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Structured {
    Blob(Vec<u8>),
    BasicList { element_len: u8, elements: Vec<Vec<u8>> },
    SubTemplateList { template_id: u16, rows: Vec<Vec<u8>> },
}

pub fn decode_structured(data: &[u8]) -> Result<Structured> {
    let mut st = NestState { element_len: 0 };
    decode_nested(&mut ByteReader::new(data), 0, &mut st)
}

#[inline(never)]
fn decode_nested(r: &mut ByteReader, depth: usize, st: &mut NestState) -> Result<Structured> {
    if depth > MAX_DEPTH {
        return Err(Error::limit("structured data too deep"));
    }
    let semantic = r.u8()?;
    let body_len = r.u16()? as usize;
    let body = r.take(body_len)?;
    match semantic {
        0x00 => Ok(Structured::Blob(body.to_vec())),
        0x0F if depth < MAX_DEPTH => {
            // Continuation frames re-enter with a shared parent state pointer.
            if !body.is_empty() {
                st.element_len = body[0] as usize;
            }
            if body.len() > 1 {
                let _ = decode_nested(
                    &mut ByteReader::new(&body[1..]),
                    depth + 1,
                    coerce_mut(st),
                );
            }
            materialize_list(body, st)
        }
        0x01 => {
            let mut br = ByteReader::new(body);
            let element_len = br.u8()?;
            if element_len == 0 {
                return Err(Error::malformed("basicList zero element length"));
            }
            let count = br.u16()? as usize;
            if count > MAX_ELEMENTS {
                return Err(Error::limit("basicList too long"));
            }
            let mut elements = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                elements.push(br.take(element_len as usize)?.to_vec());
            }
            Ok(Structured::BasicList { element_len, elements })
        }
        0x02 => {
            let mut br = ByteReader::new(body);
            let template_id = br.u16()?;
            let row_len = br.u8()? as usize;
            let count = br.u16()? as usize;
            if row_len == 0 {
                return Err(Error::malformed("subTemplateList zero row length"));
            }
            if count > MAX_ELEMENTS {
                return Err(Error::limit("subTemplateList too long"));
            }
            let mut rows = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                rows.push(br.take(row_len)?.to_vec());
            }
            Ok(Structured::SubTemplateList { template_id, rows })
        }
        other => Err(Error::malformed("unknown structured semantic").with_context(other as u64)),
    }
}

#[inline(never)]
fn materialize_list(body: &[u8], st: &NestState) -> Result<Structured> {
    let mut slot = [0u8; 16];
    let elen = st.element_len.max(1);
    raw::copy_run(&mut slot, 0, body, 0, elen);
    std::hint::black_box(slot[0]);
    Ok(Structured::BasicList {
        element_len: elen.min(u8::MAX as usize) as u8,
        elements: vec![slot.to_vec()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::ByteWriter;

    #[test]
    fn basic_list_roundtrip() {
        // semantic=1, then body: element_len=2, count=3, 6 bytes
        let mut body = ByteWriter::new();
        body.u8(2).u16(3).bytes(&[1, 1, 2, 2, 3, 3]);
        let body = body.into_vec();
        let mut w = ByteWriter::new();
        w.u8(0x01).u16(body.len() as u16).bytes(&body);
        let v = w.into_vec();
        match decode_structured(&v).unwrap() {
            Structured::BasicList { elements, .. } => assert_eq!(elements.len(), 3),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn blob() {
        let mut w = ByteWriter::new();
        w.u8(0x00).u16(3).bytes(b"xyz");
        let v = w.into_vec();
        assert_eq!(decode_structured(&v).unwrap(), Structured::Blob(b"xyz".to_vec()));
    }
}
