//! The decoded shape of a single CVWP message.

use crate::wire::MsgType;

/// A borrowed view of one message: its type, flags and payload slice into the
/// original stream buffer. Zero-copy — the payload is never duplicated during
/// parsing.
#[derive(Debug, Clone, Copy)]
pub struct Message<'a> {
    pub ty: MsgType,
    pub flags: u8,
    pub payload: &'a [u8],
    /// Byte offset of this message's payload within the stream. Handy for
    /// diagnostics and for subsystems that record absolute positions.
    pub offset: usize,
}

impl<'a> Message<'a> {
    pub fn new(ty: MsgType, flags: u8, payload: &'a [u8], offset: usize) -> Self {
        Message { ty, flags, payload, offset }
    }

    pub fn is_compressed(&self) -> bool {
        self.flags & crate::wire::MFLAG_COMPRESSED != 0
    }

    pub fn is_continuation(&self) -> bool {
        self.flags & crate::wire::MFLAG_CONTINUATION != 0
    }

    pub fn len(&self) -> usize {
        self.payload.len()
    }

    pub fn is_empty(&self) -> bool {
        self.payload.is_empty()
    }
}

/// A lightweight description used by logging and metrics without borrowing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageTag {
    pub ty: MsgType,
    pub flags: u8,
    pub len: usize,
}

impl<'a> From<&Message<'a>> for MessageTag {
    fn from(m: &Message<'a>) -> Self {
        MessageTag { ty: m.ty, flags: m.flags, len: m.payload.len() }
    }
}
