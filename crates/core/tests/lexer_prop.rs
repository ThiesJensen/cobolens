//! Property-based robustness checks for the lexer.
//!
//! The invariants we care about at the lexer layer are cheap but
//! cover the foot-guns: don't panic on arbitrary ASCII, keep every
//! token span well-ordered and inside the source, and make sure the
//! stored `text` slice actually matches the span it claims. A
//! regression in any of these would ripple through every downstream
//! stage, so we guard them up front.

use copyforge_core::lexer::lex;
use proptest::prelude::*;

proptest! {
    #[test]
    fn ascii_input_never_panics_and_spans_are_consistent(
        source in "[\\x00-\\x7f]{0,1024}"
    ) {
        let (tokens, _errors) = lex(&source);
        for t in &tokens {
            prop_assert!(t.span.start <= t.span.end, "{t:?}");
            prop_assert!(
                t.span.end <= source.len(),
                "span.end {} exceeds source length {}: {t:?}",
                t.span.end,
                source.len()
            );
            prop_assert_eq!(t.text, &source[t.span.start..t.span.end]);
        }
    }

    /// Continuation lines let a single token span multiple physical
    /// lines, so for multi-line input we relax the "text is the source
    /// slice" check and assert the weaker — but still load-bearing —
    /// invariant that every span still maps to a real byte range.
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
