//! A tiny stack machine for server-side record transforms.
//!
//! Modules carry bytecode; symbols name entry points into a module; the VM
//! executes a symbol's region against a small operand stack and register file.
//! Everything is bounded: stack depth, register count and executed-instruction
//! count all have hard caps, so a hostile transform cannot loop forever or grow
//! without limit.

use std::collections::HashMap;

use crate::engine::symbol::SymbolTable;
use crate::error::{Error, Result};

const MAX_STACK: usize = 256;
const REGISTERS: usize = 16;
const MAX_STEPS: usize = 65_536;

// Opcodes.
const OP_NOP: u8 = 0x00;
const OP_PUSH: u8 = 0x01;
const OP_PUSHW: u8 = 0x02;
const OP_ADD: u8 = 0x03;
const OP_SUB: u8 = 0x04;
const OP_MUL: u8 = 0x05;
const OP_DUP: u8 = 0x06;
const OP_DROP: u8 = 0x07;
const OP_LOAD: u8 = 0x08;
const OP_STORE: u8 = 0x09;
const OP_JZ: u8 = 0x0A;
const OP_RET: u8 = 0x0B;

/// The set of loaded modules, keyed by module id.
#[derive(Debug, Default)]
pub struct Modules {
    code: HashMap<u32, Vec<u8>>,
    loads: u64,
    reloads: u64,
}

impl Modules {
    pub fn new() -> Modules {
        Modules { code: HashMap::new(), loads: 0, reloads: 0 }
    }

    pub fn len(&self) -> usize {
        self.code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    pub fn load(&mut self, id: u32, bytecode: Vec<u8>) {
        if self.code.insert(id, bytecode).is_some() {
            self.reloads += 1;
        } else {
            self.loads += 1;
        }
    }

    /// Reload replaces a module's bytecode, allocating a fresh buffer.
    pub fn reload(&mut self, id: u32, bytecode: Vec<u8>) -> Result<()> {
        if !self.code.contains_key(&id) {
            return Err(Error::unresolved("reload of unloaded module"));
        }
        self.code.insert(id, bytecode);
        self.reloads += 1;
        Ok(())
    }

    pub fn unload(&mut self, id: u32) -> bool {
        self.code.remove(&id).is_some()
    }

    pub fn get(&self, id: u32) -> Option<&[u8]> {
        self.code.get(&id).map(|v| v.as_slice())
    }
}

/// A single execution of a symbol.
pub struct Vm {
    stack: Vec<i64>,
    regs: [i64; REGISTERS],
    steps: usize,
}

impl Vm {
    pub fn new() -> Vm {
        Vm { stack: Vec::with_capacity(64), regs: [0; REGISTERS], steps: 0 }
    }

    /// Seed a register with an input value (e.g. a decoded field) before a run.
    pub fn set_input(&mut self, reg: usize, value: i64) {
        if reg < REGISTERS {
            self.regs[reg] = value;
        }
    }

    /// Execute the symbol `sym_id`, resolving its module bytecode on the fly.
    pub fn run(&mut self, modules: &Modules, symtab: &SymbolTable, sym_id: u32) -> Result<i64> {
        let sym = symtab.resolve(sym_id)?;
        let code = modules
            .get(sym.module_id)
            .ok_or_else(|| Error::unresolved("symbol's module gone"))?;
        let start = sym.offset as usize;
        let end = start
            .checked_add(sym.len as usize)
            .ok_or_else(|| Error::malformed("symbol region overflow"))?;
        if end > code.len() {
            return Err(Error::malformed("symbol region past module"));
        }
        self.exec(&code[start..end])
    }

    fn push(&mut self, v: i64) -> Result<()> {
        if self.stack.len() >= MAX_STACK {
            return Err(Error::limit("vm stack overflow"));
        }
        self.stack.push(v);
        Ok(())
    }

    fn pop(&mut self) -> Result<i64> {
        self.stack.pop().ok_or_else(|| Error::protocol("vm stack underflow"))
    }

    fn exec(&mut self, code: &[u8]) -> Result<i64> {
        let mut ip = 0usize;
        while ip < code.len() {
            self.steps += 1;
            if self.steps > MAX_STEPS {
                return Err(Error::limit("vm step budget exhausted"));
            }
            let op = code[ip];
            ip += 1;
            match op {
                OP_NOP => {}
                OP_PUSH => {
                    let v = *code.get(ip).ok_or_else(|| Error::malformed("push operand"))?;
                    ip += 1;
                    self.push(v as i64)?;
                }
                OP_PUSHW => {
                    if ip + 2 > code.len() {
                        return Err(Error::malformed("pushw operand"));
                    }
                    let v = u16::from_be_bytes([code[ip], code[ip + 1]]);
                    ip += 2;
                    self.push(v as i64)?;
                }
                OP_ADD => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(a.wrapping_add(b))?;
                }
                OP_SUB => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(a.wrapping_sub(b))?;
                }
                OP_MUL => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(a.wrapping_mul(b))?;
                }
                OP_DUP => {
                    let a = *self.stack.last().ok_or_else(|| Error::protocol("dup empty"))?;
                    self.push(a)?;
                }
                OP_DROP => {
                    self.pop()?;
                }
                OP_LOAD => {
                    let r = *code.get(ip).ok_or_else(|| Error::malformed("load operand"))? as usize;
                    ip += 1;
                    if r >= REGISTERS {
                        return Err(Error::malformed("register out of range"));
                    }
                    self.push(self.regs[r])?;
                }
                OP_STORE => {
                    let r = *code.get(ip).ok_or_else(|| Error::malformed("store operand"))? as usize;
                    ip += 1;
                    if r >= REGISTERS {
                        return Err(Error::malformed("register out of range"));
                    }
                    self.regs[r] = self.pop()?;
                }
                OP_JZ => {
                    let rel = *code.get(ip).ok_or_else(|| Error::malformed("jz operand"))? as i8;
                    ip += 1;
                    let cond = self.pop()?;
                    if cond == 0 {
                        let target = (ip as i64) + rel as i64;
                        if target < 0 || target as usize > code.len() {
                            return Err(Error::malformed("jz target out of range"));
                        }
                        ip = target as usize;
                    }
                }
                OP_RET => break,
                other => {
                    return Err(Error::malformed("bad opcode").with_context(other as u64));
                }
            }
        }
        Ok(self.stack.last().copied().unwrap_or(0))
    }
}

impl Default for Vm {
    fn default() -> Self {
        Vm::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_program() {
        // PUSH 2; PUSH 3; ADD; RET
        let code = vec![OP_PUSH, 2, OP_PUSH, 3, OP_ADD, OP_RET];
        let mut modules = Modules::new();
        modules.load(1, code);
        let mut symtab = SymbolTable::new();
        symtab.define(10, 1, 0, 6);
        let mut vm = Vm::new();
        assert_eq!(vm.run(&modules, &symtab, 10).unwrap(), 5);
    }

    #[test]
    fn register_transform() {
        // LOAD r0; PUSH 10; MUL; RET
        let code = vec![OP_LOAD, 0, OP_PUSH, 10, OP_MUL, OP_RET];
        let mut modules = Modules::new();
        modules.load(2, code);
        let mut symtab = SymbolTable::new();
        symtab.define(20, 2, 0, 6);
        let mut vm = Vm::new();
        vm.set_input(0, 7);
        assert_eq!(vm.run(&modules, &symtab, 20).unwrap(), 70);
    }

    #[test]
    fn reload_then_run_uses_new_code() {
        let mut modules = Modules::new();
        modules.load(3, vec![OP_PUSH, 1, OP_RET]);
        modules.reload(3, vec![OP_PUSH, 9, OP_RET]).unwrap();
        let mut symtab = SymbolTable::new();
        symtab.define(30, 3, 0, 3);
        let mut vm = Vm::new();
        assert_eq!(vm.run(&modules, &symtab, 30).unwrap(), 9);
    }
}
