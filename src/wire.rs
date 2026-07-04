//! On-the-wire constants for the Corvid Wire Protocol (CVWP).
//!
//! A CVWP stream is a 8-byte stream header followed by a run of length-framed
//! messages. The header carries a magic tag so a mis-pointed reader bails out
//! immediately instead of interpreting arbitrary bytes as a message count.
//!
//! ```text
//! stream:
//!   magic   : 4 bytes  = 0x43 0x56 0x57 0x50 ("CVWP")
//!   version : 1 byte
//!   flags   : 1 byte
//!   count   : 2 bytes  (big-endian) number of messages that follow
//!   message : count times
//!
//! message:
//!   type    : 1 byte    (see MsgType)
//!   flags   : 1 byte
//!   length  : 4 bytes   (big-endian) payload length
//!   payload : length bytes
//! ```

/// Magic tag at the start of every stream: ASCII "CVWP".
pub const MAGIC: [u8; 4] = [0x43, 0x56, 0x57, 0x50];

/// Protocol version this build speaks natively.
pub const VERSION: u8 = 4;

/// Size of the fixed stream header.
pub const STREAM_HEADER_LEN: usize = 8;

/// Size of the fixed per-message header (type, flags, 4-byte length).
pub const MSG_HEADER_LEN: usize = 6;

/// Upper bound on a single message payload (16 MiB). Guards against a bogus
/// length field asking us to buffer the world.
pub const MAX_MSG_LEN: usize = 16 * 1024 * 1024;

/// Upper bound on messages per stream.
pub const MAX_MSG_COUNT: usize = 1 << 16;

/// Stream-header flag: the peer will send a trailing CRC section.
pub const SFLAG_TRAILER_CRC: u8 = 0x01;
/// Stream-header flag: payloads may be individually compressed.
pub const SFLAG_COMPRESSED: u8 = 0x02;

/// Per-message flag: payload is compressed with the session default codec.
pub const MFLAG_COMPRESSED: u8 = 0x01;
/// Per-message flag: message carries a nested continuation record.
pub const MFLAG_CONTINUATION: u8 = 0x02;
/// Per-message flag: message is a control-plane message (not data).
pub const MFLAG_CONTROL: u8 = 0x04;

/// Message type discriminants. Values are stable on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MsgType {
    SessionOpen = 0x01,
    SessionClose = 0x02,
    SchemaDef = 0x03,
    SchemaUpdate = 0x04,
    TemplateDef = 0x05,
    DataRecord = 0x06,
    Fragment = 0x07,
    FragmentReorder = 0x08,
    FlowOpen = 0x09,
    FlowTimeout = 0x0A,
    CompressedBlock = 0x0B,
    SectionHeader = 0x0C,
    ConnOpen = 0x0D,
    ConnReset = 0x0E,
    Continuation = 0x0F,
    ModuleLoad = 0x10,
    ModuleReload = 0x11,
    CallSymbol = 0x12,
    ScopeBegin = 0x13,
    PluginRegister = 0x14,
    ScopeEnd = 0x15,
    PluginInvoke = 0x16,
    CompressConfig = 0x17,
    CompressData = 0x18,
    Request = 0x19,
    ProcessQueue = 0x1A,
    StreamOpen = 0x1B,
    Packet = 0x1C,
    ReadWindow = 0x1D,
    SeekRelative = 0x1E,
    ChannelOpen = 0x1F,
    ChannelTeardown = 0x20,
    ChannelAlloc = 0x21,
    NestedRecord = 0x22,
    SnapshotBegin = 0x23,
    SnapshotCommit = 0x24,
}

impl MsgType {
    pub fn from_u8(v: u8) -> Option<MsgType> {
        use MsgType::*;
        Some(match v {
            0x01 => SessionOpen,
            0x02 => SessionClose,
            0x03 => SchemaDef,
            0x04 => SchemaUpdate,
            0x05 => TemplateDef,
            0x06 => DataRecord,
            0x07 => Fragment,
            0x08 => FragmentReorder,
            0x09 => FlowOpen,
            0x0A => FlowTimeout,
            0x0B => CompressedBlock,
            0x0C => SectionHeader,
            0x0D => ConnOpen,
            0x0E => ConnReset,
            0x0F => Continuation,
            0x10 => ModuleLoad,
            0x11 => ModuleReload,
            0x12 => CallSymbol,
            0x13 => ScopeBegin,
            0x14 => PluginRegister,
            0x15 => ScopeEnd,
            0x16 => PluginInvoke,
            0x17 => CompressConfig,
            0x18 => CompressData,
            0x19 => Request,
            0x1A => ProcessQueue,
            0x1B => StreamOpen,
            0x1C => Packet,
            0x1D => ReadWindow,
            0x1E => SeekRelative,
            0x1F => ChannelOpen,
            0x20 => ChannelTeardown,
            0x21 => ChannelAlloc,
            0x22 => NestedRecord,
            0x23 => SnapshotBegin,
            0x24 => SnapshotCommit,
            _ => return None,
        })
    }

    pub fn is_control(self) -> bool {
        use MsgType::*;
        matches!(
            self,
            SessionOpen
                | SessionClose
                | ConnOpen
                | ConnReset
                | FlowOpen
                | FlowTimeout
                | ScopeBegin
                | ScopeEnd
                | ChannelOpen
                | ChannelTeardown
                | SnapshotBegin
                | SnapshotCommit
        )
    }

    pub fn name(self) -> &'static str {
        use MsgType::*;
        match self {
            SessionOpen => "SESSION_OPEN",
            SessionClose => "SESSION_CLOSE",
            SchemaDef => "SCHEMA_DEF",
            SchemaUpdate => "SCHEMA_UPDATE",
            TemplateDef => "TEMPLATE_DEF",
            DataRecord => "DATA_RECORD",
            Fragment => "FRAGMENT",
            FragmentReorder => "FRAGMENT_REORDER",
            FlowOpen => "FLOW_OPEN",
            FlowTimeout => "FLOW_TIMEOUT",
            CompressedBlock => "COMPRESSED_BLOCK",
            SectionHeader => "SECTION_HEADER",
            ConnOpen => "CONN_OPEN",
            ConnReset => "CONN_RESET",
            Continuation => "CONTINUATION",
            ModuleLoad => "MODULE_LOAD",
            ModuleReload => "MODULE_RELOAD",
            CallSymbol => "CALL_SYMBOL",
            ScopeBegin => "SCOPE_BEGIN",
            PluginRegister => "PLUGIN_REGISTER",
            ScopeEnd => "SCOPE_END",
            PluginInvoke => "PLUGIN_INVOKE",
            CompressConfig => "COMPRESS_CONFIG",
            CompressData => "COMPRESS_DATA",
            Request => "REQUEST",
            ProcessQueue => "PROCESS_QUEUE",
            StreamOpen => "STREAM_OPEN",
            Packet => "PACKET",
            ReadWindow => "READ_WINDOW",
            SeekRelative => "SEEK_RELATIVE",
            ChannelOpen => "CHANNEL_OPEN",
            ChannelTeardown => "CHANNEL_TEARDOWN",
            ChannelAlloc => "CHANNEL_ALLOC",
            NestedRecord => "NESTED_RECORD",
            SnapshotBegin => "SNAPSHOT_BEGIN",
            SnapshotCommit => "SNAPSHOT_COMMIT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_known_types() {
        for v in 0x01u8..=0x24 {
            if let Some(t) = MsgType::from_u8(v) {
                assert_eq!(t as u8, v, "type {v:#x} did not round-trip");
                assert!(!t.name().is_empty());
            }
        }
    }

    #[test]
    fn unknown_type_is_none() {
        assert!(MsgType::from_u8(0x00).is_none());
        assert!(MsgType::from_u8(0xFF).is_none());
    }
}
