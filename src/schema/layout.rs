//! On-wire layout computation for a schema.
//!
//! While [`Schema`] caches the *row image* width, the decoder also wants to know
//! the on-wire byte offset of each fixed-width field so it can skip directly to
//! a column without decoding the ones before it. Variable-width fields break the
//! fixed layout, so the computation stops at the first variable field and marks
//! everything after it as "dynamic".

use crate::schema::field::FieldSpec;
use crate::schema::Schema;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Field starts at a known byte offset with a known width.
    Fixed { offset: usize, width: usize },
    /// Field's position depends on the length of preceding variable fields.
    Dynamic,
}

#[derive(Debug, Clone)]
pub struct Layout {
    placements: Vec<Placement>,
    fixed_prefix_bytes: usize,
    fully_fixed: bool,
}

impl Layout {
    pub fn compute(schema: &Schema) -> Layout {
        Layout::from_fields(&schema.fields)
    }

    pub fn from_fields(fields: &[FieldSpec]) -> Layout {
        let mut placements = Vec::with_capacity(fields.len());
        let mut offset = 0usize;
        let mut dynamic = false;
        for f in fields {
            if dynamic {
                placements.push(Placement::Dynamic);
                continue;
            }
            match f.wire_fixed_width() {
                Some(w) => {
                    placements.push(Placement::Fixed { offset, width: w });
                    offset += w;
                }
                None => {
                    // First variable field: it and everything after are dynamic.
                    placements.push(Placement::Dynamic);
                    dynamic = true;
                }
            }
        }
        Layout {
            placements,
            fixed_prefix_bytes: offset,
            fully_fixed: !dynamic,
        }
    }

    pub fn placement(&self, idx: usize) -> Option<Placement> {
        self.placements.get(idx).copied()
    }

    /// Byte offset of field `idx`, if it is in the fixed prefix.
    pub fn offset_of(&self, idx: usize) -> Option<usize> {
        match self.placements.get(idx) {
            Some(Placement::Fixed { offset, .. }) => Some(*offset),
            _ => None,
        }
    }

    pub fn fixed_prefix_bytes(&self) -> usize {
        self.fixed_prefix_bytes
    }

    pub fn is_fully_fixed(&self) -> bool {
        self.fully_fixed
    }

    /// Minimum on-wire record length (fixed prefix only; variable fields add a
    /// length prefix that is validated separately).
    pub fn min_record_len(&self) -> usize {
        self.fixed_prefix_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::field::FieldType;

    #[test]
    fn fixed_layout() {
        let s = Schema::new(
            1,
            1,
            vec![
                FieldSpec::new(1, FieldType::U32, 0),
                FieldSpec::new(2, FieldType::U16, 0),
                FieldSpec::new(3, FieldType::U8, 0),
            ],
        );
        let l = Layout::compute(&s);
        assert!(l.is_fully_fixed());
        assert_eq!(l.offset_of(1), Some(4));
        assert_eq!(l.offset_of(2), Some(6));
        assert_eq!(l.min_record_len(), 7);
    }

    #[test]
    fn variable_breaks_layout() {
        let s = Schema::new(
            1,
            1,
            vec![
                FieldSpec::new(1, FieldType::U32, 0),
                FieldSpec::new(2, FieldType::VarBytes, 64),
                FieldSpec::new(3, FieldType::U16, 0),
            ],
        );
        let l = Layout::compute(&s);
        assert!(!l.is_fully_fixed());
        assert_eq!(l.offset_of(0), Some(0));
        assert_eq!(l.placement(2), Some(Placement::Dynamic));
    }
}
