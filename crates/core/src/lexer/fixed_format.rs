//! Pre-scanning pass that converts raw fixed-format COBOL source into
//! logical lines containing only the cols 8-72 text area.
//!
//! Fixed-format layout:
//!
//! ```text
//! | cols 1-6  | col 7     | cols 8-72   | cols 73-80 |
//! | sequence  | indicator | source text | ignored    |
//! ```
//!
//! The indicator selects how the line behaves:
//!
//! | Indicator | Meaning                                           |
//! |-----------|---------------------------------------------------|
//! | ` ` / `D` | Normal code (D = debug line, still treated as code) |
//! | `*` / `/` | Comment / form-feed — entire line dropped          |
//! | `-`       | Continuation — text area appended to the prior logical line |
//! | anything  | Invalid — flagged as error, line dropped           |
//!
//! Lines shorter than 8 bytes cannot carry code and are skipped
//! silently. Bytes past column 72 are truncated without an error.

use std::ops::Range;

use crate::error::LexerError;
use crate::span::Span;

/// A single logical line of text ready for tokenisation.
///
/// `text` is the cols-8-72 slice of one physical line. Future PRs will
/// extend this into a joined buffer built from multiple physical lines
/// via the `-` continuation indicator; segments record where each piece
/// of the joined text lives in the original source so a token that
/// straddles physical lines can still be mapped back to an accurate
/// span.
#[derive(Debug, Clone)]
pub struct LogicalLine {
    pub text: String,
    pub segments: Vec<Segment>,
    pub start_line: u32,
}

/// Maps a contiguous region of `LogicalLine.text` back to its original
/// position in the source file.
///
/// A segment always represents a contiguous slice of one physical line.
/// A logical line with no continuation has exactly one segment covering
/// the full text. With continuation, each extra physical line adds
/// another segment.
#[derive(Debug, Clone)]
pub struct Segment {
    /// Byte offset within `LogicalLine.text` where this segment starts.
    pub logical_start: usize,
    /// Byte offset within the original source where this segment's
    /// first character lives.
    pub source_start: usize,
    /// Byte length of the segment (same in both logical and source
    /// coordinates — segments never remap individual characters).
    pub len: usize,
    /// 1-indexed physical line number of this segment's first character.
    pub source_line: u32,
    /// 1-indexed physical column of this segment's first character.
    pub source_col: u32,
}

impl LogicalLine {
    /// Map a byte range within `self.text` back to a `Span` in the
    /// original source.
    ///
    /// The span's `start` / `end` are the original byte offsets of the
    /// first and one-past-the-last characters the range covers. `line`
    /// and `column` refer to the first character. An empty range yields
    /// an empty span pointing at the position of `range.start`.
    pub fn map_span(&self, range: Range<usize>) -> Span {
        let start_seg = self.segment_for(range.start);
        let start_delta = range.start - start_seg.logical_start;
        let source_start = start_seg.source_start + start_delta;
        let line = start_seg.source_line;
        let column = start_seg.source_col + start_delta as u32;

        let source_end = if range.end == range.start {
            source_start
        } else {
            let last_logical = range.end - 1;
            let end_seg = self.segment_for(last_logical);
            end_seg.source_start + (last_logical - end_seg.logical_start) + 1
        };

        Span::new(source_start, source_end, line, column)
    }

    fn segment_for(&self, logical_offset: usize) -> &Segment {
        // Segments are sorted by logical_start and non-overlapping, so a
        // linear scan is fine — logical lines rarely hold more than two
        // or three segments in practice.
        self.segments
            .iter()
            .rfind(|s| logical_offset >= s.logical_start)
            .expect("LogicalLine has at least one segment")
    }
}

/// Returns the opening quote character if `s` ends while still inside
/// an unclosed string literal.
///
/// Follows COBOL's doubled-quote escape rule: `''` inside a `'`-literal
/// (or `""` inside a `"`-literal) stands for a single embedded quote
/// and does *not* close the literal. This mirrors the scanner's logos
/// regex so that the preprocessor's notion of "line ends in an open
/// literal" agrees with what the scanner would see.
pub(super) fn ends_with_open_literal(s: &str) -> Option<char> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut inside: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match inside {
            None => {
                if b == b'\'' || b == b'"' {
                    inside = Some(b);
                }
                i += 1;
            }
            Some(q) => {
                if b == q {
                    // Doubled quote → escape, keep scanning inside.
                    // A single quote with no partner closes the literal.
                    if i + 1 < bytes.len() && bytes[i + 1] == q {
                        i += 2;
                    } else {
                        inside = None;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
        }
    }
    inside.map(|q| q as char)
}

pub fn preprocess(source: &str) -> (Vec<LogicalLine>, Vec<LexerError>) {
    let mut lines: Vec<LogicalLine> = Vec::new();
    let mut errors = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0usize;
    let mut line_no: u32 = 1;
    // True iff the immediately previous physical line is eligible to
    // be continued: a normal code line or a successful `-` line.
    // Comments, invalid indicators, null bytes, short lines, orphan
    // `-`, and failed literal continuations all reset this to false so
    // a later `-` cannot silently graft content onto an older
    // statement across a dropped line.
    let mut prev_continuable = false;

    while pos <= bytes.len() {
        let newline = bytes[pos..].iter().position(|&b| b == b'\n');
        let (line_end, next_pos) = match newline {
            Some(off) => (pos + off, pos + off + 1),
            None => {
                if pos == bytes.len() {
                    break;
                }
                (bytes.len(), bytes.len())
            }
        };

        let mut content_end = line_end;
        if content_end > pos && bytes[content_end - 1] == b'\r' {
            content_end -= 1;
        }
        let line_len = content_end - pos;

        if line_len >= 8 {
            let indicator = bytes[pos + 6];
            let text_start = pos + 7;
            // Clamp to col 72 *and* walk back to a UTF-8 boundary so a
            // multi-byte char straddling the cutoff does not split.
            let mut text_end = pos + line_len.min(72);
            while text_end > text_start && !source.is_char_boundary(text_end) {
                text_end -= 1;
            }
            let col7_span = Span::new(pos + 6, pos + 7, line_no, 7);
            match indicator {
                b' ' | b'D' | b'd' => {
                    let text = &source[text_start..text_end];
                    let segment = Segment {
                        logical_start: 0,
                        source_start: text_start,
                        len: text.len(),
                        source_line: line_no,
                        source_col: 8,
                    };
                    lines.push(LogicalLine {
                        text: text.to_string(),
                        segments: vec![segment],
                        start_line: line_no,
                    });
                    prev_continuable = true;
                }
                b'*' | b'/' => {
                    prev_continuable = false;
                }
                0 => {
                    errors.push(LexerError::EncounteredNullByte { span: col7_span });
                    prev_continuable = false;
                }
                b'-' if !prev_continuable => {
                    errors.push(LexerError::OrphanContinuation { span: col7_span });
                    prev_continuable = false;
                }
                b'-' => {
                    let prior =
                        lines.last_mut().expect("prev_continuable implies a prior logical line");
                    let area = &source[text_start..text_end];
                    prev_continuable = if let Some(open_quote) = ends_with_open_literal(&prior.text)
                    {
                        match area.bytes().position(|b| b == open_quote as u8) {
                            Some(offset) => {
                                let keep_start = text_start + offset + 1;
                                if keep_start < text_end {
                                    append_segment(
                                        prior, source, keep_start, text_end, line_no, pos,
                                    );
                                }
                                true
                            }
                            None => {
                                errors.push(LexerError::ContinuationWithoutReopeningQuote {
                                    expected: open_quote,
                                    span: col7_span,
                                });
                                false
                            }
                        }
                    } else {
                        let first_non_ws = area
                            .bytes()
                            .position(|b| b != b' ' && b != b'\t')
                            .unwrap_or(area.len());
                        let keep_start = text_start + first_non_ws;
                        if keep_start < text_end {
                            append_segment(prior, source, keep_start, text_end, line_no, pos);
                        }
                        true
                    };
                }
                other => {
                    errors.push(LexerError::InvalidCharacter {
                        ch: char::from(other),
                        span: col7_span,
                    });
                    prev_continuable = false;
                }
            }
        } else {
            prev_continuable = false;
        }

        pos = next_pos;
        line_no = line_no.saturating_add(1);

        if next_pos == bytes.len() && newline.is_none() {
            break;
        }
    }

    (lines, errors)
}

/// Appends `source[keep_start..keep_end]` to `line`'s joined text and
/// records a matching segment. `pos_of_phys_line` is the byte offset
/// where the physical line being continued begins; columns are derived
/// as `(keep_start - pos_of_phys_line) + 1`.
fn append_segment(
    line: &mut LogicalLine,
    source: &str,
    keep_start: usize,
    keep_end: usize,
    source_line: u32,
    pos_of_phys_line: usize,
) {
    let logical_start = line.text.len();
    let piece = &source[keep_start..keep_end];
    line.text.push_str(piece);
    line.segments.push(Segment {
        logical_start,
        source_start: keep_start,
        len: piece.len(),
        source_line,
        source_col: (keep_start - pos_of_phys_line) as u32 + 1,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(indicator: char, text: &str) -> String {
        let mut s = String::from("      "); // cols 1-6
        s.push(indicator);
        s.push_str(text);
        s.push('\n');
        s
    }

    #[test]
    fn extracts_text_area_and_tracks_offset() {
        let src = sample(' ', "01 FOO.");
        let (lines, errors) = preprocess(&src);
        assert!(errors.is_empty());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "01 FOO.");
        assert_eq!(lines[0].start_line, 1);
        assert_eq!(lines[0].segments.len(), 1);
        let seg = &lines[0].segments[0];
        assert_eq!(seg.logical_start, 0);
        assert_eq!(seg.source_start, 7);
        assert_eq!(seg.len, "01 FOO.".len());
        assert_eq!(seg.source_line, 1);
        assert_eq!(seg.source_col, 8);
    }

    #[test]
    fn skips_comment_and_formfeed_lines() {
        let src = format!(
            "{}{}{}",
            sample('*', " commentary"),
            sample('/', " formfeed"),
            sample(' ', "02 BAR.")
        );
        let (lines, errors) = preprocess(&src);
        assert!(errors.is_empty());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "02 BAR.");
        assert_eq!(lines[0].start_line, 3);
    }

    #[test]
    fn debug_indicator_emits_code_line() {
        let src = sample('D', "DISPLAY X.");
        let (lines, _errors) = preprocess(&src);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "DISPLAY X.");
    }

    #[test]
    fn orphan_continuation_on_first_line_is_reported() {
        let src = sample('-', "    TAIL.");
        let (lines, errors) = preprocess(&src);
        assert!(lines.is_empty());
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            LexerError::OrphanContinuation { span } => {
                assert_eq!(span.column, 7);
                assert_eq!(span.line, 1);
            }
            other => panic!("expected OrphanContinuation, got {other:?}"),
        }
    }

    #[test]
    fn literal_continuation_skips_reopening_quote() {
        let src = format!("{}{}", sample(' ', "VALUE 'HELLO"), sample('-', "    'WORLD'."));
        let (lines, errors) = preprocess(&src);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "VALUE 'HELLOWORLD'.");
        assert_eq!(lines[0].segments.len(), 2);
    }

    #[test]
    fn literal_continuation_without_reopening_quote_is_reported() {
        let src = format!("{}{}", sample(' ', "VALUE 'HELLO"), sample('-', "   missing."));
        let (_, errors) = preprocess(&src);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            LexerError::ContinuationWithoutReopeningQuote { expected, span } => {
                assert_eq!(*expected, '\'');
                assert_eq!(span.line, 2);
                assert_eq!(span.column, 7);
            }
            other => panic!("expected ContinuationWithoutReopeningQuote, got {other:?}"),
        }
    }

    #[test]
    fn continuation_after_comment_is_orphan() {
        // A comment between a continuable code line and a `-` line must
        // break the chain; otherwise the continuation would silently
        // graft onto the older statement, skipping the comment.
        let src = format!(
            "{}{}{}",
            sample(' ', "05 A VALUE 'HELLO"),
            sample('*', " COMMENT"),
            sample('-', "    'WORLD'.")
        );
        let (lines, errors) = preprocess(&src);
        assert_eq!(lines.len(), 1, "only the first code line should be logical");
        // Logical line remains open (literal not reopened by a valid
        // continuation), so the scanner's UnterminatedStringLiteral
        // still fires downstream — the preprocessor only asserts the
        // chain was broken here.
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, LexerError::OrphanContinuation { span } if span.line == 3)),
            "expected OrphanContinuation at line 3, got {errors:?}"
        );
    }

    #[test]
    fn continuation_after_invalid_indicator_is_orphan() {
        let src =
            format!("{}{}{}", sample(' ', "05 A."), sample('X', " BAR."), sample('-', "    BAZ."));
        let (_, errors) = preprocess(&src);
        assert!(
            errors.iter().any(|e| matches!(e, LexerError::InvalidCharacter { ch: 'X', .. })),
            "expected InvalidCharacter 'X', got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, LexerError::OrphanContinuation { span } if span.line == 3)),
            "expected OrphanContinuation at line 3, got {errors:?}"
        );
    }

    #[test]
    fn non_literal_continuation_appends_stripped_text_as_new_segment() {
        let src = format!("{}{}", sample(' ', "01 A "), sample('-', "   FOO."));
        let (lines, errors) = preprocess(&src);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(lines.len(), 1);

        let line = &lines[0];
        assert_eq!(line.text, "01 A FOO.");
        assert_eq!(line.segments.len(), 2);

        let seg1 = &line.segments[1];
        assert_eq!(seg1.logical_start, "01 A ".len());
        assert_eq!(seg1.source_line, 2);
        // Continuation text `FOO.` starts after 3 stripped spaces, so
        // at col 8+3 = 11.
        assert_eq!(seg1.source_col, 11);
        assert_eq!(seg1.len, "FOO.".len());
    }

    #[test]
    fn short_lines_are_silently_skipped() {
        let src = "abc\n\n      \n";
        let (lines, errors) = preprocess(src);
        assert!(lines.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn truncates_past_column_72() {
        let mut src = String::from("      "); // cols 1-6
        src.push(' '); // col 7
        for _ in 8..=80 {
            src.push('X');
        }
        src.push('\n');
        let (lines, _) = preprocess(&src);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text.len(), 65); // cols 8..=72 is 65 chars
        assert!(lines[0].text.chars().all(|c| c == 'X'));
    }

    #[test]
    fn crlf_line_endings_supported() {
        let src = "      \x20LINE1.\r\n      \x20LINE2.\r\n";
        let (lines, _) = preprocess(src);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "LINE1.");
        assert_eq!(lines[1].text, "LINE2.");
    }

    #[test]
    fn multibyte_char_straddling_col_72_does_not_panic() {
        let mut src = String::from("       "); // 7-col prefix
        for _ in 0..64 {
            src.push('X');
        }
        src.push('é'); // 2 bytes, crosses the byte-72 cutoff
        src.push('\n');
        let (lines, errors) = preprocess(&src);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.chars().count() <= 65);
    }

    #[test]
    fn null_byte_indicator_raises_null_error() {
        let src = "      \x00FOO.\n";
        let (_, errors) = preprocess(src);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], LexerError::EncounteredNullByte { .. }));
    }

    fn line_with_two_segments() -> LogicalLine {
        // text "ABCXYZ" where "ABC" is segment 0 (source 10, line 1 col 8)
        // and "XYZ" is segment 1 (source 40, line 2 col 12).
        LogicalLine {
            text: "ABCXYZ".to_string(),
            segments: vec![
                Segment {
                    logical_start: 0,
                    source_start: 10,
                    len: 3,
                    source_line: 1,
                    source_col: 8,
                },
                Segment {
                    logical_start: 3,
                    source_start: 40,
                    len: 3,
                    source_line: 2,
                    source_col: 12,
                },
            ],
            start_line: 1,
        }
    }

    #[test]
    fn map_span_within_single_segment() {
        let line = line_with_two_segments();
        let span = line.map_span(0..3);
        assert_eq!(span, Span::new(10, 13, 1, 8));

        let mid = line.map_span(1..3);
        assert_eq!(mid, Span::new(11, 13, 1, 9));

        let seg1 = line.map_span(3..6);
        assert_eq!(seg1, Span::new(40, 43, 2, 12));
    }

    #[test]
    fn map_span_across_segments_picks_outer_endpoints() {
        let line = line_with_two_segments();
        let span = line.map_span(1..5);
        // start is inside seg0 at offset 1 → source 11, col 9
        // last char is at logical 4 → seg1 offset 1 → source 41; end = 42
        assert_eq!(span, Span::new(11, 42, 1, 9));
    }

    #[test]
    fn map_span_empty_range_yields_empty_span() {
        let line = line_with_two_segments();
        let span = line.map_span(2..2);
        assert_eq!(span, Span::new(12, 12, 1, 10));
    }

    #[test]
    fn ends_with_open_literal_tracks_single_quotes() {
        assert_eq!(ends_with_open_literal(""), None);
        assert_eq!(ends_with_open_literal("no quotes here"), None);
        assert_eq!(ends_with_open_literal("'HELLO"), Some('\''));
        assert_eq!(ends_with_open_literal("'HELLO'"), None);
    }

    #[test]
    fn ends_with_open_literal_tracks_double_quotes() {
        assert_eq!(ends_with_open_literal("\"HELLO"), Some('"'));
        assert_eq!(ends_with_open_literal("\"HELLO\""), None);
    }

    #[test]
    fn doubled_quotes_inside_literal_are_escapes_not_closers() {
        // 'A''B' -> closed after escape. 'A''B -> still open.
        assert_eq!(ends_with_open_literal("'A''B'"), None);
        assert_eq!(ends_with_open_literal("'A''B"), Some('\''));

        // Trailing doubled quote at end of buffer: the scanner treats it
        // as "escape then unterminated", so we must say open too.
        assert_eq!(ends_with_open_literal("'A''"), Some('\''));
        assert_eq!(ends_with_open_literal("\"X\"\""), Some('"'));

        // Three quotes -> open, escape, then unterminated.
        assert_eq!(ends_with_open_literal("'''"), Some('\''));
    }

    #[test]
    fn opposite_quote_inside_literal_is_plain_content() {
        assert_eq!(ends_with_open_literal("'A\"B'"), None);
        assert_eq!(ends_with_open_literal("\"A'B\""), None);
        assert_eq!(ends_with_open_literal("'A\"B"), Some('\''));
    }
}
