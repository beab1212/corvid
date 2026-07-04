//! The schema registry: define, update and resolve schemas by id.
//!
//! Schemas are versioned. `SCHEMA_DEF` installs a new schema (version 1);
//! `SCHEMA_UPDATE` bumps the version and replaces the field list. Consumers
//! that cache anything derived from a schema (row widths, field offsets) must
//! re-read it after an update — the registry exposes the current version so
//! they can tell when their cache is stale.

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::schema::field::FieldSpec;
use crate::schema::Schema;

pub struct SchemaRegistry {
    schemas: HashMap<u16, Schema>,
    /// Interned column names, deduplicated across schemas.
    name_pool: Vec<String>,
    name_index: HashMap<String, u32>,
    defines: u64,
    updates: u64,
}

impl SchemaRegistry {
    pub fn new() -> SchemaRegistry {
        SchemaRegistry {
            schemas: HashMap::new(),
            name_pool: Vec::new(),
            name_index: HashMap::new(),
            defines: 0,
            updates: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    pub fn stats(&self) -> (u64, u64) {
        (self.defines, self.updates)
    }

    /// Intern a column name, returning a stable index into the name pool.
    pub fn intern(&mut self, name: &str) -> u32 {
        if let Some(&idx) = self.name_index.get(name) {
            return idx;
        }
        let idx = self.name_pool.len() as u32;
        self.name_pool.push(name.to_string());
        self.name_index.insert(name.to_string(), idx);
        idx
    }

    pub fn resolve_name(&self, idx: u32) -> Option<&str> {
        self.name_pool.get(idx as usize).map(|s| s.as_str())
    }

    /// Install a fresh schema at version 1, replacing any prior definition.
    pub fn define(&mut self, id: u16, fields: Vec<FieldSpec>) -> Result<u16> {
        if fields.is_empty() {
            return Err(Error::malformed("schema with no fields"));
        }
        if fields.len() > 4096 {
            return Err(Error::limit("too many fields"));
        }
        let schema = Schema::new(id, 1, fields);
        self.schemas.insert(id, schema);
        self.defines += 1;
        Ok(1)
    }

    /// Update an existing schema, bumping its version.
    pub fn update(&mut self, id: u16, fields: Vec<FieldSpec>) -> Result<u16> {
        let prev_version = self
            .schemas
            .get(&id)
            .ok_or_else(|| Error::unresolved("update of undefined schema"))?
            .version;
        if fields.is_empty() {
            return Err(Error::malformed("schema update with no fields"));
        }
        let next = prev_version.wrapping_add(1);
        let schema = Schema::new(id, next, fields);
        self.schemas.insert(id, schema);
        self.updates += 1;
        Ok(next)
    }

    pub fn get(&self, id: u16) -> Option<&Schema> {
        self.schemas.get(&id)
    }

    pub fn current_version(&self, id: u16) -> Option<u16> {
        self.schemas.get(&id).map(|s| s.version)
    }

    pub fn remove(&mut self, id: u16) -> bool {
        self.schemas.remove(&id).is_some()
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        SchemaRegistry::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::field::FieldType;

    #[test]
    fn define_then_update_bumps_version() {
        let mut r = SchemaRegistry::new();
        r.define(5, vec![FieldSpec::new(1, FieldType::U32, 0)]).unwrap();
        assert_eq!(r.current_version(5), Some(1));
        let v = r.update(5, vec![FieldSpec::new(1, FieldType::U64, 0)]).unwrap();
        assert_eq!(v, 2);
        assert_eq!(r.get(5).unwrap().row_width, 8);
    }

    #[test]
    fn update_unknown_errors() {
        let mut r = SchemaRegistry::new();
        assert!(r.update(9, vec![FieldSpec::new(1, FieldType::U8, 0)]).is_err());
    }

    #[test]
    fn interning_dedups() {
        let mut r = SchemaRegistry::new();
        let a = r.intern("src");
        let b = r.intern("src");
        assert_eq!(a, b);
        assert_eq!(r.resolve_name(a), Some("src"));
    }
}
