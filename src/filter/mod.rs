//! A small boolean filter language over flow fields.
//!
//! Used by the CLI (`corvidctl`) and the export path to select which flows to
//! emit. Compile a filter once with [`Filter::compile`] and evaluate it against
//! many records.

pub mod ast;
pub mod eval;
pub mod functions;
pub mod lexer;
pub mod optimize;
pub mod parser;

pub use ast::Expr;
pub use eval::{FieldSource, FieldValue, FlowView};

use crate::error::Result;

/// A compiled filter ready to evaluate.
pub struct Filter {
    expr: Expr,
}

impl Filter {
    pub fn compile(src: &str) -> Result<Filter> {
        let expr = optimize::simplify(parser::Parser::parse_str(src)?);
        Ok(Filter { expr })
    }

    pub fn matches(&self, src: &dyn FieldSource) -> bool {
        eval::eval(&self.expr, src).unwrap_or(false)
    }

    pub fn cost(&self) -> usize {
        self.expr.field_refs()
    }

    pub fn expr(&self) -> &Expr {
        &self.expr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowKey, FlowRecord};

    #[test]
    fn compile_and_match() {
        let f = Filter::compile("octets >= 100").unwrap();
        let mut r = FlowRecord::new(FlowKey::from_flow_id(1), 0);
        r.accumulate(100, 1, 0);
        assert!(f.matches(&FlowView::new(&r)));
        assert_eq!(f.cost(), 1);
    }
}
