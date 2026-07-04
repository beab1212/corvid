//! Custom allocators backing the decode and session layers.
//!
//! Three flavours, each suited to a different lifetime pattern:
//!
//! * [`arena::Arena`] — bump allocation for batch-scoped scratch.
//! * [`pool::RegionPool`] — one flat allocation of fixed-stride rows.
//! * [`slab::Slab`] — generational, handle-addressed storage for long-lived
//!   objects that come and go individually.

pub mod arena;
pub mod pool;
pub mod slab;

pub use arena::Arena;
pub use pool::RegionPool;
pub use slab::{Handle, Slab};
