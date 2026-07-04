//! Schema and template management.
//!
//! * A **schema** names and types the columns of a record.
//! * A **template** binds a schema to a concrete on-wire record layout and is
//!   what `DATA_RECORD` messages reference by id.
//!
//! Both live in registries that support versioned redefinition, which is where
//! most of the interesting lifecycle interactions happen.

pub mod catalog;
pub mod diff;
pub mod field;
pub mod infomodel;
pub mod layout;
pub mod registry;
pub mod template;

pub use catalog::Catalog;
pub use layout::{Layout, Placement};
pub use field::{FieldSpec, FieldType};
pub use registry::SchemaRegistry;
pub use template::{Template, TemplateCache};

/// A schema: an ordered set of typed fields plus a precomputed row width.
#[derive(Debug, Clone)]
pub struct Schema {
    pub id: u16,
    pub version: u16,
    pub fields: Vec<FieldSpec>,
    /// Sum of `row_slot_width` across all fields; cached for the decoder.
    pub row_width: usize,
    /// Number of fixed-width leading fields, used to fast-path decode.
    pub fixed_prefix: usize,
}

impl Schema {
    pub fn new(id: u16, version: u16, fields: Vec<FieldSpec>) -> Schema {
        let row_width = fields.iter().map(|f| f.row_slot_width()).sum();
        let fixed_prefix = fields
            .iter()
            .take_while(|f| !f.ty.is_variable())
            .count();
        Schema { id, version, fields, row_width, fixed_prefix }
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub fn has_variable_fields(&self) -> bool {
        self.fixed_prefix < self.fields.len()
    }

    /// Byte offset of field `idx` within a decoded row image.
    pub fn field_offset(&self, idx: usize) -> usize {
        self.fields[..idx].iter().map(|f| f.row_slot_width()).sum()
    }
}
