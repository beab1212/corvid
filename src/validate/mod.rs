//! Structural validation of schemas, templates and decoded records.
//!
//! Validation is advisory: it surfaces inconsistencies (a field whose width
//! disagrees with the information model, a template referencing an unknown
//! schema) so operators can catch misconfigured exporters, but the decode path
//! itself is defensive and does not rely on validation having run.

use crate::error::{Error, Result};
use crate::schema::field::{FieldSpec, FieldType};
use crate::schema::infomodel;
use crate::schema::Schema;

/// A single validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub field_id: u16,
    pub message: String,
}

/// The outcome of validating a schema.
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn push(&mut self, field_id: u16, message: impl Into<String>) {
        self.findings.push(Finding { field_id, message: message.into() });
    }
}

/// Validate a list of fields against the information model.
pub fn validate_fields(fields: &[FieldSpec]) -> Report {
    let mut report = Report::default();
    let mut seen = std::collections::HashSet::new();
    for f in fields {
        if !seen.insert(f.id) {
            report.push(f.id, "duplicate field id in record");
        }
        if let Some(elem) = infomodel::lookup(f.id) {
            if elem.ty != f.ty {
                report.push(
                    f.id,
                    format!("type {:?} disagrees with model {:?}", f.ty, elem.ty),
                );
            }
            if elem.width != 0 && elem.width != f.width && f.ty == FieldType::Fixed {
                report.push(f.id, format!("width {} disagrees with model {}", f.width, elem.width));
            }
        }
        if f.ty == FieldType::Fixed && f.width == 0 {
            report.push(f.id, "fixed field with zero width");
        }
    }
    report
}

/// Validate a schema as a whole, returning an error only for fatal problems
/// (an empty schema); softer issues are returned as findings.
pub fn validate_schema(schema: &Schema) -> Result<Report> {
    if schema.fields.is_empty() {
        return Err(Error::malformed("schema has no fields"));
    }
    let mut report = validate_fields(&schema.fields);
    if schema.row_width == 0 {
        report.push(0, "schema row width is zero");
    }
    if schema.row_width > 1 << 20 {
        report.push(0, "schema row width improbably large");
    }
    Ok(report)
}

/// Check that a record's byte length is consistent with its template's fixed
/// portion. Returns the number of trailing variable bytes.
pub fn record_length_ok(fields: &[FieldSpec], record_len: usize) -> Result<usize> {
    let mut fixed = 0usize;
    let mut has_var = false;
    for f in fields {
        match f.wire_fixed_width() {
            Some(w) => fixed += w,
            None => has_var = true,
        }
    }
    if record_len < fixed {
        return Err(Error::malformed("record shorter than fixed fields")
            .with_context(record_len as u64));
    }
    let trailing = record_len - fixed;
    if !has_var && trailing != 0 {
        return Err(Error::malformed("fixed record has trailing bytes"));
    }
    Ok(trailing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_width_mismatch() {
        // Element 8 (sourceIPv4Address) is a 4-byte U32; declare it wrong.
        let fields = vec![FieldSpec::new(8, FieldType::Fixed, 16)];
        let report = validate_fields(&fields);
        assert!(!report.is_clean());
    }

    #[test]
    fn record_length_check() {
        let fields = vec![
            FieldSpec::new(1, FieldType::U32, 0),
            FieldSpec::new(2, FieldType::VarBytes, 0),
        ];
        // 4 fixed + some variable.
        assert_eq!(record_length_ok(&fields, 10).unwrap(), 6);
        assert!(record_length_ok(&fields, 2).is_err());
    }
}
