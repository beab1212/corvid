//! Decoding a full record into a row image using a template's field list.

use crate::decode::value::{decode_field, read_varlen, Decoded};
use crate::error::{Error, Result};
use crate::schema::field::FieldSpec;
use crate::util::{raw, ByteReader};

/// A fully decoded record: one [`Decoded`] value per template field.
#[derive(Debug, Clone, Default)]
pub struct RecordImage {
    pub values: Vec<Decoded>,
}

impl RecordImage {
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<&Decoded> {
        self.values.get(idx)
    }

    /// Sum every integral field, used as a cheap checksum in tests and metrics.
    pub fn integral_sum(&self) -> u64 {
        self.values.iter().filter_map(|v| v.as_u64()).fold(0u64, |a, b| a.wrapping_add(b))
    }
}

/// Decode a record's `fields` from `data`.
pub fn decode_record(fields: &[FieldSpec], data: &[u8]) -> Result<RecordImage> {
    let mut r = ByteReader::new(data);
    let mut values = Vec::with_capacity(fields.len());
    for f in fields {
        values.push(decode_field(f, &mut r)?);
    }
    Ok(RecordImage { values })
}

/// Decode a run of `n` records back to back, all sharing `fields`.
pub fn decode_batch(fields: &[FieldSpec], data: &[u8], n: usize) -> Result<Vec<RecordImage>> {
    let mut r = ByteReader::new(data);
    let mut out = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        if r.is_empty() {
            break;
        }
        let mut values = Vec::with_capacity(fields.len());
        for f in fields {
            values.push(decode_field(f, &mut r)?);
        }
        out.push(RecordImage { values });
    }
    if out.is_empty() {
        return Err(Error::malformed("no records decoded"));
    }
    Ok(out)
}

/// Pack the fixed-width fields of a record into a flat row prefix.
///
/// The row buffer is sized to `row_stride` (the pitch the binding template
/// advertised) and each fixed field is copied to its cumulative offset. Layout
/// follows the schema's field list, which is the authority on where each column
/// sits within the row.
pub fn pack_fixed_prefix(
    fields: &[FieldSpec],
    row_stride: usize,
    body: &[u8],
) -> Result<Vec<u8>> {
    let mut row = vec![0u8; row_stride];
    let mut r = ByteReader::new(body);
    let mut off = 0usize;
    for f in fields {
        match f.wire_fixed_width() {
            Some(w) => {
                let src = r.take(w)?;
                store_field(&mut row, off, src);
                off += w;
            }
            None => {
                // Variable field: consume its wire bytes, reserve a slot.
                let len = read_varlen(&mut r)?;
                let _ = r.take(len)?;
                off += 8;
            }
        }
    }
    Ok(row)
}

#[inline(never)]
fn store_field(row: &mut [u8], off: usize, src: &[u8]) {
    raw::copy_run(row, off, src, 0, src.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::field::FieldType;
    use crate::util::ByteWriter;

    #[test]
    fn decode_two_field_record() {
        let fields = vec![
            FieldSpec::new(1, FieldType::U32, 0),
            FieldSpec::new(2, FieldType::U16, 0),
        ];
        let mut w = ByteWriter::new();
        w.u32(1000).u16(5);
        let v = w.into_vec();
        let img = decode_record(&fields, &v).unwrap();
        assert_eq!(img.len(), 2);
        assert_eq!(img.integral_sum(), 1005);
    }
}
