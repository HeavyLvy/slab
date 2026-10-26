//! Every ```slab block of the guide and the introduction is a real program (or a set of items):
//! this checks each one with the compiler, so the documentation cannot show code that does not
//! work. A block that already starts with `import` is checked as it is; the others get
//! `import std::*; import game::*;` in front. Blocks that are not slab (`text`, `toml`) are skipped.

use std::fs;
use std::path::{Path, PathBuf};

fn blocks(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((n, line)) = lines.next() {
        if line.trim_end() == "```slab" {
            let mut body = Vec::new();
            for (_, l) in lines.by_ref() {
                if l.starts_with("```") {
                    break;
                }
                body.push(l);
            }
            out.push((n + 2, body.join("\n")));
        }
    }
    out
}

fn pages() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    let mut pages = vec![root.join("introduction.md")];
    for entry in fs::read_dir(root.join("guide")).unwrap().flatten() {
        pages.push(entry.path());
    }
    pages.sort();
    pages
}

#[test]
fn the_examples_in_the_guide_compile() {
    let dir = std::env::temp_dir().join(format!("slab-docs-{}", std::process::id()));
    let mut problems = Vec::new();
    let mut checked = 0;
    for page in pages() {
        let text = fs::read_to_string(&page).unwrap();
        for (line, code) in blocks(&text) {
            let source = if code.trim_start().starts_with("import") { code.clone() } else { format!("import std::*; import game::*;\n{code}") };
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("main.slab"), &source).unwrap();
            checked += 1;
            if let Err(e) = slab::compile::check(&dir) {
                problems.push(format!("{}:{line}\n{}\n{}", page.file_name().unwrap().to_string_lossy(), code, e.render()));
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
    assert!(checked >= 20, "found only {checked} examples");
    assert!(problems.is_empty(), "{} example(s) do not compile:\n\n{}", problems.len(), problems.join("\n----\n"));
}

/// The highlighter of the book (`docs/theme/slab-highlight.js`) lists the keywords by hand; this
/// makes sure it has every one the compiler has.
#[test]
fn the_book_highlights_every_keyword() {
    let js = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/theme/slab-highlight.js")).unwrap();
    let line = js.lines().find(|l| l.trim_start().starts_with("keyword:")).expect("a keyword line");
    let listed: Vec<&str> = line.split('\'').nth(1).unwrap().split_whitespace().collect();
    for k in slab::syntax::token::Kw::ALL {
        let word = k.as_str();
        if matches!(word, "true" | "false") {
            continue;
        }
        assert!(listed.contains(&word), "`{word}` is a keyword of the language but not in docs/theme/slab-highlight.js");
    }
}
