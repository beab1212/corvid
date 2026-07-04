//! Field type system for CVWP records.
//!
//! A schema is an ordered list of [`FieldSpec`]s. Fixed-width fields contribute
//! a known number of bytes to a record; variable-width fields carry a
//! length-prefix on the wire and contribute a pointer-sized slot to the decoded
//! row image.

use crate::error::{Error, Result};

/// Logical type of a single field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    U8,
    U16,
    U32,
    U64,
    I32,
    I64,
    /// Fixed-length opaque bytes; the length is carried in the [`FieldSpec`].
    Fixed,
    /// Length-prefixed opaque bytes.
    VarBytes,
    /// UTF-8 string, length-prefixed.
    Utf8,
    /// A timestamp in milliseconds since the epoch (stored as u64).
    Timestamp,
}

impl FieldType {
    pub fn from_code(code: u8) -> Result<FieldType> {
        Ok(match code {
            0x01 => FieldType::U8,
            0x02 => FieldType::U16,
            0x03 => FieldType::U32,
            0x04 => FieldType::U64,
            0x05 => FieldType::I32,
            0x06 => FieldType::I64,
            0x07 => FieldType::Fixed,
            0x08 => FieldType::VarBytes,
            0x09 => FieldType::Utf8,
            0x0A => FieldType::Timestamp,
            other => {
                return Err(Error::malformed("unknown field type").with_context(other as u64))
            }
        })
    }

    pub fn code(self) -> u8 {
        match self {
            FieldType::U8 => 0x01,
            FieldType::U16 => 0x02,
            FieldType::U32 => 0x03,
            FieldType::U64 => 0x04,
            FieldType::I32 => 0x05,
            FieldType::I64 => 0x06,
            FieldType::Fixed => 0x07,
            FieldType::VarBytes => 0x08,
            FieldType::Utf8 => 0x09,
            FieldType::Timestamp => 0x0A,
        }
    }

    /// Whether this type is variable-width on the wire.
    pub fn is_variable(self) -> bool {
        matches!(self, FieldType::VarBytes | FieldType::Utf8)
    }

    /// The natural fixed width in bytes, if any. `Fixed` returns `None` because
    /// its width lives in the [`FieldSpec`].
    pub fn intrinsic_width(self) -> Option<usize> {
        Some(match self {
            FieldType::U8 => 1,
            FieldType::U16 => 2,
            FieldType::U32 | FieldType::I32 => 4,
            FieldType::U64 | FieldType::I64 | FieldType::Timestamp => 8,
            FieldType::Fixed | FieldType::VarBytes | FieldType::Utf8 => return None,
        })
    }
}

/// One field in a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    pub id: u16,
    pub ty: FieldType,
    /// On-wire width for `Fixed`; for variable fields this is the maximum
    /// length we will materialise; for intrinsically-sized fields it is
    /// ignored on decode but retained for round-tripping.
    pub width: u16,
    /// Column name, interned by the schema registry. Empty if anonymous.
    pub name: String,
}

impl FieldSpec {
    pub fn new(id: u16, ty: FieldType, width: u16) -> FieldSpec {
        FieldSpec { id, ty, width, name: String::new() }
    }

    pub fn named(id: u16, ty: FieldType, width: u16, name: impl Into<String>) -> FieldSpec {
        FieldSpec { id, ty, width, name: name.into() }
    }

    /// The number of bytes this field contributes to a decoded row image. For
    /// variable fields this is a 8-byte slot holding an offset+length pair.
    pub fn row_slot_width(&self) -> usize {
        match self.ty.intrinsic_width() {
            Some(w) => w,
            None => {
                if self.ty.is_variable() {
                    8
                } else {
                    self.width as usize
                }
            }
        }
    }

    /// On-wire fixed contribution, or `None` for variable fields.
    pub fn wire_fixed_width(&self) -> Option<usize> {
        match self.ty {
            FieldType::Fixed => Some(self.width as usize),
            t if t.is_variable() => None,
            t => t.intrinsic_width(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_roundtrip() {
        for code in 1u8..=0x0A {
            let t = FieldType::from_code(code).unwrap();
            assert_eq!(t.code(), code);
        }
        assert!(FieldType::from_code(0xFF).is_err());
    }

    #[test]
    fn slot_widths() {
        assert_eq!(FieldSpec::new(1, FieldType::U32, 0).row_slot_width(), 4);
        assert_eq!(FieldSpec::new(2, FieldType::Fixed, 12).row_slot_width(), 12);
        assert_eq!(FieldSpec::new(3, FieldType::VarBytes, 99).row_slot_width(), 8);
    }
}
