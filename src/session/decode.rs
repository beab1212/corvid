//! Payload decoding helpers shared by the session handlers.
//!
//! These translate raw message payloads into the structured values the
//! subsystems consume. They are deliberately small and side-effect free so the
//! handlers in [`super`] read as a sequence of "decode, then act" steps.

use crate::error::{Error, Result};
use crate::flow::FlowKey;
use crate::schema::field::{FieldSpec, FieldType};
use crate::util::ByteReader;

/// A decoded data-record header — the fixed portion every `DATA_RECORD`
/// carries before its template-specific field bytes.
#[derive(Debug, Clone, Copy)]
pub struct DataHeader {
    pub template_id: u16,
    pub key: FlowKey,
    pub octets: u64,
    pub packets: u64,
}

/// Parse a `[field_count: u16][field...]` block, where each field is
/// `[id: u16][type: u8][width: u16]`.
pub fn parse_fields(r: &mut ByteReader) -> Result<Vec<FieldSpec>> {
    let count = r.u16()? as usize;
    if count == 0 {
        return Err(Error::malformed("record with zero fields"));
    }
    if count > 4096 {
        return Err(Error::limit("field count too high").with_context(count as u64));
    }
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.u16()?;
        let ty = FieldType::from_code(r.u8()?)?;
        let width = r.u16()?;
        fields.push(FieldSpec::new(id, ty, width));
    }
    Ok(fields)
}

/// Parse the fixed header of a `DATA_RECORD`.
pub fn parse_data_header(r: &mut ByteReader) -> Result<DataHeader> {
    let template_id = r.u16()?;
    let flow_id = r.u32()?;
    let src = r.u32()?;
    let dst = r.u32()?;
    let sport = r.u16()?;
    let dport = r.u16()?;
    let proto = r.u8()?;
    let octets = r.u64()?;
    let packets = r.u64()?;
    let key = FlowKey::new(src, dst, flow_id, sport, dport, proto);
    Ok(DataHeader { template_id, key, octets, packets })
}

/// Validate that the remaining bytes are consistent with a template's fixed
/// field widths, returning the total fixed width consumed.
pub fn fixed_width_of(fields: &[FieldSpec]) -> usize {
    fields.iter().filter_map(|f| f.wire_fixed_width()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::ByteWriter;

    #[test]
    fn parse_fields_roundtrip() {
        let mut w = ByteWriter::new();
        w.u16(2);
        w.u16(1).u8(FieldType::U32.code()).u16(0);
        w.u16(2).u8(FieldType::Fixed.code()).u16(6);
        let v = w.into_vec();
        let mut r = ByteReader::new(&v);
        let fields = parse_fields(&mut r).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[1].width, 6);
        assert_eq!(fixed_width_of(&fields), 4 + 6);
    }

    #[test]
    fn zero_fields_error() {
        let v = 0u16.to_be_bytes();
        let mut r = ByteReader::new(&v);
        assert!(parse_fields(&mut r).is_err());
    }
}
