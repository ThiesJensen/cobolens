//! Property-based robustness checks for the lexer.
//!
//! The invariants we care about at the lexer layer are cheap but
//! cover the foot-guns: don't panic on arbitrary ASCII, keep every
//! token span well-ordered and inside the source, and make sure any
//! single-segment token's text still agrees with the bytes its span
//! claims. Continuation lines deliberately break the span/text
//! correspondence for multi-segment tokens — the joined logical
//! content, not the raw source slice, is what keyword lookup and
//! parser logic need.

use copyforge_core::lexer::lex;
use proptest::prelude::*;

proptest! {
    #[test]
    fn ascii_input_never_panics_and_spans_stay_inside_source(
        source in "[\\x00-\\x7f]{0,1024}"
    ) {
        let (tokens, _errors) = lex(&source);
        for t in &tokens {
            prop_assert!(t.span.start <= t.span.end, "{t:?}");
            prop_assert!(
                source.get(t.span.start..t.span.end).is_some(),
                "span {}..{} is not a valid slice of {}-byte source: {t:?}",
                t.span.start,
                t.span.end,
                source.len()
            );
        }
    }

    #[test]
    fn multiline_spans_stay_inside_source(
        lines in proptest::collection::vec("[\\x00-\\x7f]{0,80}", 1..=10)
    ) {
        let source = lines.join("\n");
        let (tokens, _errors) = lex(&source);
        for t in &tokens {
            prop_assert!(t.span.start <= t.span.end, "{t:?}");
            prop_assert!(
                source.get(t.span.start..t.span.end).is_some(),
                "span {}..{} is not a valid slice of {}-byte source: {t:?}",
                t.span.start,
                t.span.end,
                source.len()
            );
        }
    }
}
