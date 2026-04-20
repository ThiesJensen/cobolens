//! Logos-based scanner that turns each logical line into cooked tokens.
//!
//! The scanner runs once per logical line. It tracks two bits of state
//! in addition to the logos cursor:
//!
//! - `at_line_start`: true until the first token has been emitted for
//!   the current line; used to promote a 2-digit numeric prefix into a
//!   `LevelNumber` token.
//! - PICTURE-mode capture: immediately after emitting `Keyword(Pic)` or
//!   `Keyword(Picture)` the scanner steps outside logos to grab the
//!   next non-whitespace non-period run and emit it as `PictureString`.
//!   This keeps PICTURE strings like `S9(4)V99` atomic instead of
//!   fragmenting them into ident / paren / number tokens.

use logos::Logos;

use crate::error::LexerError;
use crate::lexer::fixed_format::LogicalLine;
use crate::lexer::token::{match_keyword, KeywordKind, Token, TokenKind};

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
#[logos(skip r"[ \t]+")]
enum RawToken {
    #[regex(r"[A-Za-z][A-Za-z0-9_]*(?:-[A-Za-z0-9_]+)*")]
    Identifier,

    #[regex(r"[0-9]+(?:\.[0-9]+)?")]
    Number,

    // COBOL represents a literal quote inside a string by doubling it
    // (`""` / `''`), so the valid-string regex must accept the
    // doubled form as part of the content.
    #[regex(r#""(?:[^"\n]|"")*""#)]
    DoubleString,

    #[regex(r#"'(?:[^'\n]|'')*'"#)]
    SingleString,

    #[regex(r#""(?:[^"\n]|"")*"#, priority = 1)]
    UnterminatedDouble,

    #[regex(r#"'(?:[^'\n]|'')*"#, priority = 1)]
    UnterminatedSingle,

    #[token("\0")]
    NullByte,

    #[token(".")]
    Period,

    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token(",")]
    Comma,
}

pub(crate) fn scan_line(line: &LogicalLine, tokens: &mut Vec<Token>, errors: &mut Vec<LexerError>) {
    let mut lex = RawToken::lexer(&line.text);
    let mut at_line_start = true;

    while let Some(result) = lex.next() {
        let local = lex.span();
        let span = line.map_span(local.clone());
        // Token text comes from the *joined* logical line, not from
        // source[span.start..span.end]: a token that straddles
        // continuation segments would otherwise pick up intervening
        // newlines, indicator columns, and stripped whitespace, which
        // breaks keyword matching and literal content.
        let text = &line.text[local.start..local.end];

        match result {
            Ok(RawToken::Identifier) => {
                let kind =
                    match_keyword(text).map(TokenKind::Keyword).unwrap_or(TokenKind::Identifier);
                tokens.push(Token::new(kind, span, text));
                if matches!(
                    kind,
                    TokenKind::Keyword(KeywordKind::Pic) | TokenKind::Keyword(KeywordKind::Picture)
                ) {
                    capture_picture_string(&mut lex, line, tokens);
                }
            }
            Ok(RawToken::Number) => {
                let kind = if at_line_start && is_level_number(text) {
                    TokenKind::LevelNumber(text.parse().expect("two ASCII digits"))
                } else {
                    TokenKind::NumericLiteral
                };
                tokens.push(Token::new(kind, span, text));
            }
            Ok(RawToken::DoubleString | RawToken::SingleString) => {
                tokens.push(Token::new(TokenKind::StringLiteral, span, text));
            }
            Ok(RawToken::UnterminatedDouble | RawToken::UnterminatedSingle) => {
                errors.push(LexerError::UnterminatedStringLiteral { span });
            }
            Ok(RawToken::NullByte) => {
                errors.push(LexerError::EncounteredNullByte { span });
            }
            Ok(RawToken::Period) => tokens.push(Token::new(TokenKind::Period, span, text)),
            Ok(RawToken::LParen) => tokens.push(Token::new(TokenKind::LParen, span, text)),
            Ok(RawToken::RParen) => tokens.push(Token::new(TokenKind::RParen, span, text)),
            Ok(RawToken::Comma) => tokens.push(Token::new(TokenKind::Comma, span, text)),
            Err(()) => {
                let ch = text.chars().next().unwrap_or('\u{FFFD}');
                errors.push(LexerError::InvalidCharacter { ch, span });
            }
        }
        at_line_start = false;
    }
}

fn is_level_number(text: &str) -> bool {
    text.len() == 2 && text.as_bytes().iter().all(u8::is_ascii_digit)
}

fn capture_picture_string(
    lex: &mut logos::Lexer<'_, RawToken>,
    line: &LogicalLine,
    tokens: &mut Vec<Token>,
) {
    // Skip leading whitespace in remainder.
    while let Some(&b) = lex.remainder().as_bytes().first() {
        if b == b' ' || b == b'\t' {
            lex.bump(1);
        } else {
            break;
        }
    }

    let rem = lex.remainder();
    let end_idx = rem
        .as_bytes()
        .iter()
        .position(|&b| b == b' ' || b == b'\t' || b == b'.')
        .unwrap_or(rem.len());
    if end_idx == 0 {
        return;
    }

    let cursor = lex.source().len() - rem.len();
    let span = line.map_span(cursor..cursor + end_idx);
    let text = &line.text[cursor..cursor + end_idx];
    tokens.push(Token::new(TokenKind::PictureString, span, text));
    lex.bump(end_idx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(text: &str) -> (Vec<Token>, Vec<LexerError>) {
        use crate::lexer::fixed_format::Segment;
        let line = LogicalLine {
            text: text.to_string(),
            segments: vec![Segment {
                logical_start: 0,
                source_start: 0,
                len: text.len(),
                source_line: 1,
                source_col: 1,
            }],
            start_line: 1,
        };
        let mut tokens = vec![];
        let mut errors = vec![];
        scan_line(&line, &mut tokens, &mut errors);
        (tokens, errors)
    }

    #[test]
    fn level_number_only_at_line_start() {
        let (tokens, _) = scan("01 FOO 99.");
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(matches!(kinds[0], TokenKind::LevelNumber(1)));
        assert!(matches!(kinds[1], TokenKind::Identifier));
        assert!(matches!(kinds[2], TokenKind::NumericLiteral));
        assert!(matches!(kinds[3], TokenKind::Period));
    }

    #[test]
    fn picture_string_captured_atomically() {
        let (tokens, _) = scan("05 AMOUNT PIC S9(7)V99.");
        let pic = tokens.iter().find(|t| t.kind == TokenKind::PictureString).unwrap();
        assert_eq!(pic.text, "S9(7)V99");
    }

    #[test]
    fn unterminated_string_reports_error() {
        let (_, errors) = scan("VALUE \"oops");
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], LexerError::UnterminatedStringLiteral { .. }));
    }

    #[test]
    fn doubled_quotes_inside_string_literal_stay_in_one_token() {
        let (tokens, errors) = scan(r#"VALUE "A""B"."#);
        assert!(errors.is_empty(), "{errors:?}");
        let literals: Vec<&str> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::StringLiteral)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(literals, vec![r#""A""B""#]);

        let (tokens, errors) = scan(r#"VALUE 'A''B'."#);
        assert!(errors.is_empty(), "{errors:?}");
        let literals: Vec<&str> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::StringLiteral)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(literals, vec!["'A''B'"]);
    }

    #[test]
    fn invalid_character_recorded_and_scanning_continues() {
        let (tokens, errors) = scan("FOO ~ BAR.");
        assert_eq!(errors.len(), 1);
        assert_eq!(tokens.len(), 3);
        assert!(matches!(errors[0], LexerError::InvalidCharacter { ch: '~', .. }));
    }
}
