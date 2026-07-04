//! Symbol table for the transform VM.
//!
//! A symbol names a callable region of a module's bytecode. Symbols are stored
//! by `(module id, offset, length)` rather than by pointer, so a module can be
//! reloaded and every symbol re-resolves against the current bytecode.

use std::collections::HashMap;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy)]
pub struct Symbol {
    pub module_id: u32,
    pub offset: u32,
    pub len: u32,
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    by_id: HashMap<u32, Symbol>,
}

impl SymbolTable {
    pub fn new() -> SymbolTable {
        SymbolTable { by_id: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn define(&mut self, sym_id: u32, module_id: u32, offset: u32, len: u32) {
        self.by_id.insert(sym_id, Symbol { module_id, offset, len });
    }

    pub fn resolve(&self, sym_id: u32) -> Result<Symbol> {
        self.by_id
            .get(&sym_id)
            .copied()
            .ok_or_else(|| Error::unresolved("unknown symbol").with_context(sym_id as u64))
    }

    /// Forget every symbol belonging to `module_id`. Called when a module is
    /// unloaded so stale symbols cannot be resolved afterwards.
    pub fn forget_module(&mut self, module_id: u32) {
        self.by_id.retain(|_, s| s.module_id != module_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_resolve_forget() {
        let mut t = SymbolTable::new();
        t.define(1, 100, 0, 8);
        assert_eq!(t.resolve(1).unwrap().module_id, 100);
        t.forget_module(100);
        assert!(t.resolve(1).is_err());
    }
}
