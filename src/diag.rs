//! Source locations and compiler diagnostics, shared by every compiler phase.

use std::fmt;

/// Half-open byte range into the source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start: start as u32, end: end as u32 }
    }

    /// The smallest span covering both.
    pub fn to(self, other: Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Index of the source file (in a [`SourceMap`]) the span points into.
    pub file: usize,
    pub message: String,
    pub span: Span,
    /// Optional advice shown under the snippet.
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Diagnostic { severity: Severity::Error, file: 0, message: message.into(), span, hint: None }
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Diagnostic { severity: Severity::Warning, ..Diagnostic::new(message, span) }
    }

    pub fn in_file(mut self, file: usize) -> Self {
        self.file = file;
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Diagnostic {}

pub struct SourceFile {
    pub name: String,
    pub text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        SourceFile { name: name.into(), text, line_starts }
    }

    /// 1-based line and column (in characters) of a byte offset.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.text.len());
        let line = self.line_starts.partition_point(|&s| s <= offset) - 1;
        let col = self.text[self.line_starts[line]..offset].chars().count() + 1;
        (line + 1, col)
    }

    fn line_text(&self, line: usize) -> &str {
        let start = self.line_starts[line - 1];
        let end = self.line_starts.get(line).copied().unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches(['\n', '\r'])
    }

    /// rustc-style rendering (`warning:` for warnings):
    /// ```text
    /// error: expected `;`, found `}`
    ///  --> fib.slab:3:18
    ///   |
    /// 3 |     return n }
    ///   |              ^
    ///   = hint: ...
    /// ```
    pub fn render(&self, d: &Diagnostic) -> String {
        let (line, col) = self.line_col(d.span.start as usize);
        let text = self.line_text(line);
        let line_end = self.line_starts[line - 1] + text.len();
        let span_end = (d.span.end as usize).clamp(d.span.start as usize, line_end.max(d.span.start as usize));
        let width = self.text[(d.span.start as usize).min(self.text.len())..span_end.min(self.text.len())].chars().count().max(1);
        let gutter = line.to_string();
        let pad = " ".repeat(gutter.len());
        let label = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let mut out = format!("{label}: {}\n{pad}--> {}:{line}:{col}\n{pad} |\n{gutter} | {text}\n{pad} | {}{}\n", d.message, self.name, " ".repeat(col - 1), "^".repeat(width));
        if let Some(h) = &d.hint {
            out.push_str(&format!("{pad} = hint: {h}\n"));
        }
        out
    }
}

/// All source files of a compilation, indexed by [`Diagnostic::file`].
#[derive(Default)]
pub struct SourceMap {
    pub files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn add(&mut self, file: SourceFile) -> usize {
        self.files.push(file);
        self.files.len() - 1
    }

    pub fn render(&self, d: &Diagnostic) -> String {
        match self.files.get(d.file).or(self.files.first()) {
            Some(f) => f.render(d),
            None => format!("error: {}\n", d.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_column() {
        let f = SourceFile::new("t.slab", "ab\ncdé\nf");
        assert_eq!(f.line_col(0), (1, 1));
        assert_eq!(f.line_col(3), (2, 1));
        assert_eq!(f.line_col(7), (2, 4)); // after the two-byte é
        assert_eq!(f.line_col(8), (3, 1));
        assert_eq!(f.line_col(9), (3, 2)); // end of text
    }

    #[test]
    fn rendering() {
        let f = SourceFile::new("fib.slab", "fn f() {\n    return n }\n");
        let d = Diagnostic::new("expected `;`, found `}`", Span::new(22, 23)).hint("add a `;`");
        let out = f.render(&d);
        assert_eq!(
            out,
            "error: expected `;`, found `}`\n --> fib.slab:2:14\n  |\n2 |     return n }\n  |              ^\n  = hint: add a `;`\n"
        );
    }
}
