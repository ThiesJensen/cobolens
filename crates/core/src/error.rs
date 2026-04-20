//! Lexer-level error types.
//!
//! Scanning never short-circuits: the lexer accumulates every error it
//! sees and keeps producing tokens around them. Downstream stages
//! (parser, resolver) therefore still get useful input for recovery,
//! and end users get all the complaints at once instead of having to
//! fix one, re-run, and get the next.

use crate::span::Span;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LexerError {
    #[error("unterminated string literal")]
    UnterminatedStringLiteral { span: Span },

    #[error("invalid character {ch:?} in source")]
    InvalidCharacter { ch: char, span: Span },

    #[error("null byte encountered in source")]
    EncounteredNullByte { span: Span },

    #[error("continuation line does not reopen {expected:?}-quoted literal")]
    ContinuationWithoutReopeningQuote { expected: char, span: Span },

    #[error("continuation indicator with no prior line to continue")]
    OrphanContinuation { span: Span },
}

impl LexerError {
    pub fn span(&self) -> Span {
        match self {
            Self::UnterminatedStringLiteral { span }
            | Self::InvalidCharacter { span, .. }
            | Self::EncounteredNullByte { span }
            | Self::ContinuationWithoutReopeningQuote { span, .. }
            | Self::OrphanContinuation { span } => *span,
        }
    }
}
