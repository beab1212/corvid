//! CSV rendering of decoded records and flows.

use crate::decode::Decoded;
use crate::flow::FlowRecord;

fn field_into(v: &Decoded, out: &mut String) {
    match v {
        Decoded::U8(x) => out.push_str(&x.to_string()),
        Decoded::U16(x) => out.push_str(&x.to_string()),
        Decoded::U32(x) => out.push_str(&x.to_string()),
        Decoded::U64(x) => out.push_str(&x.to_string()),
        Decoded::I32(x) => out.push_str(&x.to_string()),
        Decoded::I64(x) => out.push_str(&x.to_string()),
        Decoded::Timestamp(x) => out.push_str(&x.to_string()),
        Decoded::Utf8(s) => quote_into(s, out),
        Decoded::Bytes(b) => {
            for byte in b {
                out.push_str(&format!("{byte:02x}"));
            }
        }
    }
}

/// Quote a field if it contains a comma, quote or newline.
fn quote_into(s: &str, out: &mut String) {
    if s.contains([',', '"', '\n', '\r']) {
        out.push('"');
        for c in s.chars() {
            if c == '"' {
                out.push('"');
            }
            out.push(c);
        }
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// One CSV row from a decoded record.
pub fn record_to_csv(values: &[Decoded]) -> String {
    let mut out = String::new();
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        field_into(v, &mut out);
    }
    out
}

/// The canonical flow CSV header.
pub fn flow_header() -> &'static str {
    "src,dst,sport,dport,proto,octets,packets,records,template"
}

pub fn flow_to_csv(rec: &FlowRecord) -> String {
    format!(
        "{},{},{},{},{},{},{},{},{}",
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
    fn quotes_when_needed() {
        let vals = vec![Decoded::Utf8("a,b".into()), Decoded::U32(3)];
        assert_eq!(record_to_csv(&vals), "\"a,b\",3");
    }
}
