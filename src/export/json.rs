//! A minimal, dependency-free JSON writer for decoded records and flows.
//!
//! We only ever emit a fixed shape (objects of scalars and short arrays), so a
//! hand-rolled writer is smaller and faster than pulling in a serialisation
//! crate — and keeps the fuzz build lean.

use crate::decode::Decoded;
use crate::flow::FlowRecord;

/// Escape a string into `out` following JSON string rules.
pub fn escape_into(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn value_into(v: &Decoded, out: &mut String) {
    match v {
        Decoded::U8(x) => out.push_str(&x.to_string()),
        Decoded::U16(x) => out.push_str(&x.to_string()),
        Decoded::U32(x) => out.push_str(&x.to_string()),
        Decoded::U64(x) => out.push_str(&x.to_string()),
        Decoded::I32(x) => out.push_str(&x.to_string()),
        Decoded::I64(x) => out.push_str(&x.to_string()),
        Decoded::Timestamp(x) => out.push_str(&x.to_string()),
        Decoded::Utf8(s) => escape_into(s, out),
        Decoded::Bytes(b) => {
            out.push('"');
            for byte in b {
                out.push_str(&format!("{byte:02x}"));
            }
            out.push('"');
        }
    }
}

/// Render a decoded record as a JSON array of values, with field names from the
/// info model if the caller supplies field ids.
pub fn record_to_json(values: &[Decoded], field_ids: Option<&[u16]>) -> String {
    let mut out = String::from("{");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let key = match field_ids.and_then(|ids| ids.get(i)) {
            Some(&id) => crate::schema::infomodel::name_of(id).to_string(),
            None => format!("f{i}"),
        };
        escape_into(&key, &mut out);
        out.push(':');
        value_into(v, &mut out);
    }
    out.push('}');
    out
}

/// Render a flow record as a JSON object.
pub fn flow_to_json(rec: &FlowRecord) -> String {
    format!(
        "{{\"src\":{},\"dst\":{},\"sport\":{},\"dport\":{},\"proto\":{},\"octets\":{},\"packets\":{},\"records\":{},\"template\":{}}}",
        rec.key.src,
        rec.key.dst,
        rec.key.sport,
        rec.key.dport,
        rec.key.proto,
        rec.octets,
        rec.packets,
        rec.records,
        rec.template_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_control_chars() {
        let mut s = String::new();
        escape_into("a\"b\n", &mut s);
        assert_eq!(s, "\"a\\\"b\\n\"");
    }

    #[test]
    fn record_json() {
        let vals = vec![Decoded::U32(10), Decoded::Utf8("eth0".into())];
        let j = record_to_json(&vals, None);
        assert!(j.contains("\"f0\":10"));
        assert!(j.contains("\"f1\":\"eth0\""));
    }
}
