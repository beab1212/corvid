//! Typed decoding of record field values, whole records and structured
//! (nested list) data.

pub mod coerce;
pub mod cursor;
pub mod record;
pub mod schema_view;
pub mod structured;
pub mod value;

pub use cursor::RecordCursor;
pub use record::{decode_batch, decode_record, RecordImage};
pub use schema_view::SchemaView;
pub use structured::{decode_structured, Structured};
pub use value::{decode_field, Decoded};
