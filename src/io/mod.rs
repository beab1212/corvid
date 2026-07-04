//! Stream IO: multi-stream batching and seekable cursors.

pub mod capture;
pub mod framing;
pub mod hexdump;
pub mod stream;
pub mod tee;

pub use capture::{CaptureReader, CaptureRecord};
pub use framing::BatchReader;
pub use stream::SeekableStream;
pub use tee::{CountingSink, Sink, TeeSink};
