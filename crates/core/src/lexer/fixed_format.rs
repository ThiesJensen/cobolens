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
//! | `-`       | Continuation — not yet supported, flagged as error |
//! | anything  | Invalid — flagged as error, line dropped           |
//!
//! Lines shorter than 8 bytes cannot carry code and are skipped
//! silently. Bytes past column 72 are truncated without an error.

use crate::error::LexerError;
use crate::span::Span;

/// A single logical line of text ready for tokenisation.
///
/// `text` is an owned copy of the cols-8-72 slice so the scanner can
/// iterate it freely. `base_offset` is the byte position within the
/// original source where `text[0]` lives; tokens add to this to
/// reconstruct source-relative spans.
#[derive(Debug, Clone)]
pub struct LogicalLine {
    pub text: String,
    pub base_offset: usize,
    pub line_no: u32,
}

pub fn preprocess(source: &str) -> (Vec<LogicalLine>, Vec<LexerError>) {
    let mut lines = Vec::new();
    let mut errors = Vec::new();
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
                    lines.push(LogicalLine {
                        text: source[text_start..text_end].to_string(),
                        base_offset: text_start,
                        line_no,
                    });
                }
                b'*' | b'/' => {}
                0 => errors.push(LexerError::EncounteredNullByte { span: col7_span }),
                other => errors
                    .push(LexerError::InvalidCharacter { ch: char::from(other), span: col7_span }),
            }
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
        assert_eq!(lines[0].base_offset, 7);
        assert_eq!(lines[0].line_no, 1);
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
        assert_eq!(lines[0].line_no, 3);
    }

    #[test]
    fn debug_indicator_emits_code_line() {
        let src = sample('D', "DISPLAY X.");
        let (lines, _errors) = preprocess(&src);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "DISPLAY X.");
    }

    #[test]
    fn continuation_indicator_produces_error_and_drops_content() {
        let src = sample('-', "\"tail\"");
        let (lines, errors) = preprocess(&src);
        assert!(lines.is_empty());
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            LexerError::InvalidCharacter { ch, span } => {
                assert_eq!(*ch, '-');
                assert_eq!(span.column, 7);
            }
            other => panic!("expected InvalidCharacter, got {other:?}"),
        }
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
}
