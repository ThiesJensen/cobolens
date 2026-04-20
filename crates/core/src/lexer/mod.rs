//! Lexical analysis of COBOL copybook source.
//!
//! Two physical-line layouts are supported: the traditional fixed-format
//! card image (cols 1-6 sequence, col 7 indicator, cols 8-72 code) and
//! COBOL 2002 free format (every column is code, inline comments start
//! with `*>`). The caller picks the layout via [`SourceFormat`]; the
//! format-specific preprocessor produces `LogicalLine`s, after which the
//! scanner is format-agnostic.

use crate::error::LexerError;
use crate::lexer::scanner::scan_line;
use crate::lexer::token::{Token, TokenKind};
use crate::span::Span;

pub mod fixed_format;
pub mod scanner;
pub mod token;

/// Selects the physical-line layout of the input.
///
/// The scanner itself is the same for both variants; only the
/// pre-scanning pass differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Fixed,
    Free,
}

pub fn lex(source: &str, format: SourceFormat) -> (Vec<Token>, Vec<LexerError>) {
    let (logical_lines, mut errors) = match format {
        SourceFormat::Fixed => fixed_format::preprocess(source),
        SourceFormat::Free => todo!("free-format preprocessor lands in a follow-up commit"),
    };
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
