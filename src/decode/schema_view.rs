//! Named access over a decoded record.
//!
//! A [`RecordImage`] is a positional vector of values; pairing it with the
//! [`Schema`] that produced it lets callers look fields up by id or well-known
//! name instead of by index. This is what the filter's record adapter and the
//! JSON exporter use to label columns.

use crate::decode::value::Decoded;
use crate::decode::RecordImage;
use crate::schema::infomodel;
use crate::schema::Schema;

pub struct SchemaView<'a> {
    schema: &'a Schema,
    image: &'a RecordImage,
}

impl<'a> SchemaView<'a> {
    pub fn new(schema: &'a Schema, image: &'a RecordImage) -> SchemaView<'a> {
        SchemaView { schema, image }
    }

    pub fn len(&self) -> usize {
        self.schema.fields.len().min(self.image.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The value of the field with element id `id`, if present.
    pub fn by_id(&self, id: u16) -> Option<&Decoded> {
        let idx = self.schema.fields.iter().position(|f| f.id == id)?;
        self.image.get(idx)
    }

    /// The value of the field named `name`, matching either the field's own
    /// name or its well-known info-model name.
    pub fn by_name(&self, name: &str) -> Option<&Decoded> {
        for (idx, f) in self.schema.fields.iter().enumerate() {
            if f.name == name || infomodel::name_of(f.id) == name {
                return self.image.get(idx);
            }
        }
        None
    }

    /// The display name for field index `idx`.
    pub fn name_at(&self, idx: usize) -> &str {
        match self.schema.fields.get(idx) {
            Some(f) if !f.name.is_empty() => &f.name,
            Some(f) => infomodel::name_of(f.id),
            None => "",
        }
    }

    /// Iterate `(name, value)` pairs.
    pub fn pairs(&self) -> Vec<(&str, &Decoded)> {
        let mut out = Vec::with_capacity(self.len());
        for idx in 0..self.len() {
            if let Some(v) = self.image.get(idx) {
                out.push((self.name_at(idx), v));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::field::{FieldSpec, FieldType};

    #[test]
    fn named_lookup() {
        let schema = Schema::new(
            1,
            1,
            vec![
                FieldSpec::named(1, FieldType::U32, 0, "src_addr"),
                FieldSpec::named(2, FieldType::U16, 0, "src_port"),
            ],
        );
        let image = RecordImage { values: vec![Decoded::U32(7), Decoded::U16(80)] };
        let view = SchemaView::new(&schema, &image);
        assert!(matches!(view.by_name("src_addr"), Some(Decoded::U32(7))));
        assert!(matches!(view.by_id(2), Some(Decoded::U16(80))));
        assert_eq!(view.name_at(0), "src_addr");
        assert_eq!(view.pairs().len(), 2);
    }
}
