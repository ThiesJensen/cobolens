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

pub fn lex(source: &str) -> (Vec<Token<'_>>, Vec<LexerError>) {
    let (logical_lines, mut errors) = preprocess(source);
    let mut tokens = Vec::new();
    let mut last_line_no: u32 = 0;
    for line in &logical_lines {
        scan_line(line, source, &mut tokens, &mut errors);
        last_line_no = line.line_no;
    }
    tokens.push(Token::new(
        TokenKind::Eof,
        Span::new(source.len(), source.len(), last_line_no.saturating_add(1).max(1), 1),
        "",
    ));
    (tokens, errors)
}
