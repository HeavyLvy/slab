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
    pub fn position(&self, text: &str, offset: usize) -> Position {
        let offset = offset.min(text.len());
        let line = self.line_starts.partition_point(|&s| s <= offset) - 1;
        let character = text[self.line_starts[line]..offset].chars().count() as u32;
        Position { line: line as u32, character }
    }

    /// Position -> byte offset. A character count past the end of the line lands on the next
    /// line's start (LSP allows a position right after the last character).
    pub fn offset(&self, text: &str, pos: Position) -> Option<usize> {
        let line = pos.line as usize;
        let start = *self.line_starts.get(line)?;
        let end = self.line_starts.get(line + 1).copied().unwrap_or(text.len());
        let mut offset = start;
        let mut left = pos.character as usize;
        for c in text[start..end.min(text.len())].chars() {
            if left == 0 {
                break;
            }
            left -= 1;
            offset += c.len_utf8();
        }
        Some(offset.min(text.len()))
    }

    /// A compiler `Span` (byte range) as an LSP range.
    pub fn range(&self, text: &str, span: Span) -> Range {
        Range { start: self.position(text, span.start as usize), end: self.position(text, span.end as usize) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let text = "ab\ncdé\nf";
        let idx = LineIndex::new(text);
        assert_eq!(idx.position(text, 3), Position { line: 1, character: 0 });
        assert_eq!(idx.position(text, 7), Position { line: 1, character: 3 }); // after the two-byte é
        assert_eq!(idx.position(text, text.len()), Position { line: 2, character: 1 });
        assert_eq!(idx.offset(text, Position { line: 1, character: 2 }), Some(5));
        assert_eq!(idx.offset(text, Position { line: 1, character: 99 }), Some(8)); // clamped to next line start
        assert_eq!(idx.offset(text, Position { line: 5, character: 0 }), None);
        let r = idx.range(text, Span::new(0, 8));
        assert_eq!(r, Range { start: Position { line: 0, character: 0 }, end: Position { line: 2, character: 0 } });
    }
}