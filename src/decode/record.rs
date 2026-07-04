//! Decoding a full record into a row image using a template's field list.

use crate::decode::value::{decode_field, Decoded};
use crate::error::{Error, Result};
use crate::schema::field::FieldSpec;
use crate::util::ByteReader;

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
