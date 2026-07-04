//! Constant folding and algebraic simplification for filter expressions.
//!
//! Applied once after parsing so the evaluator never re-walks trivially-true or
//! trivially-false subtrees. Simplification is conservative: it only rewrites
//! nodes whose value is statically known.

use crate::filter::ast::Expr;

/// Simplify `expr`, returning an equivalent but potentially smaller tree.
pub fn simplify(expr: Expr) -> Expr {
    match expr {
        Expr::Not(inner) => {
            let inner = simplify(*inner);
            match inner {
                Expr::Const(b) => Expr::Const(!b),
                Expr::Not(x) => *x, // double negation
                other => Expr::Not(Box::new(other)),
            }
        }
        Expr::And(a, b) => {
            let a = simplify(*a);
            let b = simplify(*b);
            match (a, b) {
                (Expr::Const(false), _) | (_, Expr::Const(false)) => Expr::Const(false),
                (Expr::Const(true), x) | (x, Expr::Const(true)) => x,
                (a, b) => Expr::And(Box::new(a), Box::new(b)),
            }
        }
        Expr::Or(a, b) => {
            let a = simplify(*a);
            let b = simplify(*b);
            match (a, b) {
                (Expr::Const(true), _) | (_, Expr::Const(true)) => Expr::Const(true),
                (Expr::Const(false), x) | (x, Expr::Const(false)) => x,
                (a, b) => Expr::Or(Box::new(a), Box::new(b)),
            }
        }
        leaf => leaf,
    }
}

/// The number of nodes in an expression tree.
pub fn node_count(expr: &Expr) -> usize {
    match expr {
        Expr::And(a, b) | Expr::Or(a, b) => 1 + node_count(a) + node_count(b),
        Expr::Not(e) => 1 + node_count(e),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn and_with_false_collapses() {
        let e = Expr::And(Box::new(Expr::Const(false)), Box::new(Expr::Const(true)));
        assert!(matches!(simplify(e), Expr::Const(false)));
    }

    #[test]
    fn double_negation_removed() {
        let e = Expr::Not(Box::new(Expr::Not(Box::new(Expr::Const(true)))));
        assert!(matches!(simplify(e), Expr::Const(true)));
    }

    #[test]
    fn or_with_true_collapses() {
        let inner = Expr::Contains {
            field: crate::filter::ast::FieldRef { name: "x".into() },
            needle: "y".into(),
        };
        let e = Expr::Or(Box::new(Expr::Const(true)), Box::new(inner));
        assert_eq!(node_count(&simplify(e)), 1);
    }
}
