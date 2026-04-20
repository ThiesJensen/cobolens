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
}
