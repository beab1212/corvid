//! Static descriptors for every CVWP message type.
//!
//! Each [`MsgType`] has a fixed set of properties — which processing category
//! it belongs to, the minimum payload it must carry, and whether it is only
//! legal after a session has been opened. Centralising this here lets the
//! parser and session share one source of truth instead of scattering
//! `match`es across handlers.

use crate::wire::MsgType;

/// Broad handling category, used for routing and metrics bucketing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Session,
    Schema,
    Data,
    Reassembly,
    Flow,
    Compression,
    Connection,
    Engine,
    Stream,
    Channel,
    Snapshot,
}

/// Static properties of a message type.
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    pub ty: MsgType,
    pub category: Category,
    /// Minimum legal payload length in bytes.
    pub min_len: usize,
    /// Whether the message is only valid after `SESSION_OPEN`.
    pub requires_open: bool,
}

impl Descriptor {
    pub fn accepts_len(&self, len: usize) -> bool {
        len >= self.min_len
    }
}

macro_rules! desc {
    ($ty:ident, $cat:ident, $min:expr, $open:expr) => {
        Descriptor {
            ty: MsgType::$ty,
            category: Category::$cat,
            min_len: $min,
            requires_open: $open,
        }
    };
}

/// Return the descriptor for `ty`.
pub fn descriptor(ty: MsgType) -> Descriptor {
    use MsgType::*;
    match ty {
        SessionOpen => desc!(SessionOpen, Session, 8, false),
        SessionClose => desc!(SessionClose, Session, 0, false),
        SchemaDef => desc!(SchemaDef, Schema, 4, true),
        SchemaUpdate => desc!(SchemaUpdate, Schema, 4, true),
        TemplateDef => desc!(TemplateDef, Schema, 4, true),
        DataRecord => desc!(DataRecord, Data, 2, true),
        Fragment => desc!(Fragment, Reassembly, 10, true),
        FragmentReorder => desc!(FragmentReorder, Reassembly, 8, true),
        FlowOpen => desc!(FlowOpen, Flow, 8, true),
        FlowTimeout => desc!(FlowTimeout, Flow, 4, true),
        CompressedBlock => desc!(CompressedBlock, Compression, 5, true),
        SectionHeader => desc!(SectionHeader, Data, 4, true),
        ConnOpen => desc!(ConnOpen, Connection, 4, true),
        ConnReset => desc!(ConnReset, Connection, 4, true),
        Continuation => desc!(Continuation, Data, 0, true),
        ModuleLoad => desc!(ModuleLoad, Engine, 4, true),
        ModuleReload => desc!(ModuleReload, Engine, 4, true),
        CallSymbol => desc!(CallSymbol, Engine, 4, true),
        ScopeBegin => desc!(ScopeBegin, Engine, 0, true),
        PluginRegister => desc!(PluginRegister, Engine, 4, true),
        ScopeEnd => desc!(ScopeEnd, Engine, 0, true),
        PluginInvoke => desc!(PluginInvoke, Engine, 4, true),
        CompressConfig => desc!(CompressConfig, Compression, 1, true),
        CompressData => desc!(CompressData, Compression, 4, true),
        Request => desc!(Request, Connection, 4, true),
        ProcessQueue => desc!(ProcessQueue, Connection, 0, true),
        StreamOpen => desc!(StreamOpen, Stream, 4, true),
        Packet => desc!(Packet, Stream, 2, true),
        ReadWindow => desc!(ReadWindow, Stream, 4, true),
        SeekRelative => desc!(SeekRelative, Stream, 4, true),
        ChannelOpen => desc!(ChannelOpen, Channel, 4, true),
        ChannelTeardown => desc!(ChannelTeardown, Channel, 4, true),
        ChannelAlloc => desc!(ChannelAlloc, Channel, 8, true),
        NestedRecord => desc!(NestedRecord, Data, 0, true),
        SnapshotBegin => desc!(SnapshotBegin, Snapshot, 0, true),
        SnapshotCommit => desc!(SnapshotCommit, Snapshot, 0, true),
    }
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Session => "session",
            Category::Schema => "schema",
            Category::Data => "data",
            Category::Reassembly => "reassembly",
            Category::Flow => "flow",
            Category::Compression => "compression",
            Category::Connection => "connection",
            Category::Engine => "engine",
            Category::Stream => "stream",
            Category::Channel => "channel",
            Category::Snapshot => "snapshot",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_has_descriptor() {
        for v in 0x01u8..=0x24 {
            if let Some(ty) = MsgType::from_u8(v) {
                let d = descriptor(ty);
                assert_eq!(d.ty, ty);
            }
        }
    }

    #[test]
    fn session_open_needs_no_open() {
        assert!(!descriptor(MsgType::SessionOpen).requires_open);
        assert!(descriptor(MsgType::DataRecord).requires_open);
    }

    #[test]
    fn min_len_enforced() {
        assert!(!descriptor(MsgType::SessionOpen).accepts_len(4));
        assert!(descriptor(MsgType::SessionOpen).accepts_len(8));
    }
}
