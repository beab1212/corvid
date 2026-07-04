//! A generic aligned-text table builder.
//!
//! Unlike [`crate::report`], which knows about flows specifically, this is a
//! dumb grid: push a header and rows of strings and it renders them with
//! per-column width alignment. Used by `corvidctl` for ad-hoc listings.

pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    right_align: Vec<bool>,
}

impl Table {
    pub fn new(headers: &[&str]) -> Table {
        Table {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
            right_align: vec![false; headers.len()],
        }
    }

    pub fn columns(&self) -> usize {
        self.headers.len()
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Mark column `i` as right-aligned (typical for numbers).
    pub fn right_align(mut self, i: usize) -> Table {
        if i < self.right_align.len() {
            self.right_align[i] = true;
        }
        self
    }

    /// Push a row. Rows shorter than the header are padded with blanks; longer
    /// rows are truncated to the column count.
    pub fn push_row(&mut self, cells: Vec<String>) {
        let mut row = cells;
        row.resize(self.headers.len(), String::new());
        self.rows.push(row);
    }

    fn widths(&self) -> Vec<usize> {
        let mut w: Vec<usize> = self.headers.iter().map(|h| h.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if cell.len() > w[i] {
                    w[i] = cell.len();
                }
            }
        }
        w
    }

    fn fmt_cell(&self, i: usize, cell: &str, width: usize) -> String {
        let fill = width.saturating_sub(cell.len());
        if self.right_align[i] {
            format!("{}{}", " ".repeat(fill), cell)
        } else {
            format!("{}{}", cell, " ".repeat(fill))
        }
    }

    pub fn render(&self) -> String {
        let widths = self.widths();
        let mut out = String::new();
        for (i, h) in self.headers.iter().enumerate() {
            if i > 0 {
                out.push_str("  ");
            }
            out.push_str(&self.fmt_cell(i, h, widths[i]));
        }
        out.push('\n');
        for (i, w) in widths.iter().enumerate() {
            if i > 0 {
                out.push_str("  ");
            }
            out.push_str(&"-".repeat(*w));
        }
        out.push('\n');
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    out.push_str("  ");
                }
                out.push_str(&self.fmt_cell(i, cell, widths[i]));
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_aligned() {
        let mut t = Table::new(&["name", "count"]).right_align(1);
        t.push_row(vec!["alpha".into(), "3".into()]);
        t.push_row(vec!["b".into(), "1200".into()]);
        let out = t.render();
        assert!(out.contains("name"));
        assert!(out.lines().count() >= 4);
    }

    #[test]
    fn short_rows_padded() {
        let mut t = Table::new(&["a", "b", "c"]);
        t.push_row(vec!["x".into()]);
        assert_eq!(t.rows[0].len(), 3);
    }
}
