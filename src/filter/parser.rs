//! Recursive-descent parser for the filter language.
//!
//! Grammar (lowest to highest precedence):
//! ```text
//! or    := and ('or' and)*
//! and   := unary ('and' unary)*
//! unary := 'not' unary | primary
//! primary := '(' or ')'
//!          | field op value
//!          | field 'contains' string
//!          | field 'in' '[' value (',' value)* ']'
//! ```

use crate::error::{Error, Result};
use crate::filter::ast::{CmpOp, Expr, FieldRef, Value};
use crate::filter::lexer::{Lexer, Token};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Parser {
        Parser { tokens, pos: 0 }
    }

    /// Parse a filter expression from source.
    pub fn parse_str(src: &str) -> Result<Expr> {
        let tokens = Lexer::new(src).tokenize()?;
        let mut p = Parser::new(tokens);
        let expr = p.parse_or()?;
        p.expect(Token::Eof)?;
        Ok(expr)
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        t
    }

    fn expect(&mut self, want: Token) -> Result<()> {
        if *self.peek() == want {
            self.pos += 1;
            Ok(())
        } else {
            Err(Error::malformed("filter: unexpected token"))
        }
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        while *self.peek() == Token::Or {
            self.bump();
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        while *self.peek() == Token::And {
            self.bump();
            let right = self.parse_unary()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if *self.peek() == Token::Not {
            self.bump();
            let inner = self.parse_unary()?;
            return Ok(Expr::Not(Box::new(inner)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        if *self.peek() == Token::LParen {
            self.bump();
            let e = self.parse_or()?;
            self.expect(Token::RParen)?;
            return Ok(e);
        }

        // field ...
        let field = match self.bump() {
            Token::Ident(name) => FieldRef { name },
            _ => return Err(Error::malformed("filter: expected field name")),
        };

        match self.peek().clone() {
            Token::Contains => {
                self.bump();
                match self.bump() {
                    Token::Str(s) => Ok(Expr::Contains { field, needle: s }),
                    _ => Err(Error::malformed("filter: contains needs a string")),
                }
            }
            Token::In => {
                self.bump();
                self.expect(Token::LBracket)?;
                let mut set = Vec::new();
                loop {
                    set.push(self.parse_value()?);
                    match self.bump() {
                        Token::Comma => continue,
                        Token::RBracket => break,
                        _ => return Err(Error::malformed("filter: malformed set")),
                    }
                }
                Ok(Expr::InSet { field, set })
            }
            _ => {
                let op = self.parse_cmp_op()?;
                let value = self.parse_value()?;
                Ok(Expr::Compare { field, op, value })
            }
        }
    }

    fn parse_cmp_op(&mut self) -> Result<CmpOp> {
        Ok(match self.bump() {
            Token::Eq => CmpOp::Eq,
            Token::Ne => CmpOp::Ne,
            Token::Lt => CmpOp::Lt,
            Token::Le => CmpOp::Le,
            Token::Gt => CmpOp::Gt,
            Token::Ge => CmpOp::Ge,
            _ => return Err(Error::malformed("filter: expected comparison operator")),
        })
    }

    fn parse_value(&mut self) -> Result<Value> {
        Ok(match self.bump() {
            Token::Int(v) => Value::Int(v),
            Token::Str(s) => Value::Str(s),
            _ => return Err(Error::malformed("filter: expected value")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_or() {
        let e = Parser::parse_str("octets > 1000 and proto == 6 or packets < 2").unwrap();
        // top level is Or
        assert!(matches!(e, Expr::Or(_, _)));
        assert_eq!(e.field_refs(), 3);
    }

    #[test]
    fn parses_in_set() {
        let e = Parser::parse_str("proto in [6, 17, 1]").unwrap();
        match e {
            Expr::InSet { set, .. } => assert_eq!(set.len(), 3),
            _ => panic!("expected InSet"),
        }
    }

    #[test]
    fn parens_and_not() {
        let e = Parser::parse_str("not (proto == 6)").unwrap();
        assert!(matches!(e, Expr::Not(_)));
    }

    #[test]
    fn rejects_garbage() {
        assert!(Parser::parse_str("and or").is_err());
        assert!(Parser::parse_str("octets >").is_err());
    }
}
