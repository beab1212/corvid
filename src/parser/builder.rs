//! A fluent builder for CVWP streams.
//!
//! Encoding is the exact inverse of [`super::frame::FrameParser`]. The builder
//! is used by the test suite, the seed generator and anywhere else that needs
//! to synthesise a well-formed stream.

use crate::util::ByteWriter;
use crate::wire::{self, MsgType};

pub struct StreamBuilder {
    version: u8,
    flags: u8,
    messages: Vec<(u8, u8, Vec<u8>)>,
}

impl StreamBuilder {
    pub fn new() -> StreamBuilder {
        StreamBuilder { version: wire::VERSION, flags: 0, messages: Vec::new() }
    }

    pub fn version(mut self, v: u8) -> StreamBuilder {
        self.version = v;
        self
    }

    pub fn stream_flags(mut self, f: u8) -> StreamBuilder {
        self.flags = f;
        self
    }

    /// Append a message by raw type byte.
    pub fn raw(mut self, ty: u8, flags: u8, payload: Vec<u8>) -> StreamBuilder {
        self.messages.push((ty, flags, payload));
        self
    }

    /// Append a message by typed [`MsgType`].
    pub fn msg(self, ty: MsgType, payload: Vec<u8>) -> StreamBuilder {
        self.raw(ty as u8, 0, payload)
    }

    pub fn msg_flags(self, ty: MsgType, flags: u8, payload: Vec<u8>) -> StreamBuilder {
        self.raw(ty as u8, flags, payload)
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Finalise into a byte buffer.
    pub fn build(self) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.bytes(&wire::MAGIC).u8(self.version).u8(self.flags).u16(self.messages.len() as u16);
        for (ty, flags, payload) in &self.messages {
            w.u8(*ty).u8(*flags).u32(payload.len() as u32).bytes(payload);
        }
        w.into_vec()
    }
}

impl Default for StreamBuilder {
    fn default() -> Self {
        StreamBuilder::new()
    }
}

/// Convenience payload builders for the common message shapes.
pub mod payload {
    use crate::util::ByteWriter;

    pub fn session_open(session_id: u32, features: u32) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.u32(session_id).u32(features);
        w.into_vec()
    }

    pub fn session_close(session_id: u32) -> Vec<u8> {
        session_id.to_be_bytes().to_vec()
    }

    pub fn flow_open(flow_id: u32, window: u32) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.u32(flow_id).u32(window);
        w.into_vec()
    }

    pub fn fragment(flow_id: u32, offset: i32, data: &[u8]) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.u32(flow_id).i32(offset).u16(data.len() as u16).bytes(data);
        w.into_vec()
    }

    pub fn data_record(
        template_id: u16,
        flow_id: u32,
        src: u32,
        dst: u32,
        octets: u64,
        packets: u64,
    ) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.u16(template_id).u32(flow_id).u32(src).u32(dst).u16(0).u16(0).u8(6);
        w.u64(octets).u64(packets);
        w.into_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::FrameParser;

    #[test]
    fn builds_parseable_stream() {
        let data = StreamBuilder::new()
            .msg(MsgType::SessionOpen, payload::session_open(1, 0))
            .msg(MsgType::FlowOpen, payload::flow_open(7, 4096))
            .build();
        let mut p = FrameParser::new();
        let msgs = p.parse_all(&data).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].ty, MsgType::FlowOpen);
    }
}
