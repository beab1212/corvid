//! Templates and the template cache.
//!
//! A template is a lightweight, per-session binding of a record layout to an
//! id. Because peers churn through template ids, the cache is bounded and
//! evicts least-recently-used entries. Rather than call back into dependent
//! subsystems (which would tangle borrows), mutating operations *return* the
//! templates they retired so the owning session can notify the flow table and
//! anything else that references them.

use std::collections::HashMap;

/// A decoded template. `generation` increments each time the same id is
/// redefined so consumers can detect a layout change behind a stable id.
#[derive(Debug, Clone)]
pub struct Template {
    pub id: u16,
    pub schema_id: u16,
    pub generation: u32,
    pub fields: Vec<crate::schema::field::FieldSpec>,
    /// Cached row stride derived from `fields` at definition time.
    pub row_stride: usize,
    /// Logical clock of last use, for LRU ordering.
    pub last_used: u64,
}

impl Template {
    pub fn new(id: u16, schema_id: u16, fields: Vec<crate::schema::field::FieldSpec>) -> Template {
        let row_stride = fields.iter().map(|f| f.row_slot_width()).sum();
        Template { id, schema_id, generation: 1, fields, row_stride, last_used: 0 }
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }
}

/// A bounded, LRU template store keyed by template id.
pub struct TemplateCache {
    map: HashMap<u16, Template>,
    /// Ids in most-recently-used-first order.
    lru: Vec<u16>,
    capacity: usize,
    clock: u64,
    evictions: u64,
}

impl TemplateCache {
    pub fn new(capacity: usize) -> TemplateCache {
        TemplateCache {
            map: HashMap::new(),
            lru: Vec::new(),
            capacity: capacity.max(1),
            clock: 0,
            evictions: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    fn touch(&mut self, id: u16) {
        if let Some(pos) = self.lru.iter().position(|&x| x == id) {
            self.lru.remove(pos);
        }
        self.lru.insert(0, id);
    }

    /// Define or redefine a template. A redefinition carries the generation
    /// forward and replaces the entry. Returns the ids of any templates evicted
    /// to make room, so the caller can drop dependent references.
    pub fn define(&mut self, mut tmpl: Template) -> Vec<u16> {
        self.clock += 1;
        tmpl.last_used = self.clock;
        if let Some(old) = self.map.get(&tmpl.id) {
            tmpl.generation = old.generation + 1;
        }
        let id = tmpl.id;
        self.map.insert(id, tmpl);
        self.touch(id);
        self.enforce_capacity()
    }

    fn enforce_capacity(&mut self) -> Vec<u16> {
        let mut evicted = Vec::new();
        while self.map.len() > self.capacity {
            match self.lru.pop() {
                Some(victim_id) => {
                    if self.map.remove(&victim_id).is_some() {
                        self.evictions += 1;
                        evicted.push(victim_id);
                    }
                }
                None => break,
            }
        }
        evicted
    }

    /// Look up a template, marking it most-recently-used.
    pub fn get(&mut self, id: u16) -> Option<&Template> {
        if self.map.contains_key(&id) {
            self.clock += 1;
            self.touch(id);
            let clock = self.clock;
            let t = self.map.get_mut(&id).unwrap();
            t.last_used = clock;
            Some(&*t)
        } else {
            None
        }
    }

    /// Look up without disturbing LRU order.
    pub fn peek(&self, id: u16) -> Option<&Template> {
        self.map.get(&id)
    }

    /// Explicitly withdraw a template. Returns whether it existed.
    pub fn withdraw(&mut self, id: u16) -> bool {
        if self.map.remove(&id).is_some() {
            if let Some(pos) = self.lru.iter().position(|&x| x == id) {
                self.lru.remove(pos);
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::field::{FieldSpec, FieldType};

    fn tmpl(id: u16) -> Template {
        Template::new(id, 1, vec![FieldSpec::new(1, FieldType::U32, 0)])
    }

    #[test]
    fn redefinition_bumps_generation() {
        let mut c = TemplateCache::new(8);
        c.define(tmpl(10));
        c.define(tmpl(10));
        assert_eq!(c.peek(10).unwrap().generation, 2);
    }

    #[test]
    fn lru_eviction_returns_victim() {
        let mut c = TemplateCache::new(2);
        assert!(c.define(tmpl(1)).is_empty());
        assert!(c.define(tmpl(2)).is_empty());
        let _ = c.get(1); // 1 is MRU, 2 is LRU
        let evicted = c.define(tmpl(3));
        assert_eq!(evicted, vec![2]);
        assert!(c.peek(2).is_none());
    }
}
