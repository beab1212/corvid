//! Record compression codecs and the codec dispatcher.

pub mod bitstream;
pub mod compress;
pub mod delta;
pub mod dictionary;
pub mod frame;
pub mod huffman;
pub mod lz;
pub mod rle;
pub mod varblock;

pub use compress::{Codec, CompressorState};
