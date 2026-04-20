//! Lexical analysis of COBOL copybook source.
//!
//! Today the lexer handles fixed-format input without continuation
//! lines. Free format and continuation (`-` indicator) land in
//! follow-up PRs. The public surface is deliberately tiny: one free
//! function `lex`, returning both the accumulated tokens and every
//! error seen along the way.

use crate::error::LexerError;
use crate::lexer::fixed_format::preprocess;
use crate::lexer::scanner::scan_line;
use crate::lexer::token::{Token, TokenKind};
use crate::span::Span;

pub mod fixed_format;
pub mod scanner;
pub mod token;

pub fn lex(source: &str) -> (Vec<Token>, Vec<LexerError>) {
    let (logical_lines, mut errors) = preprocess(source);
    let mut tokens = Vec::new();
    for line in &logical_lines {
        scan_line(line, &mut tokens, &mut errors);
    }
    // EOF position is derived from the physical source so parser
    // diagnostics at end-of-file point at the real last line even when
    // every line was a comment or was otherwise dropped in preprocess.
    let eof_line = 1 + source.bytes().filter(|b| *b == b'\n').count() as u32;
    tokens.push(Token::new(TokenKind::Eof, Span::new(source.len(), source.len(), eof_line, 1), ""));
    (tokens, errors)
}
