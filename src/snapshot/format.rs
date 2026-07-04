//! On-disk snapshot format for flow-table state.
//!
//! A snapshot lets the broker persist accumulated flow state and reload it after
//! a restart. The format is self-describing and CRC-protected:
//!
//! ```text
//! magic    : 4 bytes = "CVSS"
//! version  : 1 byte
//! flags    : 1 byte
//! count    : 4 bytes (big-endian) number of flow records
//! records  : count * FLOW_RECORD_LEN bytes
//! crc32    : 4 bytes over everything above
//! ```

use crate::error::{Error, Result};
use crate::flow::{FlowKey, FlowRecord};
use crate::util::checksum::crc32;
use crate::util::{ByteReader, ByteWriter};

pub const SNAP_MAGIC: [u8; 4] = *b"CVSS";
pub const SNAP_VERSION: u8 = 1;
/// Serialised size of one flow record in a snapshot.
pub const FLOW_RECORD_LEN: usize = 4 + 4 + 4 + 2 + 2 + 1 + 8 + 8 + 8 + 8 + 8 + 2 + 4;

/// Serialise a set of flow records into a snapshot buffer.
pub fn encode(records: &[FlowRecord]) -> Vec<u8> {
    let mut w = ByteWriter::new();
    w.bytes(&SNAP_MAGIC).u8(SNAP_VERSION).u8(0).u32(records.len() as u32);
    for r in records {
        w.u32(r.key.src).u32(r.key.dst).u32(r.key.flow_id);
        w.u16(r.key.sport).u16(r.key.dport).u8(r.key.proto);
        w.u64(r.octets).u64(r.packets).u64(r.records);
        w.u64(r.first_ms).u64(r.last_ms);
        w.u16(r.template_id).u32(r.template_gen);
    }
    let crc = crc32(w.as_slice());
    w.u32(crc);
    w.into_vec()
}

/// Parse a snapshot buffer back into flow records, verifying the CRC.
pub fn decode(data: &[u8]) -> Result<Vec<FlowRecord>> {
    if data.len() < 10 + 4 {
        return Err(Error::malformed("snapshot too short"));
    }
    if data[0..4] != SNAP_MAGIC {
        return Err(Error::malformed("bad snapshot magic"));
    }
    let version = data[4];
    if version != SNAP_VERSION {
        return Err(Error::malformed("unsupported snapshot version").with_context(version as u64));
    }
    let body_len = data.len() - 4;
    let stored_crc = u32::from_be_bytes([
        data[body_len],
        data[body_len + 1],
        data[body_len + 2],
        data[body_len + 3],
    ]);
    if crc32(&data[..body_len]) != stored_crc {
        return Err(Error::malformed("snapshot crc mismatch"));
    }

    let mut r = ByteReader::new(&data[..body_len]);
    r.skip(6)?; // magic + version + flags
    let count = r.u32()? as usize;
    if count.saturating_mul(FLOW_RECORD_LEN) > r.remaining() {
        return Err(Error::malformed("snapshot record count exceeds body"));
    }
    let mut out = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let key = FlowKey::new(r.u32()?, r.u32()?, r.u32()?, r.u16()?, r.u16()?, r.u8()?);
        let mut rec = FlowRecord::new(key, 0);
        rec.octets = r.u64()?;
        rec.packets = r.u64()?;
        rec.records = r.u64()?;
        rec.first_ms = r.u64()?;
        rec.last_ms = r.u64()?;
        rec.template_id = r.u16()?;
        rec.template_gen = r.u32()?;
        out.push(rec);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip() {
        let mut a = FlowRecord::new(FlowKey::new(1, 2, 3, 4, 5, 6), 10);
        a.accumulate(500, 3, 20);
        a.bind_template(256, 2, 4);
        let b = FlowRecord::new(FlowKey::from_flow_id(9), 0);
        let buf = encode(&[a, b]);
        let back = decode(&buf).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].octets, 500);
        assert_eq!(back[0].template_id, 256);
    }

    #[test]
    fn corrupt_crc_rejected() {
        let buf = encode(&[FlowRecord::new(FlowKey::from_flow_id(1), 0)]);
        let mut bad = buf.clone();
        let n = bad.len();
        bad[n - 1] ^= 0xFF;
        assert!(decode(&bad).is_err());
    }
}
