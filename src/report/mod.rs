//! Human-readable report rendering.
//!
//! Turns a [`crate::query::ResultSet`] or a [`crate::analytics::FlowSummary`]
//! into an aligned plain-text table. Kept separate from the analytics that
//! produce the numbers so the formatting can evolve independently.

use std::fmt::Write as _;

use crate::analytics::FlowSummary;
use crate::net::fmt_ipv4;
use crate::query::ResultSet;

/// A column specification for the table renderer.
struct Column {
    header: &'static str,
    width: usize,
    right: bool,
}

fn pad(s: &str, width: usize, right: bool) -> String {
    if s.len() >= width {
        return s[..width].to_string();
    }
    let fill = width - s.len();
    if right {
        format!("{}{}", " ".repeat(fill), s)
    } else {
        format!("{}{}", s, " ".repeat(fill))
    }
}

fn render_header(out: &mut String, cols: &[Column]) {
    for (i, c) in cols.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(&pad(c.header, c.width, c.right));
    }
    out.push('\n');
    for (i, c) in cols.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(&"-".repeat(c.width));
    }
    out.push('\n');
}

/// Render a flow result set as a table.
pub fn render_flows(rs: &ResultSet) -> String {
    let cols = [
        Column { header: "flow", width: 10, right: true },
        Column { header: "src", width: 15, right: false },
        Column { header: "dst", width: 15, right: false },
        Column { header: "octets", width: 12, right: true },
        Column { header: "packets", width: 10, right: true },
        Column { header: "records", width: 8, right: true },
    ];
    let mut out = String::new();
    render_header(&mut out, &cols);
    for r in &rs.rows {
        let _ = write!(
            out,
            "{}  {}  {}  {}  {}  {}\n",
            pad(&r.key.flow_id.to_string(), cols[0].width, true),
            pad(&fmt_ipv4(r.key.src), cols[1].width, false),
            pad(&fmt_ipv4(r.key.dst), cols[2].width, false),
            pad(&r.octets.to_string(), cols[3].width, true),
            pad(&r.packets.to_string(), cols[4].width, true),
            pad(&r.records.to_string(), cols[5].width, true),
        );
    }
    let _ = write!(
        out,
        "\n{} flow(s) shown of {} matched ({} scanned)\n",
        rs.rows.len(),
        rs.total_matched,
        rs.scanned
    );
    for a in &rs.aggregates {
        let _ = writeln!(out, "  {} = {}", a.label(), a.value());
    }
    out
}

/// Render summary analytics.
pub fn render_summary(summary: &FlowSummary) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "flows observed : {}", summary.flows_seen());
    let _ = writeln!(
        out,
        "octets  p50/p90/p99 : {} / {} / {}",
        summary.octet_hist.percentile(0.50),
        summary.octet_hist.percentile(0.90),
        summary.octet_hist.percentile(0.99),
    );
    let _ = writeln!(
        out,
        "packets p50/p90/p99 : {} / {} / {}",
        summary.packet_hist.percentile(0.50),
        summary.packet_hist.percentile(0.90),
        summary.packet_hist.percentile(0.99),
    );
    out.push_str("top sources:\n");
    for (key, weight) in summary.top_sources.ranked().iter().take(5) {
        let _ = writeln!(out, "  {:<15} {}", fmt_ipv4(*key as u32), weight);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowKey, FlowRecord};
    use crate::query::Query;

    #[test]
    fn renders_table() {
        let mut r = FlowRecord::new(FlowKey::new(0x0100_0001, 0x0100_0002, 7, 1, 2, 6), 0);
        r.accumulate(1234, 10, 100);
        let rs = Query::new().run(&[r]);
        let text = render_flows(&rs);
        assert!(text.contains("flow"));
        assert!(text.contains("1234"));
        assert!(text.contains("1 flow(s) shown"));
    }
}
