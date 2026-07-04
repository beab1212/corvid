//! Lexical scopes and the plugin registry.
//!
//! The transform engine supports nested scopes (`SCOPE_BEGIN`/`SCOPE_END`) that
//! bound the lifetime of scope-local plugins. A plugin is a named transform
//! callable by id; a scope-local plugin is unregistered automatically when its
//! scope ends, so an invocation after the scope closes resolves to nothing.

use crate::error::{Error, Result};

/// A registered transform plugin. `handler` is an opcode into the VM's builtin
/// dispatch table; the engine does not execute arbitrary native code.
#[derive(Debug, Clone, Copy)]
pub struct Plugin {
    pub id: u32,
    pub handler: u16,
    pub scope_depth: u32,
}

#[derive(Debug, Default)]
pub struct ScopeStack {
    depth: u32,
    plugins: Vec<Plugin>,
    invocations: u64,
}

impl ScopeStack {
    pub fn new() -> ScopeStack {
        ScopeStack { depth: 0, plugins: Vec::new(), invocations: 0 }
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    pub fn begin(&mut self) -> Result<()> {
        if self.depth >= 64 {
            return Err(Error::limit("scope nesting too deep"));
        }
        self.depth += 1;
        Ok(())
    }

    /// End the innermost scope, dropping any plugins registered within it.
    pub fn end(&mut self) -> Result<()> {
        if self.depth == 0 {
            return Err(Error::protocol("scope end without begin"));
        }
        let closing = self.depth;
        self.plugins.retain(|p| p.scope_depth < closing);
        self.depth -= 1;
        Ok(())
    }

    /// Register a plugin at the current scope depth.
    pub fn register(&mut self, id: u32, handler: u16) -> Result<()> {
        if self.plugins.len() >= 256 {
            return Err(Error::limit("too many plugins"));
        }
        // Replace an existing registration with the same id at this depth.
        self.plugins.retain(|p| p.id != id);
        self.plugins.push(Plugin { id, handler, scope_depth: self.depth });
        Ok(())
    }

    /// Resolve a plugin by id, returning its builtin handler code.
    pub fn invoke(&mut self, id: u32) -> Result<u16> {
        let handler = self
            .plugins
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.handler)
            .ok_or_else(|| Error::unresolved("plugin not registered").with_context(id as u64))?;
        self.invocations += 1;
        Ok(handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_local_plugin_expires() {
        let mut s = ScopeStack::new();
        s.begin().unwrap();
        s.register(7, 0x01).unwrap();
        assert!(s.invoke(7).is_ok());
        s.end().unwrap();
        assert!(s.invoke(7).is_err());
    }

    #[test]
    fn unbalanced_end_errors() {
        let mut s = ScopeStack::new();
        assert!(s.end().is_err());
    }
}
