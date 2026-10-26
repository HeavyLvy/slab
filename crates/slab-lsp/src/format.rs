//! Document formatting: the whole document replaced by its `slab fmt` form.

use crate::lines::{Enc, LineIndex, Range};

/// The range to replace and the new text, or `None` when the document is already formatted or
/// does not parse (the diagnostics already say why).
pub fn edit(text: &str, lines: &LineIndex, enc: Enc) -> Option<(Range, String)> {
    let formatted = slab::fmt::format(text).ok().filter(|f| f != text)?;
    let range = Range { start: lines.position(text, 0, enc), end: lines.position(text, text.len(), enc) };
    Some((range, formatted))
}
