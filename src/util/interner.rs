//! A string interner returning small integer symbols.
//!
//! Column names and tag strings repeat across records; interning them lets the
//! rest of the engine compare `u32`s instead of strings and keeps a single
//! canonical copy of each.

use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Interner {
    strings: Vec<String>,
    lookup: HashMap<String, u32>,
}

impl Interner {
    pub fn new() -> Interner {
        Interner { strings: Vec::new(), lookup: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Intern `s`, returning its symbol. Repeated interning is idempotent.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.lookup.get(s) {
            return id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.lookup.insert(s.to_string(), id);
        id
    }

    pub fn resolve(&self, id: u32) -> Option<&str> {
        self.strings.get(id as usize).map(|s| s.as_str())
    }

    pub fn get(&self, s: &str) -> Option<u32> {
        self.lookup.get(s).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent_intern() {
        let mut i = Interner::new();
        let a = i.intern("src");
        let b = i.intern("dst");
        assert_ne!(a, b);
        assert_eq!(i.intern("src"), a);
        assert_eq!(i.resolve(a), Some("src"));
        assert_eq!(i.len(), 2);
    }
}
