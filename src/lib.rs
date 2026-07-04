//! # corvid
//!
//! Corvid is an embeddable broker and codec engine for the **Corvid Wire
//! Protocol** (CVWP), a compact length-framed binary protocol for streaming
//! structured records between services.
//!
//! The crate is organised as a set of loosely-coupled subsystems wired together
//! by the [`session`] layer:
//!
//! * [`parser`] — stream framing and zero-copy message decoding.
//! * [`schema`] — schema and template registries with versioning.
//! * [`codec`] — record (de)compression: RLE, delta, varint, canonical Huffman.
//! * [`reassembly`] — fragment reassembly and sliding-window stream buffers.
//! * [`alloc`] — bump arenas and object pools backing the hot decode path.
//! * [`engine`] — a tiny stack VM used for server-side record transforms.
//! * [`flow`] — flow/connection lifecycle and the record accumulation table.
//! * [`session`] — the front door: drives everything from parsed messages.
//!
//! The public entry point most callers want is [`session::Session`].

pub mod alloc;
pub mod analytics;
pub mod codec;
pub mod config;
pub mod decode;
pub mod engine;
pub mod error;
pub mod export;
pub mod filter;
pub mod flow;
pub mod inspect;
pub mod io;
pub mod metrics;
pub mod net;
pub mod parser;
pub mod pipeline;
pub mod proto;
pub mod query;
pub mod reassembly;
pub mod report;
pub mod schema;
pub mod session;
pub mod snapshot;
pub mod store;
pub mod util;
pub mod validate;
pub mod wire;

pub use config::Config;
pub use error::{Error, Kind, Result};
pub use metrics::Metrics;
pub use session::Session;

/// Crate version string, surfaced by the CLI and the `SESSION_OPEN` handshake.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
