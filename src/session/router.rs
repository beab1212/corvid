//! Message pre-routing checks and per-category accounting.
//!
//! Before a message reaches its handler the router validates it against the
//! static [`crate::proto`] descriptor — minimum length and session-open
//! requirement — and tallies traffic by [`crate::proto::Category`]. Handlers can
//! then assume the coarse structural invariants already hold.

use crate::error::{Error, Result};
use crate::proto::{descriptor, Category};
use crate::wire::MsgType;

const CATEGORY_COUNT: usize = 11;

fn category_index(c: Category) -> usize {
    match c {
        Category::Session => 0,
        Category::Schema => 1,
        Category::Data => 2,
        Category::Reassembly => 3,
        Category::Flow => 4,
        Category::Compression => 5,
        Category::Connection => 6,
        Category::Engine => 7,
        Category::Stream => 8,
        Category::Channel => 9,
        Category::Snapshot => 10,
    }
}

#[derive(Debug, Default)]
pub struct Router {
    counts: [u64; CATEGORY_COUNT],
    bytes: [u64; CATEGORY_COUNT],
    rejected: u64,
}

impl Router {
    pub fn new() -> Router {
        Router { counts: [0; CATEGORY_COUNT], bytes: [0; CATEGORY_COUNT], rejected: 0 }
    }

    /// Validate `ty`/`payload_len` against the descriptor and, if session state
    /// requires it, that the session is open. On success the message is
    /// accounted to its category.
    pub fn admit(&mut self, ty: MsgType, payload_len: usize, session_open: bool) -> Result<Category> {
        let d = descriptor(ty);
        if d.requires_open && !session_open {
            self.rejected += 1;
            return Err(Error::malformed("message before session open"));
        }
        if !d.accepts_len(payload_len) {
            self.rejected += 1;
            return Err(Error::malformed("message shorter than minimum")
                .with_context(payload_len as u64));
        }
        let idx = category_index(d.category);
        self.counts[idx] += 1;
        self.bytes[idx] = self.bytes[idx].wrapping_add(payload_len as u64);
        Ok(d.category)
    }

    pub fn count(&self, c: Category) -> u64 {
        self.counts[category_index(c)]
    }

    pub fn bytes(&self, c: Category) -> u64 {
        self.bytes[category_index(c)]
    }

    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    pub fn total(&self) -> u64 {
        self.counts.iter().sum()
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for c in [
            Category::Session,
            Category::Schema,
            Category::Data,
            Category::Reassembly,
            Category::Flow,
            Category::Compression,
            Category::Connection,
            Category::Engine,
            Category::Stream,
            Category::Channel,
            Category::Snapshot,
        ] {
            let i = category_index(c);
            if self.counts[i] > 0 {
                out.push_str(&format!("{}: {} msgs, {} bytes\n", c.name(), self.counts[i], self.bytes[i]));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_before_open() {
        let mut r = Router::new();
        assert!(r.admit(MsgType::DataRecord, 8, false).is_err());
        assert_eq!(r.rejected(), 1);
    }

    #[test]
    fn admits_and_accounts() {
        let mut r = Router::new();
        r.admit(MsgType::SessionOpen, 8, false).unwrap();
        r.admit(MsgType::DataRecord, 16, true).unwrap();
        assert_eq!(r.count(Category::Session), 1);
        assert_eq!(r.count(Category::Data), 1);
        assert_eq!(r.total(), 2);
    }

    #[test]
    fn rejects_short() {
        let mut r = Router::new();
        assert!(r.admit(MsgType::SessionOpen, 2, false).is_err());
    }
}
