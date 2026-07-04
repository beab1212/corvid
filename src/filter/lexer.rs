//! Tokeniser for the flow filter language.
//!
//! The filter language is a small boolean expression grammar over flow fields,
//! e.g. `octets > 1000 and proto == 6 or src == 0x0a000001`. The lexer turns a
//! source string into a flat token stream; the parser in [`super::parser`]
//! turns that into an AST.

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Int(u64),
    Str(String),
    // Operators
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Not,
    LParen,
    RParen,
    Contains,
    In,
    Comma,
    LBracket,
    RBracket,
    Eof,
}

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Lexer<'a> {
        Lexer { src: src.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Produce the entire token stream, terminated by `Eof`.
    pub fn tokenize(mut self) -> Result<Vec<Token>> {
        let mut out = Vec::new();
        loop {
            let t = self.next_token()?;
            let done = t == Token::Eof;
            out.push(t);
            if done {
                break;
            }
            if out.len() > 4096 {
                return Err(Error::limit("filter expression too long"));
            }
        }
        Ok(out)
    }

    fn next_token(&mut self) -> Result<Token> {
        self.skip_ws();
        let c = match self.peek() {
            Some(c) => c,
            None => return Ok(Token::Eof),
        };
        match c {
            b'(' => {
                self.pos += 1;
                Ok(Token::LParen)
            }
            b')' => {
                self.pos += 1;
                Ok(Token::RParen)
            }
            b'[' => {
                self.pos += 1;
                Ok(Token::LBracket)
            }
            b']' => {
                self.pos += 1;
                Ok(Token::RBracket)
            }
            b',' => {
                self.pos += 1;
                Ok(Token::Comma)
            }
            b'=' => {
                self.pos += 1;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                }
                Ok(Token::Eq)
            }
            b'!' => {
                self.pos += 1;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Ok(Token::Ne)
                } else {
                    Ok(Token::Not)
                }
            }
            b'<' => {
                self.pos += 1;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Ok(Token::Le)
                } else {
                    Ok(Token::Lt)
                }
            }
            b'>' => {
                self.pos += 1;
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Ok(Token::Ge)
                } else {
                    Ok(Token::Gt)
                }
            }
            b'"' | b'\'' => self.lex_string(c),
            b'0'..=b'9' => self.lex_number(),
            c if c == b'_' || c.is_ascii_alphabetic() => Ok(self.lex_ident()),
            other => Err(Error::malformed("filter: unexpected char").with_context(other as u64)),
        }
    }

    fn lex_string(&mut self, quote: u8) -> Result<Token> {
        self.pos += 1; // opening quote
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == quote {
                let s = std::str::from_utf8(&self.src[start..self.pos])
                    .map_err(|_| Error::malformed("filter: bad utf8 in string"))?
                    .to_string();
                self.pos += 1; // closing quote
                return Ok(Token::Str(s));
            }
            self.pos += 1;
        }
        Err(Error::malformed("filter: unterminated string"))
    }

    fn lex_number(&mut self) -> Result<Token> {
        let start = self.pos;
        // Hex?
        if self.peek() == Some(b'0') && self.src.get(self.pos + 1) == Some(&b'x') {
            self.pos += 2;
            let hstart = self.pos;
            while let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            let s = std::str::from_utf8(&self.src[hstart..self.pos]).unwrap_or("");
            let v = u64::from_str_radix(s, 16)
                .map_err(|_| Error::malformed("filter: bad hex literal"))?;
            return Ok(Token::Int(v));
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("");
        let v = s.parse::<u64>().map_err(|_| Error::malformed("filter: bad int literal"))?;
        Ok(Token::Int(v))
    }

    fn lex_ident(&mut self) -> Token {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'_' || c == b'.' || c.is_ascii_alphanumeric() {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
        match s.as_str() {
            "and" | "AND" => Token::And,
            "or" | "OR" => Token::Or,
            "not" | "NOT" => Token::Not,
            "contains" => Token::Contains,
            "in" => Token::In,
            _ => Token::Ident(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_comparison() {
        let toks = Lexer::new("octets > 1000 and proto == 6").tokenize().unwrap();
        assert_eq!(toks[0], Token::Ident("octets".into()));
        assert_eq!(toks[1], Token::Gt);
        assert_eq!(toks[2], Token::Int(1000));
        assert_eq!(toks[3], Token::And);
    }

    #[test]
    fn hex_and_strings() {
        let toks = Lexer::new("src == 0x0a000001 and name contains \"eth\"").tokenize().unwrap();
        assert!(toks.contains(&Token::Int(0x0a000001)));
        assert!(toks.contains(&Token::Contains));
        assert!(toks.contains(&Token::Str("eth".into())));
    }
}
