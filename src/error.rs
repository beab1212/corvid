//! Error and result types shared across the crate.
//!
//! Corvid distinguishes between *protocol* errors (the peer sent something we
//! cannot make sense of) and *engine* errors (we failed to service a
//! well-formed request). The two are folded into a single [`Error`] enum so
//! that call sites can bubble everything up with `?` while still being able to
//! branch on the kind when it matters — the broker's connection loop, for
//! instance, tears a session down on `Fatal` but merely counts `Malformed`.

use std::fmt;

/// The category of a failure. Kept deliberately coarse; finer detail lives in
/// the message string attached to each variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Bytes on the wire did not match the grammar.
    Malformed,
    /// A referenced object (schema, template, flow, …) was not found.
    Unresolved,
    /// A limit (size, depth, count) was exceeded.
    Limit,
    /// State machine was driven into an invalid transition.
    Protocol,
    /// A codec or transform failed on otherwise-valid input.
    Codec,
    /// Something we treat as unrecoverable for the current connection.
    Fatal,
    /// Ran out of a bounded internal resource.
    Exhausted,
}

impl Kind {
    pub fn is_recoverable(self) -> bool {
        !matches!(self, Kind::Fatal)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Malformed => "malformed",
            Kind::Unresolved => "unresolved",
            Kind::Limit => "limit",
            Kind::Protocol => "protocol",
            Kind::Codec => "codec",
            Kind::Fatal => "fatal",
            Kind::Exhausted => "exhausted",
        }
    }
}

/// A crate-wide error carrying a [`Kind`] and a human-readable detail.
#[derive(Debug, Clone)]
pub struct Error {
    kind: Kind,
    detail: DetailStr,
    /// Optional numeric context — an offset, an id, a length. Purely for logs.
    context: Option<u64>,
}

// Most error strings are static; a few are formatted. Avoid allocating for the
// common static case.
#[derive(Debug, Clone)]
enum DetailStr {
    Static(&'static str),
    Owned(String),
}

impl DetailStr {
    fn as_str(&self) -> &str {
        match self {
            DetailStr::Static(s) => s,
            DetailStr::Owned(s) => s.as_str(),
        }
    }
}

impl Error {
    pub fn new(kind: Kind, detail: &'static str) -> Self {
        Error { kind, detail: DetailStr::Static(detail), context: None }
    }

    pub fn owned(kind: Kind, detail: String) -> Self {
        Error { kind, detail: DetailStr::Owned(detail), context: None }
    }

    pub fn with_context(mut self, ctx: u64) -> Self {
        self.context = Some(ctx);
        self
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn detail(&self) -> &str {
        self.detail.as_str()
    }

    pub fn context(&self) -> Option<u64> {
        self.context
    }

    // Convenience constructors keep call sites terse.
    pub fn malformed(detail: &'static str) -> Self {
        Error::new(Kind::Malformed, detail)
    }
    pub fn unresolved(detail: &'static str) -> Self {
        Error::new(Kind::Unresolved, detail)
    }
    pub fn limit(detail: &'static str) -> Self {
        Error::new(Kind::Limit, detail)
    }
    pub fn protocol(detail: &'static str) -> Self {
        Error::new(Kind::Protocol, detail)
    }
    pub fn codec(detail: &'static str) -> Self {
        Error::new(Kind::Codec, detail)
    }
    pub fn fatal(detail: &'static str) -> Self {
        Error::new(Kind::Fatal, detail)
    }
    pub fn exhausted(detail: &'static str) -> Self {
        Error::new(Kind::Exhausted, detail)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.context {
            Some(c) => write!(f, "{}: {} (ctx={})", self.kind.as_str(), self.detail(), c),
            None => write!(f, "{}: {}", self.kind.as_str(), self.detail()),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::owned(Kind::Fatal, format!("io: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_recoverability() {
        assert!(Kind::Malformed.is_recoverable());
        assert!(!Kind::Fatal.is_recoverable());
    }

    #[test]
    fn context_roundtrip() {
        let e = Error::malformed("bad header").with_context(42);
        assert_eq!(e.context(), Some(42));
        assert_eq!(e.kind(), Kind::Malformed);
        assert!(format!("{e}").contains("42"));
    }
}
