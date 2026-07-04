//! A serialisable catalog of schemas.
//!
//! The registry holds the *live* schemas for a running session; the catalog is
//! its on-disk representation. Persisting the catalog lets a restarted broker
//! decode records that reference schemas defined before the restart. The wire
//! form mirrors `SCHEMA_DEF` closely so the same reader logic applies:
//!
//! ```text
//! magic    : "CVCT"          (4 bytes)
//! version  : 1 byte
//! count    : u16 schemas
//! schema   : id u16, ver u16, nfields u16, [field...]
//! field    : id u16, type u8, width u16, namelen u8, name bytes
//! ```

use crate::error::{Error, Result};
use crate::schema::field::{FieldSpec, FieldType};
use crate::schema::Schema;
use crate::util::{ByteReader, ByteWriter};

const CATALOG_MAGIC: &[u8; 4] = b"CVCT";
const CATALOG_VERSION: u8 = 1;

#[derive(Debug, Default)]
pub struct Catalog {
    schemas: Vec<Schema>,
}

impl Catalog {
    pub fn new() -> Catalog {
        Catalog { schemas: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    pub fn push(&mut self, schema: Schema) {
        // Replace an existing entry with the same id, keeping the higher version.
        if let Some(existing) = self.schemas.iter_mut().find(|s| s.id == schema.id) {
            if schema.version >= existing.version {
                *existing = schema;
            }
            return;
        }
        self.schemas.push(schema);
    }

    pub fn get(&self, id: u16) -> Option<&Schema> {
        self.schemas.iter().find(|s| s.id == id)
    }

    pub fn schemas(&self) -> &[Schema] {
        &self.schemas
    }

    /// Serialise the catalog.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = ByteWriter::new();
        w.bytes(CATALOG_MAGIC).u8(CATALOG_VERSION).u16(self.schemas.len() as u16);
        for s in &self.schemas {
            w.u16(s.id).u16(s.version).u16(s.fields.len() as u16);
            for f in &s.fields {
                w.u16(f.id).u8(f.ty.code()).u16(f.width);
                let name = f.name.as_bytes();
                let nlen = name.len().min(255) as u8;
                w.u8(nlen).bytes(&name[..nlen as usize]);
            }
        }
        w.into_vec()
    }

    /// Parse a serialised catalog.
    pub fn decode(data: &[u8]) -> Result<Catalog> {
        let mut r = ByteReader::new(data);
        let magic = r.take(4)?;
        if magic != CATALOG_MAGIC {
            return Err(Error::malformed("catalog: bad magic"));
        }
        let version = r.u8()?;
        if version != CATALOG_VERSION {
            return Err(Error::malformed("catalog: unsupported version").with_context(version as u64));
        }
        let count = r.u16()? as usize;
        let mut cat = Catalog::new();
        for _ in 0..count {
            let id = r.u16()?;
            let ver = r.u16()?;
            let nfields = r.u16()? as usize;
            let mut fields = Vec::with_capacity(nfields);
            for _ in 0..nfields {
                let fid = r.u16()?;
                let ty = FieldType::from_code(r.u8()?)?;
                let width = r.u16()?;
                let nlen = r.u8()? as usize;
                let name_bytes = r.take(nlen)?;
                let name = String::from_utf8_lossy(name_bytes).into_owned();
                fields.push(FieldSpec::named(fid, ty, width, name));
            }
            cat.push(Schema::new(id, ver, fields));
        }
        Ok(cat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Schema {
        Schema::new(
            7,
            2,
            vec![
                FieldSpec::named(1, FieldType::U32, 0, "src"),
                FieldSpec::named(2, FieldType::VarBytes, 64, "payload"),
            ],
        )
    }

    #[test]
    fn roundtrip() {
        let mut cat = Catalog::new();
        cat.push(sample());
        let bytes = cat.encode();
        let back = Catalog::decode(&bytes).unwrap();
        assert_eq!(back.len(), 1);
        let s = back.get(7).unwrap();
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0].name, "src");
    }

    #[test]
    fn higher_version_wins() {
        let mut cat = Catalog::new();
        cat.push(Schema::new(1, 1, vec![]));
        cat.push(Schema::new(1, 3, vec![FieldSpec::new(9, FieldType::U8, 0)]));
        assert_eq!(cat.len(), 1);
        assert_eq!(cat.get(1).unwrap().version, 3);
    }

    #[test]
    fn bad_magic_rejected() {
        assert!(Catalog::decode(b"XXXX\x01\x00\x00").is_err());
    }
}
