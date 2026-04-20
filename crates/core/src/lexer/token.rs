//! Cooked tokens emitted by the lexer and consumed by the parser.

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    pub kind: TokenKind,
    pub span: Span,
    pub text: &'a str,
}

impl<'a> Token<'a> {
    pub const fn new(kind: TokenKind, span: Span, text: &'a str) -> Self {
        Self { kind, span, text }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    LevelNumber(u8),
    Identifier,
    Keyword(KeywordKind),
    PictureString,
    StringLiteral,
    NumericLiteral,
    Period,
    LParen,
    RParen,
    Comma,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeywordKind {
    Pic,
    Picture,
    Occurs,
    Redefines,
    Usage,
    Comp,
    Comp3,
    Comp4,
    Comp5,
    Binary,
    Display,
    PackedDecimal,
    Value,
    Times,
    Filler,
    To,
    Is,
    Are,
}

/// Case-insensitive keyword lookup. Input must be ASCII — the scanner
/// only feeds us slices that passed the identifier regex.
pub fn match_keyword(ident: &str) -> Option<KeywordKind> {
    let mut buf = [0u8; 16];
    let bytes = ident.as_bytes();
    if bytes.len() > buf.len() {
        return None;
    }
    for (i, &b) in bytes.iter().enumerate() {
        buf[i] = b.to_ascii_lowercase();
    }
    let lower = &buf[..bytes.len()];
    Some(match lower {
        b"pic" => KeywordKind::Pic,
        b"picture" => KeywordKind::Picture,
        b"occurs" => KeywordKind::Occurs,
        b"redefines" => KeywordKind::Redefines,
        b"usage" => KeywordKind::Usage,
        b"comp" => KeywordKind::Comp,
        b"comp-3" => KeywordKind::Comp3,
        b"comp-4" => KeywordKind::Comp4,
        b"comp-5" => KeywordKind::Comp5,
        b"binary" => KeywordKind::Binary,
        b"display" => KeywordKind::Display,
        b"packed-decimal" => KeywordKind::PackedDecimal,
        b"value" => KeywordKind::Value,
        b"times" => KeywordKind::Times,
        b"filler" => KeywordKind::Filler,
        b"to" => KeywordKind::To,
        b"is" => KeywordKind::Is,
        b"are" => KeywordKind::Are,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_lookup_is_case_insensitive() {
        assert_eq!(match_keyword("PIC"), Some(KeywordKind::Pic));
        assert_eq!(match_keyword("pic"), Some(KeywordKind::Pic));
        assert_eq!(match_keyword("PiCtUrE"), Some(KeywordKind::Picture));
        assert_eq!(match_keyword("COMP-3"), Some(KeywordKind::Comp3));
        assert_eq!(match_keyword("packed-decimal"), Some(KeywordKind::PackedDecimal));
    }

    #[test]
    fn non_keyword_returns_none() {
        assert_eq!(match_keyword("CUSTOMER"), None);
        assert_eq!(match_keyword("AMOUNT-FIELD"), None);
        assert_eq!(match_keyword(""), None);
    }

    #[test]
    fn overlong_input_returns_none() {
        assert_eq!(match_keyword("packed-decimal-extended"), None);
    }
}
