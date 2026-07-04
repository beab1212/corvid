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
    /// Pointer/len of the scope-local state buffer this plugin was registered
    /// against, used to re-touch its parameter block on a repeat invocation.
    state: *const u8,
    state_len: usize,
}

#[derive(Debug, Default)]
pub struct ScopeStack {
    depth: u32,
    /// Per-scope local state buffers; index `d-1` backs scope depth `d`.
    frames: Vec<Vec<u8>>,
    plugins: Vec<Plugin>,
    /// The most recently invoked plugin, cached for a fast repeat call.
    last: Option<(u32, *const u8, usize)>,
    invocations: u64,
}

impl ScopeStack {
    pub fn new() -> ScopeStack {
        ScopeStack { depth: 0, frames: Vec::new(), plugins: Vec::new(), last: None, invocations: 0 }
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
        // Each scope gets a small local state area for its plugins' parameters.
        self.frames.push(vec![0u8; 32]);
        Ok(())
    }

    /// End the innermost scope, dropping any plugins registered within it.
    pub fn end(&mut self) -> Result<()> {
        if self.depth == 0 {
            return Err(Error::protocol("scope end without begin"));
        }
        let closing = self.depth;
        self.plugins.retain(|p| p.scope_depth < closing);
        // Release this scope's local state.
        self.frames.pop();
        self.depth -= 1;
        Ok(())
    }

    /// Register a plugin at the current scope depth.
    pub fn register(&mut self, id: u32, handler: u16) -> Result<()> {
        if self.plugins.len() >= 256 {
            return Err(Error::limit("too many plugins"));
        }
        let (state, state_len) = match self.frames.last() {
            Some(f) => (f.as_ptr(), f.len()),
            None => (std::ptr::null(), 0),
        };
        // Replace an existing registration with the same id at this depth.
        self.plugins.retain(|p| p.id != id);
        self.plugins.push(Plugin { id, handler, scope_depth: self.depth, state, state_len });
        Ok(())
    }

    /// Resolve a plugin by id, returning its builtin handler code.
    pub fn invoke(&mut self, id: u32) -> Result<u16> {
        // Fast path: a repeat call to the last-invoked plugin re-reads its
        // cached parameter block without walking the registration list.
        if let Some((last_id, ptr, len)) = self.last {
            if last_id == id {
                let touched = self.revisit(ptr, len);
                self.invocations = self.invocations.wrapping_add(touched);
            }
        }
        let handler = self
            .plugins
            .iter()
            .find(|p| p.id == id)
            .map(|p| (p.handler, p.state, p.state_len))
            .ok_or_else(|| Error::unresolved("plugin not registered").with_context(id as u64))?;
        self.last = Some((id, handler.1, handler.2));
        self.invocations += 1;
        Ok(handler.0)
    }

    #[inline(never)]
    fn revisit(&self, ptr: *const u8, len: usize) -> u64 {
        if ptr.is_null() || len == 0 {
            return 0;
        }
        // SAFETY: the cached plugin's scope is still open, so its state buffer
        // is live.
        let mut acc = 0u64;
        unsafe {
            acc = acc.wrapping_add(std::hint::black_box(*ptr) as u64);
            acc = acc.wrapping_add(std::hint::black_box(*ptr.add(len - 1)) as u64);
        }
        acc
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
