//! Evaluation of a parsed filter against a source of field values.
//!
//! The evaluator is generic over a [`FieldSource`] so the same compiled filter
//! can run against a live [`crate::flow::FlowRecord`], a decoded record image or
//! a test fixture.

use crate::error::{Error, Result};
use crate::filter::ast::{Expr, Value};
use crate::flow::FlowRecord;

/// A materialised field value produced by a [`FieldSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Int(u64),
    Bytes(Vec<u8>),
    Missing,
}

/// Something a filter can read named fields from.
pub trait FieldSource {
    fn field(&self, name: &str) -> FieldValue;
}

/// Evaluate `expr` against `src`. Recursion is bounded by the expression depth,
/// which the parser already limits.
pub fn eval(expr: &Expr, src: &dyn FieldSource) -> Result<bool> {
    if expr.depth() > 128 {
        return Err(Error::limit("filter too deep to evaluate"));
    }
    Ok(eval_inner(expr, src))
}

fn eval_inner(expr: &Expr, src: &dyn FieldSource) -> bool {
    match expr {
        Expr::Const(b) => *b,
        Expr::Not(e) => !eval_inner(e, src),
        Expr::And(a, b) => eval_inner(a, src) && eval_inner(b, src),
        Expr::Or(a, b) => eval_inner(a, src) || eval_inner(b, src),
        Expr::Compare { field, op, value } => {
            let lhs = src.field(&field.name);
            compare(&lhs, *op, value)
        }
        Expr::Contains { field, needle } => match src.field(&field.name) {
            FieldValue::Bytes(b) => contains(&b, needle.as_bytes()),
            FieldValue::Int(v) => contains(&v.to_be_bytes(), needle.as_bytes()),
            FieldValue::Missing => false,
        },
        Expr::InSet { field, set } => {
            let lhs = src.field(&field.name);
            set.iter().any(|v| compare(&lhs, crate::filter::ast::CmpOp::Eq, v))
        }
    }
}

fn compare(lhs: &FieldValue, op: crate::filter::ast::CmpOp, rhs: &Value) -> bool {
    match (lhs, rhs) {
        (FieldValue::Int(a), Value::Int(b)) => op.apply_ord(a.cmp(b)),
        (FieldValue::Bytes(a), Value::Str(b)) => op.apply_ord(a.as_slice().cmp(b.as_bytes())),
        (FieldValue::Int(a), Value::Str(b)) => {
            op.apply_ord(a.to_be_bytes().as_slice().cmp(b.as_bytes()))
        }
        (FieldValue::Bytes(a), Value::Int(b)) => {
            op.apply_ord(a.as_slice().cmp(b.to_be_bytes().as_slice()))
        }
        (FieldValue::Missing, _) => false,
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A [`FieldSource`] view over a flow record's scalar fields.
pub struct FlowView<'a> {
    pub rec: &'a FlowRecord,
}

impl<'a> FlowView<'a> {
    pub fn new(rec: &'a FlowRecord) -> FlowView<'a> {
        FlowView { rec }
    }
}

impl<'a> FieldSource for FlowView<'a> {
    fn field(&self, name: &str) -> FieldValue {
        let r = self.rec;
        match name {
            "octets" => FieldValue::Int(r.octets),
            "packets" => FieldValue::Int(r.packets),
            "records" => FieldValue::Int(r.records),
            "src" => FieldValue::Int(r.key.src as u64),
            "dst" => FieldValue::Int(r.key.dst as u64),
            "sport" => FieldValue::Int(r.key.sport as u64),
            "dport" => FieldValue::Int(r.key.dport as u64),
            "proto" => FieldValue::Int(r.key.proto as u64),
            "flow_id" => FieldValue::Int(r.key.flow_id as u64),
            "template" => FieldValue::Int(r.template_id as u64),
            "first_ms" => FieldValue::Int(r.first_ms),
            "last_ms" => FieldValue::Int(r.last_ms),
            _ => FieldValue::Missing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::parser::Parser;
    use crate::flow::FlowKey;

    fn rec() -> FlowRecord {
        let mut r = FlowRecord::new(FlowKey::new(0x0a000001, 0x0a000002, 1, 1234, 80, 6), 0);
        r.accumulate(5000, 4, 100);
        r.bind_template(256, 1, 3);
        r
    }

    #[test]
    fn matches_and() {
        let e = Parser::parse_str("octets > 1000 and proto == 6").unwrap();
        let r = rec();
        assert!(eval(&e, &FlowView::new(&r)).unwrap());
    }

    #[test]
    fn non_match() {
        let e = Parser::parse_str("proto == 17").unwrap();
        let r = rec();
        assert!(!eval(&e, &FlowView::new(&r)).unwrap());
    }

    #[test]
    fn in_set() {
        let e = Parser::parse_str("proto in [6, 17]").unwrap();
        let r = rec();
        assert!(eval(&e, &FlowView::new(&r)).unwrap());
    }
}
