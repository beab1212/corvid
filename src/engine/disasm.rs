//! Disassembler for VM bytecode, the inverse of [`super::assembler`].
//!
//! Renders bytecode as mnemonics for debugging transforms. Unknown or
//! truncated opcodes are shown as `.byte` directives rather than aborting so a
//! partially-corrupt module can still be examined.

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

/// Disassemble `code`, returning one line per instruction.
pub fn disassemble(code: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut ip = 0usize;
    while ip < code.len() {
        let op = code[ip];
        let addr = ip;
        ip += 1;
        let line = match op {
            OP_NOP => "nop".to_string(),
            OP_ADD => "add".to_string(),
            OP_SUB => "sub".to_string(),
            OP_MUL => "mul".to_string(),
            OP_DUP => "dup".to_string(),
            OP_DROP => "drop".to_string(),
            OP_RET => "ret".to_string(),
            OP_PUSH => operand8(code, &mut ip).map(|v| format!("push {v}")).unwrap_or_else(trunc),
            OP_LOAD => operand8(code, &mut ip).map(|v| format!("load {v}")).unwrap_or_else(trunc),
            OP_STORE => operand8(code, &mut ip).map(|v| format!("store {v}")).unwrap_or_else(trunc),
            OP_PUSHW => operand16(code, &mut ip).map(|v| format!("pushw {v}")).unwrap_or_else(trunc),
            OP_JZ => operand8(code, &mut ip)
                .map(|v| format!("jz {}", v as i8))
                .unwrap_or_else(trunc),
            other => format!(".byte {other:#04x}"),
        };
        out.push(format!("{addr:04x}: {line}"));
    }
    out
}

fn operand8(code: &[u8], ip: &mut usize) -> Option<u8> {
    let v = code.get(*ip).copied();
    if v.is_some() {
        *ip += 1;
    }
    v
}

fn operand16(code: &[u8], ip: &mut usize) -> Option<u16> {
    if *ip + 2 > code.len() {
        return None;
    }
    let v = u16::from_be_bytes([code[*ip], code[*ip + 1]]);
    *ip += 2;
    Some(v)
}

fn trunc() -> String {
    "<truncated operand>".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disassembles_add() {
        let code = vec![OP_PUSH, 2, OP_PUSH, 3, OP_ADD, OP_RET];
        let lines = disassemble(&code);
        assert!(lines[0].contains("push 2"));
        assert!(lines.iter().any(|l| l.contains("add")));
    }

    #[test]
    fn round_trips_with_assembler() {
        let src = "push 5\nload 1\nmul\nret";
        let code = crate::engine::assembler::assemble(src).unwrap();
        let text = disassemble(&code).join("\n");
        assert!(text.contains("push 5"));
        assert!(text.contains("mul"));
    }
}
