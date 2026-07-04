//! Human-readable inspection of a CVWP stream.
//!
//! `corvidctl inspect` uses this to dump a stream's structure without running
//! it through a full session — useful when debugging a capture. The output is a
//! best-effort structural view; it does not validate protocol state.

use crate::error::Result;
use crate::parser::FrameParser;
use crate::util::{hex_preview, ByteReader};
use crate::wire::MsgType;

/// A one-line description of a message for the inspector.
#[derive(Debug, Clone)]
pub struct MsgLine {
    pub index: usize,
    pub ty: MsgType,
    pub flags: u8,
    pub len: usize,
    pub detail: String,
}

/// Inspect a stream, returning a line per message.
pub fn inspect_stream(data: &[u8]) -> Result<Vec<MsgLine>> {
    let mut parser = FrameParser::new();
    let messages = parser.parse_all(data)?;
    let mut out = Vec::with_capacity(messages.len());
    for (i, m) in messages.iter().enumerate() {
        out.push(MsgLine {
            index: i,
            ty: m.ty,
            flags: m.flags,
            len: m.payload.len(),
            detail: describe(m.ty, m.payload),
        });
    }
    Ok(out)
}

/// Produce a short, type-specific description of a payload.
fn describe(ty: MsgType, payload: &[u8]) -> String {
    let mut r = ByteReader::new(payload);
    match ty {
        MsgType::SessionOpen => match (r.u32(), r.u32()) {
            (Ok(sid), Ok(feat)) => format!("session={sid:#010x} features={feat:#010x}"),
            _ => "truncated".into(),
        },
        MsgType::SchemaDef | MsgType::SchemaUpdate => match (r.u16(), r.u16()) {
            (Ok(id), Ok(fc)) => format!("schema={id} fields={fc}"),
            _ => "truncated".into(),
        },
        MsgType::TemplateDef => match (r.u16(), r.u16(), r.u16()) {
            (Ok(tid), Ok(sid), Ok(fc)) => format!("template={tid} schema={sid} fields={fc}"),
            _ => "truncated".into(),
        },
        MsgType::DataRecord => match (r.u16(), r.u32()) {
            (Ok(tid), Ok(flow)) => format!("template={tid} flow={flow:#010x}"),
            _ => "truncated".into(),
        },
        MsgType::Fragment => match (r.u32(), r.i32(), r.u16()) {
            (Ok(flow), Ok(off), Ok(len)) => format!("flow={flow:#010x} offset={off} len={len}"),
            _ => "truncated".into(),
        },
        MsgType::FlowOpen | MsgType::FlowTimeout => {
            r.u32().map(|f| format!("flow={f:#010x}")).unwrap_or_else(|_| "truncated".into())
        }
        MsgType::ModuleLoad | MsgType::ModuleReload => {
            r.u32().map(|m| format!("module={m}")).unwrap_or_else(|_| "truncated".into())
        }
        _ => hex_preview(payload, 12),
    }
}

/// Render inspection lines as a multi-line string.
pub fn render(lines: &[MsgLine]) -> String {
    let mut s = String::new();
    for l in lines {
        s.push_str(&format!(
            "[{:>3}] {:<16} flags={:#04x} len={:<5} {}\n",
            l.index,
            l.ty.name(),
            l.flags,
            l.len,
            l.detail
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::ByteWriter;
    use crate::wire;

    #[test]
    fn inspects_a_stream() {
        let mut payload = ByteWriter::new();
        payload.u32(0x1000).u32(0);
        let payload = payload.into_vec();
        let mut w = ByteWriter::new();
        w.bytes(&wire::MAGIC).u8(wire::VERSION).u8(0).u16(1);
        w.u8(0x01).u8(0).u32(payload.len() as u32).bytes(&payload);
        let data = w.into_vec();

        let lines = inspect_stream(&data).unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].detail.contains("session="));
        assert!(render(&lines).contains("SESSION_OPEN"));
    }
}
