//! Stream framing: split a byte buffer into a sequence of [`Message`]s.
//!
//! The parser is intentionally strict about the stream header (magic + version)
//! so that a fuzzer feeding random bytes is rejected in the first few bytes
//! rather than wandering into the message loop. Once the header validates,
//! each message is length-checked against the remaining buffer.

use crate::error::{Error, Result};
use crate::parser::message::Message;
use crate::wire::{self, MsgType};

/// Configuration knobs for the framer. Defaults match the protocol maxima.
#[derive(Debug, Clone)]
pub struct FrameConfig {
    pub max_msg_len: usize,
    pub max_msg_count: usize,
    /// If true, an unknown message type aborts the whole stream; if false
    /// (the default) the unknown message is skipped and counted.
    pub strict_types: bool,
}

impl Default for FrameConfig {
    fn default() -> Self {
        FrameConfig {
            max_msg_len: wire::MAX_MSG_LEN,
            max_msg_count: wire::MAX_MSG_COUNT,
            strict_types: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct FrameParser {
    cfg: FrameConfig,
    skipped_unknown: usize,
}

impl FrameParser {
    pub fn new() -> Self {
        FrameParser { cfg: FrameConfig::default(), skipped_unknown: 0 }
    }

    pub fn with_config(cfg: FrameConfig) -> Self {
        FrameParser { cfg, skipped_unknown: 0 }
    }

    pub fn skipped_unknown(&self) -> usize {
        self.skipped_unknown
    }

    /// Validate the stream header and return the declared message count and the
    /// offset at which the first message begins.
    fn parse_header(&self, data: &[u8]) -> Result<(usize, usize)> {
        if data.len() < wire::STREAM_HEADER_LEN {
            return Err(Error::malformed("stream shorter than header"));
        }
        if data[0..4] != wire::MAGIC {
            return Err(Error::malformed("bad magic"));
        }
        let version = data[4];
        if version == 0 || version > wire::VERSION {
            return Err(Error::malformed("unsupported version").with_context(version as u64));
        }
        // data[5] is stream flags; retained for the caller via a separate query.
        let count = u16::from_be_bytes([data[6], data[7]]) as usize;
        if count > self.cfg.max_msg_count {
            return Err(Error::limit("message count too high").with_context(count as u64));
        }
        Ok((count, wire::STREAM_HEADER_LEN))
    }

    /// Read the stream flags byte (valid only after a successful header parse).
    pub fn stream_flags(data: &[u8]) -> u8 {
        if data.len() >= wire::STREAM_HEADER_LEN {
            data[5]
        } else {
            0
        }
    }

    /// Parse every message in `data` into a vector of borrowed views.
    pub fn parse_all<'a>(&mut self, data: &'a [u8]) -> Result<Vec<Message<'a>>> {
        let (count, mut pos) = self.parse_header(data)?;
        let mut out = Vec::with_capacity(count.min(64));

        for _ in 0..count {
            if pos + wire::MSG_HEADER_LEN > data.len() {
                return Err(Error::malformed("truncated message header").with_context(pos as u64));
            }
            let ty_byte = data[pos];
            let flags = data[pos + 1];
            let len = u32::from_be_bytes([
                data[pos + 2],
                data[pos + 3],
                data[pos + 4],
                data[pos + 5],
            ]) as usize;
            pos += wire::MSG_HEADER_LEN;

            if len > self.cfg.max_msg_len {
                return Err(Error::limit("message too long").with_context(len as u64));
            }
            if pos + len > data.len() {
                return Err(Error::malformed("message runs past stream").with_context(len as u64));
            }
            let payload = &data[pos..pos + len];
            let msg_offset = pos;
            pos += len;

            match MsgType::from_u8(ty_byte) {
                Some(ty) => out.push(Message::new(ty, flags, payload, msg_offset)),
                None => {
                    if self.cfg.strict_types {
                        return Err(Error::malformed("unknown message type")
                            .with_context(ty_byte as u64));
                    }
                    self.skipped_unknown += 1;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::ByteWriter;

    fn stream(msgs: &[(u8, u8, &[u8])]) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.bytes(&wire::MAGIC).u8(wire::VERSION).u8(0).u16(msgs.len() as u16);
        for (ty, flags, payload) in msgs {
            w.u8(*ty).u8(*flags).u32(payload.len() as u32).bytes(payload);
        }
        w.into_vec()
    }

    #[test]
    fn parses_two_messages() {
        let data = stream(&[(0x01, 0, &[1, 2, 3]), (0x06, 0, &[4, 5])]);
        let mut p = FrameParser::new();
        let msgs = p.parse_all(&data).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].ty, MsgType::SessionOpen);
        assert_eq!(msgs[0].payload, &[1, 2, 3]);
        assert_eq!(msgs[1].payload, &[4, 5]);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut data = stream(&[(0x01, 0, &[])]);
        data[0] = 0;
        assert!(FrameParser::new().parse_all(&data).is_err());
    }

    #[test]
    fn skips_unknown_type() {
        let data = stream(&[(0xEE, 0, &[9])]);
        let mut p = FrameParser::new();
        let msgs = p.parse_all(&data).unwrap();
        assert_eq!(msgs.len(), 0);
        assert_eq!(p.skipped_unknown(), 1);
    }
}
