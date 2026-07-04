//! Value coercion between decoded scalar types.
//!
//! Filters and exporters frequently want "the integer value of this field"
//! regardless of whether it decoded as a `U8` or a `Timestamp`, or "the bytes"
//! regardless of whether it was `Utf8` or `Bytes`. These helpers centralise the
//! widening and narrowing rules so behaviour is consistent everywhere.

use crate::decode::value::Decoded;

/// Widen any integral value to `u64`. Signed values are sign-extended then
/// reinterpreted, matching the wire semantics.
pub fn as_u64(v: &Decoded) -> Option<u64> {
    Some(match v {
        Decoded::U8(x) => *x as u64,
        Decoded::U16(x) => *x as u64,
        Decoded::U32(x) => *x as u64,
        Decoded::U64(x) => *x,
        Decoded::I32(x) => *x as i64 as u64,
        Decoded::I64(x) => *x as u64,
        Decoded::Timestamp(x) => *x,
        Decoded::Bytes(_) | Decoded::Utf8(_) => return None,
    })
}

/// Interpret an integral value as a signed `i64`.
pub fn as_i64(v: &Decoded) -> Option<i64> {
    Some(match v {
        Decoded::I32(x) => *x as i64,
        Decoded::I64(x) => *x,
        other => as_u64(other)? as i64,
    })
}

/// Borrow the bytes of a byte- or string-valued field.
pub fn as_bytes(v: &Decoded) -> Option<&[u8]> {
    match v {
        Decoded::Bytes(b) => Some(b),
        Decoded::Utf8(s) => Some(s.as_bytes()),
        _ => None,
    }
}

/// Best-effort narrowing to `u32`, saturating on overflow.
pub fn as_u32_saturating(v: &Decoded) -> Option<u32> {
    as_u64(v).map(|x| x.min(u32::MAX as u64) as u32)
}

/// Whether two decoded values are numerically equal, ignoring their concrete
/// integer width.
pub fn numeric_eq(a: &Decoded, b: &Decoded) -> bool {
    match (as_u64(a), as_u64(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Render a value as a compact display string.
pub fn to_display(v: &Decoded) -> String {
    match v {
        Decoded::Bytes(b) => crate::util::hex::encode(b),
        Decoded::Utf8(s) => s.clone(),
        other => as_i64(other).map(|x| x.to_string()).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widening() {
        assert_eq!(as_u64(&Decoded::U8(200)), Some(200));
        assert_eq!(as_i64(&Decoded::I32(-5)), Some(-5));
        assert_eq!(as_u32_saturating(&Decoded::U64(u64::MAX)), Some(u32::MAX));
    }

    #[test]
    fn numeric_equality_across_widths() {
        assert!(numeric_eq(&Decoded::U8(7), &Decoded::U32(7)));
        assert!(!numeric_eq(&Decoded::U8(7), &Decoded::U32(8)));
    }

    #[test]
    fn bytes_display_is_hex() {
        assert_eq!(to_display(&Decoded::Bytes(vec![0xab, 0xcd])), "abcd");
    }
}
