//! Integration coverage for free-format lexing.
//!
//! Snapshots mirror the fixed-format suite: the full token stream plus
//! any accumulated errors is serialised so a regression in token kinds,
//! span columns, or comment stripping is immediately visible.

use copyforge_core::error::LexerError;
use copyforge_core::lexer::{lex, token::TokenKind, SourceFormat};

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
    let id = tokens.iter().find(|t| t.kind == TokenKind::Identifier).expect("identifier token");
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

#[test]
fn directives_do_not_leak_into_token_stream() {
    // A free-format file may legitimately carry `>>SOURCE FORMAT IS
    // FREE` at the top and `>>D` debug-line prefixes elsewhere. The
    // preprocessor drops these so the scanner neither sees `>>` as
    // invalid characters nor exposes the directive words as tokens.
    let src = ">>SOURCE FORMAT IS FREE\n01 FOO.\n>>D DISPLAY 'X'.\n";
    let (tokens, errors) = lex(src, SourceFormat::Free);
    assert!(errors.is_empty(), "{errors:?}");
    let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![TokenKind::LevelNumber(1), TokenKind::Identifier, TokenKind::Period, TokenKind::Eof]
    );
}

/// Asserts that the same logical source lexes to the same TokenKind
/// sequence under both formats. Only kinds are compared — spans differ
/// because the fixed variant carries a 7-column prefix, and that is
/// intentional, not a regression.
fn assert_token_kinds_match(fixed: &str, free: &str) {
    let (fixed_tokens, fixed_errors) = lex(fixed, SourceFormat::Fixed);
    let (free_tokens, free_errors) = lex(free, SourceFormat::Free);
    assert!(fixed_errors.is_empty(), "fixed errors: {fixed_errors:?}");
    assert!(free_errors.is_empty(), "free errors: {free_errors:?}");
    let fixed_kinds: Vec<TokenKind> = fixed_tokens.iter().map(|t| t.kind).collect();
    let free_kinds: Vec<TokenKind> = free_tokens.iter().map(|t| t.kind).collect();
    assert_eq!(
        fixed_kinds, free_kinds,
        "kind sequences diverge\nfixed: {fixed_kinds:?}\nfree: {free_kinds:?}"
    );
}

#[test]
fn parity_simple_record() {
    assert_token_kinds_match("       01 FOO.\n", "01 FOO.\n");
}

#[test]
fn parity_data_item_with_picture() {
    assert_token_kinds_match(
        "       05 AMOUNT PIC S9(7)V99 COMP-3.\n",
        "05 AMOUNT PIC S9(7)V99 COMP-3.\n",
    );
}

#[test]
fn parity_string_literal() {
    assert_token_kinds_match("       05 GREETING VALUE 'HELLO'.\n", "05 GREETING VALUE 'HELLO'.\n");
}

#[test]
fn parity_comment_drops_whole_line_in_both_formats() {
    // Fixed indicator `*` drops the line; free `*>` at col 1 drops the
    // line. Both sources therefore produce the same (small) token
    // stream from the surviving lines.
    assert_token_kinds_match(
        "      * header comment\n       05 X.\n",
        "*> header comment\n05 X.\n",
    );
}

#[test]
fn parity_nested_group() {
    let fixed = "       01 PARENT.\n          05 CHILD PIC X(4).\n          05 KID   PIC 9(2).\n";
    let free = "01 PARENT.\n   05 CHILD PIC X(4).\n   05 KID   PIC 9(2).\n";
    assert_token_kinds_match(fixed, free);
}
