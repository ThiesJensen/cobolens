//! Pre-scanning pass for COBOL 2002 free-format source.
//!
//! Free format drops the card-image rules: every byte of a physical
//! line is code (no sequence area, no indicator column, no column-72
//! cutoff) and inline comments start with `*>` and run to the end of
//! the line. Free format in this PR does not support continuation.

use crate::error::LexerError;
use crate::lexer::fixed_format::{LogicalLine, Segment};

/// Returns the byte offset of the `*>` sequence that opens an inline
/// comment on `line`, or `None` if no such marker exists outside a
/// string literal.
///
/// The quote-state machine follows the same doubled-quote escape rule
/// as [`fixed_format::ends_with_open_literal`]: `''` inside a
/// `'`-literal (or `""` inside a `"`-literal) is an embedded quote, not
/// a closer. An unterminated literal keeps us inside to the end of the
/// line, so a `*>` that appears inside such a literal is treated as
/// literal content, not a comment.
pub(super) fn find_comment_start(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut inside: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match inside {
            None => {
                if b == b'*' && bytes.get(i + 1) == Some(&b'>') {
                    return Some(i);
                }
                if b == b'\'' || b == b'"' {
                    inside = Some(b);
                }
                i += 1;
            }
            Some(q) => {
                if b == q {
                    // Doubled quote is an embedded quote; a lone quote
                    // closes the literal.
                    if bytes.get(i + 1) == Some(&q) {
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
    None
}

pub fn preprocess(source: &str) -> (Vec<LogicalLine>, Vec<LexerError>) {
    // Free format currently has no preprocessor errors: there is no
    // indicator column to reject and continuation is out of scope.
    // The empty vec preserves the signature parity with
    // `fixed_format::preprocess` so both arms of `lex` look identical.
    let mut lines: Vec<LogicalLine> = Vec::new();
    let errors: Vec<LexerError> = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0usize;
    let mut line_no: u32 = 1;

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

        let physical = &source[pos..content_end];
        let text_end = match find_comment_start(physical) {
            Some(off) => pos + off,
            None => content_end,
        };
        let text = &source[pos..text_end];

        if !text.trim().is_empty() {
            lines.push(LogicalLine {
                text: text.to_string(),
                segments: vec![Segment {
                    logical_start: 0,
                    source_start: pos,
                    len: text.len(),
                    source_line: line_no,
                    source_col: 1,
                }],
                start_line: line_no,
            });
        }

        pos = next_pos;
        line_no = line_no.saturating_add(1);

        if next_pos == bytes.len() && newline.is_none() {
            break;
        }
    }

    (lines, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_start_outside_literal() {
        assert_eq!(find_comment_start("FOO *> bar"), Some(4));
    }

    #[test]
    fn comment_start_none_when_absent() {
        assert_eq!(find_comment_start(""), None);
        assert_eq!(find_comment_start("05 A PIC X(10)."), None);
        assert_eq!(find_comment_start("lone * star"), None);
        assert_eq!(find_comment_start("lone > gt"), None);
    }

    #[test]
    fn comment_marker_inside_single_quote_is_literal_content() {
        assert_eq!(find_comment_start("VALUE '*> not a comment'"), None);
    }

    #[test]
    fn comment_marker_inside_double_quote_is_literal_content() {
        assert_eq!(find_comment_start("VALUE \"*> not a comment\""), None);
    }

    #[test]
    fn comment_after_closed_literal_is_detected() {
        // Closed `'*>'` literal, then a real `*>` after it.
        let src = "VALUE '*>' FOO *> real comment";
        let offset = find_comment_start(src).expect("comment detected");
        assert_eq!(&src[offset..], "*> real comment");
    }

    #[test]
    fn comment_inside_unterminated_literal_is_not_detected() {
        // Open quote never closes; the `*>` lives inside the literal
        // and must not be treated as a comment marker.
        assert_eq!(find_comment_start("VALUE 'open *> still literal"), None);
    }

    #[test]
    fn doubled_quote_escape_does_not_close_literal() {
        // `'A''B'` is a single literal with an embedded apostrophe; the
        // `*>` that follows the closing quote is the real comment.
        let src = "VALUE 'A''B' *> trailing";
        let offset = find_comment_start(src).expect("comment detected");
        assert_eq!(&src[offset..], "*> trailing");
    }

    #[test]
    fn doubled_quote_escape_near_star_gt_stays_literal() {
        // The `*>` sits between the embedded-quote escape and the
        // closing quote, so it is literal content.
        assert_eq!(find_comment_start("'A''*>B'"), None);
    }

    #[test]
    fn preprocess_plain_line_yields_single_segment_at_col_one() {
        let (lines, errors) = preprocess("01 FOO.\n");
        assert!(errors.is_empty());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "01 FOO.");
        assert_eq!(lines[0].segments.len(), 1);
        let seg = &lines[0].segments[0];
        assert_eq!(seg.source_col, 1);
        assert_eq!(seg.source_line, 1);
        assert_eq!(seg.source_start, 0);
        assert_eq!(seg.len, "01 FOO.".len());
    }

    #[test]
    fn preprocess_strips_inline_comment() {
        let (lines, errors) = preprocess("05 A PIC X(10). *> trailing note\n");
        assert!(errors.is_empty());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "05 A PIC X(10). ");
    }

    #[test]
    fn preprocess_skips_blank_and_whole_line_comment() {
        let src = "\n   \n*> only comment\n05 FOO.\n";
        let (lines, errors) = preprocess(src);
        assert!(errors.is_empty());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "05 FOO.");
        assert_eq!(lines[0].start_line, 4);
    }

    #[test]
    fn preprocess_no_column_72_truncation() {
        // 100 'X' chars on a single line — fixed format would truncate
        // at col 72, free format must keep all of them.
        let line: String = "X".repeat(100);
        let src = format!("{line}\n");
        let (lines, _) = preprocess(&src);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text.len(), 100);
    }

    #[test]
    fn preprocess_supports_crlf() {
        let src = "LINE1\r\nLINE2\r\n";
        let (lines, _) = preprocess(src);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "LINE1");
        assert_eq!(lines[1].text, "LINE2");
    }

    #[test]
    fn preprocess_keeps_final_line_without_trailing_newline() {
        let (lines, _) = preprocess("05 BAR.");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "05 BAR.");
        assert_eq!(lines[0].start_line, 1);
    }
}
