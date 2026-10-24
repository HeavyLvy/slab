//! Text line indexes: converting between LSP positions (0-based line, character) and byte
//! offsets. Character columns are counted in code points, matching the negotiated
//! UTF-8 position encoding.

use slab::diag::Span;

/// A 0-based line/character position (LSP `Position`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// A half-open position range (LSP `Range`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// How characters are counted in positions, as negotiated with the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Enc {
    #[default]
    /// UTF-16 code units (the protocol's default; what VS Code offers).
    Utf16,
    /// Code points (only offered by clients that say so).
    Utf8,
}

#[derive(Clone, Debug, Default)]
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { line_starts }
    }

    /// Byte offset -> position. Offsets past the end are clamped.
    pub fn position(&self, text: &str, offset: usize, enc: Enc) -> Position {
        let offset = offset.min(text.len());
        let line = self.line_starts.partition_point(|&s| s <= offset) - 1;
        let slice = &text[self.line_starts[line]..offset];
        let character = match enc {
            Enc::Utf8 => slice.chars().count() as u32,
            Enc::Utf16 => slice.encode_utf16().count() as u32,
        };
        Position { line: line as u32, character }
    }

    /// Position -> byte offset. A position right after the line's last character maps to the
    /// byte before the newline; one inside a character (possible in UTF-16) maps to that
    /// character's start; one past the end of the line to the next line's start.
    pub fn offset(&self, text: &str, pos: Position, enc: Enc) -> Option<usize> {
        let line = pos.line as usize;
        let start = *self.line_starts.get(line)?;
        let end = self.line_starts.get(line + 1).copied().unwrap_or(text.len());
        let mut offset = start;
        let mut left = pos.character as usize;
        for c in text[start..end.min(text.len())].chars() {
            if left == 0 {
                break;
            }
            let width = match enc {
                Enc::Utf8 => 1,
                Enc::Utf16 => c.len_utf16(),
            };
            if width > left {
                break;
            }
            left -= width;
            offset += c.len_utf8();
        }
        Some(offset.min(text.len()))
    }

    /// A compiler `Span` (byte range) as an LSP range.
    pub fn range(&self, text: &str, span: Span, enc: Enc) -> Range {
        Range { start: self.position(text, span.start as usize, enc), end: self.position(text, span.end as usize, enc) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A character whose UTF-8 and UTF-16 widths differ (é: 2 bytes, 1 unit; 😀: 4 bytes,
    /// 2 units) must round-trip with each encoding's own counting.
    #[test]
    fn round_trip() {
        let text = "ab\ncdé😀\nf"; // 😀 starts at byte 7 (4 bytes), the newline at 11
        let idx = LineIndex::new(text);
        assert_eq!(idx.position(text, 3, Enc::Utf8), Position { line: 1, character: 0 });
        assert_eq!(idx.position(text, 7, Enc::Utf8), Position { line: 1, character: 3 }); // after é
        assert_eq!(idx.position(text, 7, Enc::Utf16), Position { line: 1, character: 3 });
        assert_eq!(idx.position(text, 11, Enc::Utf16), Position { line: 1, character: 5 }); // 😀 is 2 units
        assert_eq!(idx.position(text, 11, Enc::Utf8), Position { line: 1, character: 4 });
        assert_eq!(idx.position(text, 13, Enc::Utf16), Position { line: 2, character: 1 });
        assert_eq!(idx.offset(text, Position { line: 1, character: 2 }, Enc::Utf16), Some(5)); // at é
        assert_eq!(idx.offset(text, Position { line: 1, character: 3 }, Enc::Utf8), Some(7)); // at 😀
        // the end of the line is the byte after the last character (before the newline)
        assert_eq!(idx.offset(text, Position { line: 1, character: 5 }, Enc::Utf16), Some(11));
        assert_eq!(idx.offset(text, Position { line: 1, character: 4 }, Enc::Utf8), Some(11));
        // a UTF-16 count that is inside 😀 clamps to its start
        assert_eq!(idx.offset(text, Position { line: 1, character: 4 }, Enc::Utf16), Some(7));
        assert_eq!(idx.offset(text, Position { line: 1, character: 8 }, Enc::Utf16), Some(12)); // clamped to the next line
        assert_eq!(idx.offset(text, Position { line: 5, character: 0 }, Enc::Utf16), None);
        let r = idx.range(text, Span::new(0, 13), Enc::Utf16);
        assert_eq!(r, Range { start: Position { line: 0, character: 0 }, end: Position { line: 2, character: 1 } });
    }
}