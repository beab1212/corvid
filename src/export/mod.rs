//! Rendering decoded records and flows to human- and machine-readable formats.

pub mod csv;
pub mod json;
pub mod table;

pub use table::Table;

use crate::flow::FlowRecord;

/// Output format selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Csv,
    Text,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s {
            "json" => Format::Json,
            "csv" => Format::Csv,
            "text" | "txt" => Format::Text,
            _ => return None,
        })
    }
}

/// A batching writer that accumulates rendered rows and can emit a header.
pub struct FlowWriter {
    format: Format,
    buf: String,
    rows: usize,
    header_written: bool,
}

impl FlowWriter {
    pub fn new(format: Format) -> FlowWriter {
        FlowWriter { format, buf: String::new(), rows: 0, header_written: false }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn push(&mut self, rec: &FlowRecord) {
        match self.format {
            Format::Json => {
                self.buf.push_str(&json::flow_to_json(rec));
                self.buf.push('\n');
            }
            Format::Csv => {
                if !self.header_written {
                    self.buf.push_str(csv::flow_header());
                    self.buf.push('\n');
                    self.header_written = true;
                }
                self.buf.push_str(&csv::flow_to_csv(rec));
                self.buf.push('\n');
            }
            Format::Text => {
                self.buf.push_str(&format!(
                    "{}:{} -> {}:{} proto={} {}o/{}p\n",
                    rec.key.src, rec.key.sport, rec.key.dst, rec.key.dport, rec.key.proto,
                    rec.octets, rec.packets,
                ));
            }
        }
        self.rows += 1;
    }

    pub fn finish(self) -> String {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowKey;

    #[test]
    fn csv_writes_header_once() {
        let mut w = FlowWriter::new(Format::Csv);
        for i in 0..3u32 {
            let mut r = FlowRecord::new(FlowKey::from_flow_id(i), 0);
            r.accumulate(10, 1, 0);
            w.push(&r);
        }
        let out = w.finish();
        assert_eq!(out.matches("src,dst").count(), 1);
        assert_eq!(out.lines().count(), 4); // header + 3 rows
    }
}
