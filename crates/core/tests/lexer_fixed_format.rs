//! Snapshot coverage for fixed-format lexing scenarios.
//!
//! Each scenario asserts a full token stream (including Eof) and any
//! accumulated errors so human review catches regressions in span
//! tracking, keyword detection, or error recovery in a single glance.

use copyforge_core::error::LexerError;
use copyforge_core::lexer::{lex, token::Token, token::TokenKind};

fn render(source: &str) -> String {
    use std::fmt::Write;
    let (tokens, errors) = lex(source);
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
fn minimal_record() {
    let src = include_str!("../../../fixtures/simple/customer.cpy");
    insta::assert_snapshot!(render(src));
}

#[test]
fn nested_group() {
    let src = include_str!("../../../fixtures/simple/nested-group.cpy");
    insta::assert_snapshot!(render(src));
}

#[test]
fn pic_variants() {
    let src = include_str!("../../../fixtures/simple/pic-variants.cpy");
    insta::assert_snapshot!(render(src));
}

#[test]
fn comment_lines() {
    let src = include_str!("../../../fixtures/simple/with-comments.cpy");
    insta::assert_snapshot!(render(src));
}

#[test]
fn sequence_area_ignored() {
    // cols 1-6 carry digits and letters; they must not become tokens.
    let src = "123456 01 FOO.\nABCDEF 05 BAR.\n";
    insta::assert_snapshot!(render(src));
}

#[test]
fn column_72_truncation() {
    let mut src = String::from("       "); // 7 cols: sequence + indicator (blank)
    for _ in 8..=80 {
        src.push('X');
    }
    src.push('\n');

    let (tokens, errors) = lex(&src);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let ident: Vec<&Token<'_>> =
        tokens.iter().filter(|t| t.kind == TokenKind::Identifier).collect();
    assert_eq!(ident.len(), 1, "expected a single identifier token");
    assert_eq!(
        ident[0].text.len(),
        65,
        "cols 8..=72 is 65 characters; truncation must drop the rest"
    );
    assert!(ident[0].text.chars().all(|c| c == 'X'));
    let max_end_col = 72 + 1;
    for tok in &tokens {
        assert!(
            tok.span.column + tok.text.len() as u32 <= max_end_col || tok.kind == TokenKind::Eof,
            "token {tok:?} extends past column 72"
        );
    }
}

#[test]
fn numeric_and_string_literals() {
    let src = "       05 A VALUE \"foo\".\n       05 B VALUE 42.\n       05 C VALUE 'X'.\n";
    insta::assert_snapshot!(render(src));
}

#[test]
fn eof_tracks_physical_line_count() {
    // Source has three physical lines, all comments, zero logical lines.
    // EOF must reflect the physical end of the file so parser
    // diagnostics reported at EOF land on the right line.
    let src = "      * only comment\n      * another comment\n      * and one more\n";
    let (tokens, _) = lex(src);
    let eof = tokens.last().expect("eof token");
    assert_eq!(eof.kind, TokenKind::Eof);
    assert_eq!(eof.span.line, 4, "three physical lines plus trailing slot");
}

#[test]
fn error_recovery() {
    // '~' is not a valid COBOL character; the lexer must record the
    // error and keep emitting tokens from surrounding lines.
    let src = "       01 FOO.\n       05 ~ BAR.\n       05 BAZ.\n";
    let (tokens, errors) = lex(src);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(errors[0], LexerError::InvalidCharacter { ch: '~', .. }));
    let idents: Vec<&str> =
        tokens.iter().filter(|t| t.kind == TokenKind::Identifier).map(|t| t.text).collect();
    assert_eq!(idents, vec!["FOO", "BAR", "BAZ"]);
    insta::assert_snapshot!(render(src));
}
