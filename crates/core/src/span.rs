//! Source location tracking for tokens and diagnostics.

/// Byte offsets into the original source plus a 1-indexed line and column
/// for user-facing diagnostics.
///
/// `start` is inclusive, `end` exclusive, both in bytes. `line` and
/// `column` point at the first character of the span. The column is
/// 1-indexed so that "column 7" in a COBOL fixed-format indicator field
/// matches everyday usage and error-message conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub column: u32,
}

impl Span {
    pub const fn new(start: usize, end: usize, line: u32, column: u32) -> Self {
        Self { start, end, line, column }
    }

    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}
