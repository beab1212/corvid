//! Abstract syntax for the filter language.

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn apply_ord(self, ordering: std::cmp::Ordering) -> bool {
        use std::cmp::Ordering::*;
        match (self, ordering) {
            (CmpOp::Eq, Equal) => true,
            (CmpOp::Ne, Equal) => false,
            (CmpOp::Ne, _) => true,
            (CmpOp::Lt, Less) => true,
            (CmpOp::Le, Less) | (CmpOp::Le, Equal) => true,
            (CmpOp::Gt, Greater) => true,
            (CmpOp::Ge, Greater) | (CmpOp::Ge, Equal) => true,
            _ => false,
        }
    }
}

/// A scalar value in a filter: either an integer or a byte string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(u64),
    Str(String),
}

/// A leaf reference to a flow field by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRef {
    pub name: String,
}

/// The expression tree.
#[derive(Debug, Clone)]
pub enum Expr {
    /// `field <op> value`
    Compare { field: FieldRef, op: CmpOp, value: Value },
    /// `field contains "substr"`
    Contains { field: FieldRef, needle: String },
    /// `field in [v1, v2, ...]`
    InSet { field: FieldRef, set: Vec<Value> },
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    /// A literal truth value, produced by constant folding.
    Const(bool),
}

impl Expr {
    /// Depth of the expression tree, used to bound recursion during eval.
    pub fn depth(&self) -> usize {
        match self {
            Expr::And(a, b) | Expr::Or(a, b) => 1 + a.depth().max(b.depth()),
            Expr::Not(e) => 1 + e.depth(),
            _ => 1,
        }
    }

    /// Number of field references, for query-cost estimation.
    pub fn field_refs(&self) -> usize {
        match self {
            Expr::Compare { .. } | Expr::Contains { .. } | Expr::InSet { .. } => 1,
            Expr::And(a, b) | Expr::Or(a, b) => a.field_refs() + b.field_refs(),
            Expr::Not(e) => e.field_refs(),
            Expr::Const(_) => 0,
        }
    }
}
