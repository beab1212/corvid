//! A tiny assembler for the transform VM.
//!
//! Turns a line-oriented mnemonic source into bytecode the [`super::vm::Vm`] can
//! run. Supports labels for the conditional jump. This is used by tests and the
//! `corvidctl asm` subcommand; it is not on the data path.

use std::collections::HashMap;

use crate::error::{Error, Result};

// These mirror the opcodes in `vm.rs`.
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

/// Assemble `src` into bytecode.
pub fn assemble(src: &str) -> Result<Vec<u8>> {
    // First pass: collect label offsets and a flat instruction list.
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut insns: Vec<Insn> = Vec::new();
    let mut offset = 0usize;

    for (lineno, raw) in src.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(label) = line.strip_suffix(':') {
            let name = label.trim().to_string();
            if labels.insert(name, offset).is_some() {
                return Err(Error::malformed("duplicate label").with_context(lineno as u64 + 1));
            }
            continue;
        }
        let insn = parse_insn(line, lineno + 1)?;
        offset += insn.size();
        insns.push(insn);
    }

    // Second pass: emit, resolving label references.
    let mut code = Vec::with_capacity(offset);
    let mut cursor = 0usize;
    for insn in &insns {
        cursor += insn.size();
        insn.emit(&mut code, &labels, cursor)?;
    }
    Ok(code)
}

fn strip_comment(line: &str) -> &str {
    match line.find(';') {
        Some(i) => &line[..i],
        None => line,
    }
}

enum Insn {
    Simple(u8),
    Imm8(u8, i64),
    ImmW(u8, i64),
    Reg(u8, i64),
    Jump(String),
}

impl Insn {
    fn size(&self) -> usize {
        match self {
            Insn::Simple(_) => 1,
            Insn::Imm8(..) | Insn::Reg(..) | Insn::Jump(_) => 2,
            Insn::ImmW(..) => 3,
        }
    }

    fn emit(&self, out: &mut Vec<u8>, labels: &HashMap<String, usize>, after: usize) -> Result<()> {
        match self {
            Insn::Simple(op) => out.push(*op),
            Insn::Imm8(op, v) => {
                out.push(*op);
                out.push(*v as u8);
            }
            Insn::Reg(op, v) => {
                if *v < 0 || *v >= 16 {
                    return Err(Error::malformed("register out of range"));
                }
                out.push(*op);
                out.push(*v as u8);
            }
            Insn::ImmW(op, v) => {
                out.push(*op);
                out.extend_from_slice(&(*v as u16).to_be_bytes());
            }
            Insn::Jump(label) => {
                let target = *labels
                    .get(label)
                    .ok_or_else(|| Error::unresolved("undefined label"))?;
                let rel = target as i64 - after as i64;
                if rel < i8::MIN as i64 || rel > i8::MAX as i64 {
                    return Err(Error::limit("jump target too far"));
                }
                out.push(OP_JZ);
                out.push(rel as i8 as u8);
            }
        }
        Ok(())
    }
}

fn parse_insn(line: &str, lineno: usize) -> Result<Insn> {
    let mut parts = line.split_whitespace();
    let mnem = parts.next().unwrap_or("");
    let operand = parts.next();

    let want_operand = |o: Option<&str>| -> Result<i64> {
        let s = o.ok_or_else(|| Error::malformed("missing operand").with_context(lineno as u64))?;
        parse_int(s)
    };

    Ok(match mnem.to_ascii_lowercase().as_str() {
        "nop" => Insn::Simple(OP_NOP),
        "add" => Insn::Simple(OP_ADD),
        "sub" => Insn::Simple(OP_SUB),
        "mul" => Insn::Simple(OP_MUL),
        "dup" => Insn::Simple(OP_DUP),
        "drop" => Insn::Simple(OP_DROP),
        "ret" => Insn::Simple(OP_RET),
        "push" => Insn::Imm8(OP_PUSH, want_operand(operand)?),
        "pushw" => Insn::ImmW(OP_PUSHW, want_operand(operand)?),
        "load" => Insn::Reg(OP_LOAD, want_operand(operand)?),
        "store" => Insn::Reg(OP_STORE, want_operand(operand)?),
        "jz" => Insn::Jump(
            operand
                .ok_or_else(|| Error::malformed("jz needs a label"))?
                .to_string(),
        ),
        other => {
            return Err(Error::owned(
                crate::error::Kind::Malformed,
                format!("unknown mnemonic '{other}' at line {lineno}"),
            ))
        }
    })
}

fn parse_int(s: &str) -> Result<i64> {
    if let Some(hex) = s.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).map_err(|_| Error::malformed("bad hex operand"))
    } else {
        s.parse::<i64>().map_err(|_| Error::malformed("bad operand"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_add() {
        let code = assemble("push 2\npush 3\nadd\nret").unwrap();
        assert_eq!(code, vec![OP_PUSH, 2, OP_PUSH, 3, OP_ADD, OP_RET]);
    }

    #[test]
    fn resolves_label() {
        let code = assemble("load 0\njz done\npush 1\ndone:\nret").unwrap();
        // Should contain a JZ opcode.
        assert!(code.contains(&OP_JZ));
    }

    #[test]
    fn rejects_unknown() {
        assert!(assemble("frobnicate 1").is_err());
    }
}
