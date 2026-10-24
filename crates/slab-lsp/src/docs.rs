//! Documentation of declarations from `///` comments (and `//!` for modules): collected by
//! scanning the source directly — the compiler treats them as plain comments, and the server
//! attaches whatever run of them sits above the declaration they hover on.

/// The run of `///` lines immediately above the line containing `at` (multi-line statements
/// with the anchor mid-line search the same lines, which is what one wants for `let`s).
pub fn doc_before(text: &str, at: usize) -> Option<String> {
    let at = at.min(text.len());
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let mut lines: Vec<&str> = Vec::new();
    let mut boundary = line_start;
    while boundary > 0 {
        // the previous line, without its newline
        let prev_start = text[..boundary - 1].rfind('\n').map_or(0, |i| i + 1);
        let line = &text[prev_start..boundary - 1];
        let trimmed = line.trim();
        let Some(content) = trimmed.strip_prefix("///") else { break };
        lines.push(content.trim_start());
        boundary = prev_start;
    }
    (!lines.is_empty()).then(|| lines.iter().rev().copied().collect::<Vec<_>>().join("\n"))
}

/// The run of `//!` lines at the top of a file: the module's documentation.
pub fn module_doc(text: &str) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        let Some(content) = line.trim().strip_prefix("//!") else {
            // blank lines between //! lines keep the block going
            if lines.is_empty() && line.trim().is_empty() {
                continue;
            }
            break;
        };
        lines.push(content.trim_start());
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_the_run_above() {
        let text = "import a;\n\n/// Adds one.\n/// Use it for small numbers.\nfn f() {}\n\nfn g() {}\n";
        assert_eq!(doc_before(text, text.find("fn f").unwrap()).as_deref(), Some("Adds one.\nUse it for small numbers."));
        assert_eq!(doc_before(text, text.find("fn g").unwrap()), None, "the run stops after a blank line");
        assert_eq!(doc_before(text, text.find("import").unwrap()), None);
    }

    #[test]
    fn an_anchor_mid_line_searches_above_its_line() {
        let text = "/// the fn's doc\nfn f() {\n    let x = 1;\n}\n";
        assert_eq!(doc_before(text, text.find("fn f").unwrap()).as_deref(), Some("the fn's doc"));
        // a local deeper in the body does not inherit the fn's doc
        assert_eq!(doc_before(text, text.find("let x").unwrap() + 5), None, "the fn line breaks the run");
        // a doc directly above the anchor's line attaches, mid-line or not
        let text2 = "/// note\nlet y: int = 2;\n";
        assert_eq!(doc_before(text2, text2.find("y:").unwrap()).as_deref(), Some("note"));
    }

    #[test]
    fn non_doc_lines_stop_the_run() {
        let text = "/// stray\nfn f() {}\nlet x: int = 1;\n";
        assert_eq!(doc_before(text, text.find("let x").unwrap()), None, "the fn line breaks the run");
    }

    #[test]
    fn module_docs_come_from_bang_comments() {
        let text = "//! Utilities.\n//! Second line.\n\npub fn f() {}\n";
        assert_eq!(module_doc(text).as_deref(), Some("Utilities.\nSecond line."));
        assert_eq!(module_doc("pub fn f() {}"), None);
        let mixed = "import a;\n\n//! too late\n";
        assert_eq!(module_doc(mixed), None);
    }
}