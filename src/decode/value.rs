//! Decoding individual field values from record bytes.

use crate::error::{Error, Result};
use crate::schema::field::{FieldSpec, FieldType};
use crate::util::ByteReader;

/// A decoded scalar or byte-string field value.
#[derive(Debug, Clone, PartialEq)]
pub enum Decoded {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I32(i32),
    I64(i64),
    Bytes(Vec<u8>),
    Utf8(String),
    Timestamp(u64),
}

impl Decoded {
    /// Best-effort conversion to an unsigned integer for aggregation.
    pub fn as_u64(&self) -> Option<u64> {
        Some(match self {
            Decoded::U8(v) => *v as u64,
            Decoded::U16(v) => *v as u64,
            Decoded::U32(v) => *v as u64,
            Decoded::U64(v) => *v,
            Decoded::I32(v) => *v as u64,
            Decoded::I64(v) => *v as u64,
            Decoded::Timestamp(v) => *v,
            _ => return None,
        })
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Decoded::U8(_) => "u8",
            Decoded::U16(_) => "u16",
            Decoded::U32(_) => "u32",
            Decoded::U64(_) => "u64",
            Decoded::I32(_) => "i32",
            Decoded::I64(_) => "i64",
            Decoded::Bytes(_) => "bytes",
            Decoded::Utf8(_) => "utf8",
            Decoded::Timestamp(_) => "timestamp",
        }
    }
}

/// Decode one field of type `spec.ty` from the reader.
pub fn decode_field(spec: &FieldSpec, r: &mut ByteReader) -> Result<Decoded> {
    Ok(match spec.ty {
        FieldType::U8 => Decoded::U8(r.u8()?),
        FieldType::U16 => Decoded::U16(r.u16()?),
        FieldType::U32 => Decoded::U32(r.u32()?),
        FieldType::U64 => Decoded::U64(r.u64()?),
        FieldType::I32 => Decoded::I32(r.i32()?),
        FieldType::I64 => Decoded::I64(r.u64()? as i64),
        FieldType::Timestamp => Decoded::Timestamp(r.u64()?),
        FieldType::Fixed => {
            let n = spec.width as usize;
            Decoded::Bytes(r.take(n)?.to_vec())
        }
        FieldType::VarBytes => {
            let len = read_varlen(r)?;
            Decoded::Bytes(r.take(len)?.to_vec())
        }
        FieldType::Utf8 => {
            let len = read_varlen(r)?;
            let bytes = r.take(len)?;
            let s = std::str::from_utf8(bytes)
                .map_err(|_| Error::malformed("field: invalid utf8"))?
                .to_string();
            Decoded::Utf8(s)
        }
    })
}

/// Read a CVWP variable-length prefix: a single length byte, or `0xFF` followed
/// by a 2-byte extended length.
pub fn read_varlen(r: &mut ByteReader) -> Result<usize> {
    let first = r.u8()?;
    if first < 0xFF {
        Ok(first as usize)
    } else {
        Ok(r.u16()? as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::ByteWriter;

    #[test]
    fn decode_scalars() {
        let mut w = ByteWriter::new();
        w.u32(0xdeadbeef);
        let v = w.into_vec();
        let mut r = ByteReader::new(&v);
        let d = decode_field(&FieldSpec::new(1, FieldType::U32, 0), &mut r).unwrap();
        assert_eq!(d, Decoded::U32(0xdeadbeef));
        assert_eq!(d.as_u64(), Some(0xdeadbeef));
    }

    #[test]
    fn decode_varbytes() {
        let mut w = ByteWriter::new();
        w.u8(3).bytes(b"abc");
        let v = w.into_vec();
        let mut r = ByteReader::new(&v);
        let d = decode_field(&FieldSpec::new(2, FieldType::VarBytes, 0), &mut r).unwrap();
        assert_eq!(d, Decoded::Bytes(b"abc".to_vec()));
    }

    #[test]
    fn extended_varlen() {
        let mut w = ByteWriter::new();
        w.u8(0xFF).u16(300);
        let v = w.into_vec();
        let mut r = ByteReader::new(&v);
        assert_eq!(read_varlen(&mut r).unwrap(), 300);
    }
}
