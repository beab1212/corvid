//! Structural diffing of two schema versions.
//!
//! When a `SCHEMA_UPDATE` arrives, comparing the old and new field lists tells
//! the decoder whether the change is backward-compatible (fields only appended)
//! or breaking (a field's type or width changed), which drives cache
//! invalidation decisions upstream.

use crate::schema::field::FieldSpec;
use crate::schema::Schema;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldChange {
    Added(u16),
    Removed(u16),
    Retyped { id: u16 },
    Rewidth { id: u16, from: u16, to: u16 },
}

#[derive(Debug, Default)]
pub struct SchemaDiff {
    pub changes: Vec<FieldChange>,
}

impl SchemaDiff {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// A diff is backward-compatible if it only adds trailing fields.
    pub fn is_backward_compatible(&self) -> bool {
        self.changes.iter().all(|c| matches!(c, FieldChange::Added(_)))
    }

    pub fn breaking_changes(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| !matches!(c, FieldChange::Added(_)))
            .count()
    }
}

fn find<'a>(fields: &'a [FieldSpec], id: u16) -> Option<&'a FieldSpec> {
    fields.iter().find(|f| f.id == id)
}

/// Compute the diff turning `old` into `new`.
pub fn diff(old: &Schema, new: &Schema) -> SchemaDiff {
    let mut changes = Vec::new();
    for nf in &new.fields {
        match find(&old.fields, nf.id) {
            None => changes.push(FieldChange::Added(nf.id)),
            Some(of) => {
                if of.ty != nf.ty {
                    changes.push(FieldChange::Retyped { id: nf.id });
                } else if of.width != nf.width {
                    changes.push(FieldChange::Rewidth {
                        id: nf.id,
                        from: of.width,
                        to: nf.width,
                    });
                }
            }
        }
    }
    for of in &old.fields {
        if find(&new.fields, of.id).is_none() {
            changes.push(FieldChange::Removed(of.id));
        }
    }
    SchemaDiff { changes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::field::FieldType;

    #[test]
    fn appended_is_compatible() {
        let old = Schema::new(1, 1, vec![FieldSpec::new(1, FieldType::U32, 0)]);
        let new = Schema::new(
            1,
            2,
            vec![FieldSpec::new(1, FieldType::U32, 0), FieldSpec::new(2, FieldType::U16, 0)],
        );
        let d = diff(&old, &new);
        assert!(d.is_backward_compatible());
        assert_eq!(d.breaking_changes(), 0);
    }

    #[test]
    fn retype_is_breaking() {
        let old = Schema::new(1, 1, vec![FieldSpec::new(1, FieldType::U32, 0)]);
        let new = Schema::new(1, 2, vec![FieldSpec::new(1, FieldType::U64, 0)]);
        let d = diff(&old, &new);
        assert_eq!(d.breaking_changes(), 1);
    }
}
