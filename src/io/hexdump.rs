//! Classic `hexdump -C`-style rendering for debugging wire data.

use std::fmt::Write as _;

/// Render `data` as offset / hex / ASCII columns, 16 bytes per row.
pub fn dump(data: &[u8]) -> String {
    dump_from(data, 0)
}

/// Like [`dump`] but labels offsets starting at `base`.
pub fn dump_from(data: &[u8], base: usize) -> String {
    let mut out = String::new();
    for (row, chunk) in data.chunks(16).enumerate() {
        let off = base + row * 16;
        let _ = write!(out, "{off:08x}  ");
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            match chunk.get(i) {
                Some(b) => {
                    let _ = write!(out, "{b:02x} ");
                }
                None => out.push_str("   "),
            }
        }
        out.push_str(" |");
        for &b in chunk {
            let c = if (0x20..0x7f).contains(&b) { b as char } else { '.' };
            out.push(c);
        }
        out.push_str("|\n");
    }
    out
}

/// Number of rows [`dump`] would produce.
pub fn row_count(len: usize) -> usize {
    len.div_ceil(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_ascii_and_hex() {
        let text = dump(b"hello world!");
        assert!(text.contains("68 65 6c 6c 6f"));
        assert!(text.contains("|hello world!|"));
    }

    #[test]
    fn row_math() {
        assert_eq!(row_count(0), 0);
        assert_eq!(row_count(16), 1);
        assert_eq!(row_count(17), 2);
    }
}
