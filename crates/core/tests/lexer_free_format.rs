//! Integration coverage for free-format lexing.
//!
//! Snapshots mirror the fixed-format suite: the full token stream plus
//! any accumulated errors is serialised so a regression in token kinds,
//! span columns, or comment stripping is immediately visible.

use copyforge_core::error::LexerError;
use copyforge_core::lexer::{
    lex,
    token::{Token, TokenKind},
    SourceFormat,
};

fn render(source: &str) -> String {
    use std::fmt::Write;
    let (tokens, errors) = lex(source, SourceFormat::Free);
    let mut out = String::new();
    for t in &tokens {
        writeln!(
            out,
            "{:>3}:{:<2} {:<20} {:?}",
            t.span.line,
            t.span.column,
            format!("{:?}", t.kind),
            t.text
        )
        .unwrap();
    }
    if !errors.is_empty() {
        out.push_str("-- errors --\n");
        for e in &errors {
            writeln!(out, "{e:?}").unwrap();
        }
    }
    out
}

#[test]
fn minimal_record_free() {
    let src = include_str!("../../../fixtures/simple/free-format-basic.cpy");
    insta::assert_snapshot!(render(src));
}

#[test]
fn inline_comment() {
    let src = include_str!("../../../fixtures/simple/free-format-inline-comment.cpy");
    insta::assert_snapshot!(render(src));
}

#[test]
fn inline_comment_inside_literal() {
    // The `*>` inside the string literal must survive as literal
    // content — the preprocessor's quote tracker ignores comment
    // markers while a literal is open.
    let src = "05 A VALUE '*> not a comment'.\n";
    let (tokens, errors) = lex(src, SourceFormat::Free);
    assert!(errors.is_empty(), "{errors:?}");
    let literal =
        tokens.iter().find(|t| t.kind == TokenKind::StringLiteral).expect("string literal token");
    assert_eq!(literal.text, "'*> not a comment'");
    insta::assert_snapshot!(render(src));
}

#[test]
fn long_line_not_truncated() {
    // 100-char identifier on a single free-format line. Fixed format
    // would clip at column 72; free format keeps the whole thing.
    let ident: String = "A".repeat(100);
    let src = format!("{ident}.\n");
    let (tokens, errors) = lex(&src, SourceFormat::Free);
    assert!(errors.is_empty(), "{errors:?}");
    let id: &Token =
        tokens.iter().find(|t| t.kind == TokenKind::Identifier).expect("identifier token");
    assert_eq!(id.text.len(), 100);
    assert_eq!(id.span.column, 1);
}

#[test]
fn content_in_cols_1_through_6_is_code() {
    // Columns 1-6 carry the sequence area in fixed format. In free
    // format they are regular source, so a level-number starting at
    // col 1 must produce a proper LevelNumber token.
    let src = "01 FOO.\n";
    let (tokens, errors) = lex(src, SourceFormat::Free);
    assert!(errors.is_empty(), "{errors:?}");
    let first = &tokens[0];
    assert!(matches!(first.kind, TokenKind::LevelNumber(1)));
    assert_eq!(first.span.line, 1);
    assert_eq!(first.span.column, 1);
}

#[test]
fn star_at_col_1_is_not_special() {
    // Dialect choice: only `*>` starts a comment. A lone `*` is a
    // syntax error (unlike fixed format's indicator column, which
    // treats `*` in col 7 as a whole-line comment). This test pins
    // the behaviour so future edits do not quietly alter it.
    let src = "* not a comment\n";
    let (_, errors) = lex(src, SourceFormat::Free);
    assert!(
        errors.iter().any(|e| matches!(e, LexerError::InvalidCharacter { ch: '*', .. })),
        "expected InvalidCharacter('*'), got {errors:?}"
    );
    insta::assert_snapshot!(render(src));
}

#[test]
fn eof_tracks_physical_line_count() {
    // Matches the fixed-format invariant: EOF line number reflects the
    // physical source even when every line was dropped as a comment.
    let src = "*> first\n*> second\n*> third\n";
    let (tokens, _) = lex(src, SourceFormat::Free);
    let eof = tokens.last().expect("eof token");
    assert_eq!(eof.kind, TokenKind::Eof);
    assert_eq!(eof.span.line, 4);
}
