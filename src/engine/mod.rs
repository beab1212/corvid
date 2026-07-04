//! The record-transform engine: a bounded stack VM, its symbol table and the
//! lexical scope/plugin machinery.

pub mod assembler;
pub mod builtins;
pub mod disasm;
pub mod scope;
pub mod symbol;
pub mod trace;
pub mod vm;

pub use scope::{Plugin, ScopeStack};
pub use symbol::{Symbol, SymbolTable};
pub use vm::{Modules, Vm};
