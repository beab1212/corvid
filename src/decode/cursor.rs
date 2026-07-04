//! A lazy record cursor.
//!
//! [`super::record::decode_batch`] eagerly materialises every record. When only
//! a subset is needed (say, the first N matching a predicate) a [`RecordCursor`]
//! decodes one record at a time and stops early, avoiding the cost of decoding
//! the tail of a large batch.

use crate::decode::value::decode_field;
use crate::decode::RecordImage;
use crate::error::Result;
use crate::schema::field::FieldSpec;
use crate::util::ByteReader;

pub struct RecordCursor<'a> {
    fields: &'a [FieldSpec],
    reader: ByteReader<'a>,
    decoded: usize,
    max_records: usize,
}

impl<'a> RecordCursor<'a> {
    pub fn new(fields: &'a [FieldSpec], data: &'a [u8]) -> RecordCursor<'a> {
        RecordCursor {
            fields,
            reader: ByteReader::new(data),
            decoded: 0,
            max_records: usize::MAX,
        }
    }

    pub fn with_limit(mut self, max_records: usize) -> RecordCursor<'a> {
        self.max_records = max_records;
        self
    }

    pub fn decoded(&self) -> usize {
        self.decoded
    }

    pub fn bytes_remaining(&self) -> usize {
        self.reader.remaining()
    }

    /// Decode the next record, or `None` once the buffer is exhausted or the
    /// limit is hit.
    pub fn next_record(&mut self) -> Option<Result<RecordImage>> {
        if self.decoded >= self.max_records || self.reader.is_empty() {
            return None;
        }
        let mut values = Vec::with_capacity(self.fields.len());
        for f in self.fields {
            match decode_field(f, &mut self.reader) {
                Ok(v) => values.push(v),
                Err(e) => return Some(Err(e)),
            }
        }
        self.decoded += 1;
        Some(Ok(RecordImage { values }))
    }

    /// Decode until `pred` returns true, returning that record.
    pub fn find<F: Fn(&RecordImage) -> bool>(&mut self, pred: F) -> Result<Option<RecordImage>> {
        while let Some(rec) = self.next_record() {
            let rec = rec?;
            if pred(&rec) {
                return Ok(Some(rec));
            }
        }
        Ok(None)
    }

    /// Count remaining decodable records (consuming the cursor).
    pub fn count(&mut self) -> Result<usize> {
        let mut n = 0;
        while let Some(rec) = self.next_record() {
            rec?;
            n += 1;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoded;
    use crate::schema::field::FieldType;
    use crate::util::ByteWriter;

    fn buf() -> Vec<u8> {
        let mut w = ByteWriter::new();
        for i in 1u32..=5 {
            w.u32(i * 10);
        }
        w.into_vec()
    }

    #[test]
    fn lazy_limit_stops_early() {
        let fields = vec![FieldSpec::new(1, FieldType::U32, 0)];
        let data = buf();
        let mut cur = RecordCursor::new(&fields, &data).with_limit(2);
        assert_eq!(cur.count().unwrap(), 2);
    }

    #[test]
    fn find_predicate() {
        let fields = vec![FieldSpec::new(1, FieldType::U32, 0)];
        let data = buf();
        let mut cur = RecordCursor::new(&fields, &data);
        let found = cur
            .find(|r| matches!(r.get(0), Some(Decoded::U32(v)) if *v >= 30))
            .unwrap();
        assert!(matches!(found.unwrap().get(0), Some(Decoded::U32(30))));
    }
}
