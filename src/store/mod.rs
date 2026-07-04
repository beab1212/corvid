//! In-memory storage: a TTL/LRU record cache and a secondary field index.

pub mod index;
pub mod record_store;

pub use index::FieldIndex;
pub use record_store::RecordStore;
