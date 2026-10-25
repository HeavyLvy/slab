//! Keeps `docs/lang/compiled-output.md` honest: every example in it is a slab program followed by
//! the commands the compiler really generates for it. This test compiles each program and fails
//! when the listing in the document differs.
//!
//! Re-generate the listings after an intended change with
//! `UPDATE_DOCS=1 cargo test --test compiled_output` and review the diff.
//!
//! Format of an example (the fences must follow each other):
//!
//! ````text
//! ```slab show=double,boot
//! fn double(x: int) -> int { return x * 2; }
//! ```
//! ```mcfunction
//! (generated)
//! ```
//! ````
//!
//! `show` lists the functions to print (default: every function not starting with `_`); a
//! `file=util.slab` attribute makes a block an extra file of the project instead of `main.slab`.
//! Every program is compiled with `import std::*; import game::*;` in front, as namespace `demo`.

use std::fs;
use std::path::{Path, PathBuf};

use slab::compile::{self, Options};
use slab::datapack::ResourceLocation;

const DOC: &str = "docs/lang/compiled-output.md";

struct Example {
    files: Vec<(String, String)>,
    show: Option<Vec<String>>,
    /// Line range (start inclusive, end exclusive) of the lines inside the `mcfunction` fence.
    listing: (usize, usize),
}

fn attr<'a>(fence: &'a str, key: &str) -> Option<&'a str> {
    fence.split_whitespace().find_map(|w| w.strip_prefix(key)?.strip_prefix('='))
}

fn parse(lines: &[&str]) -> Vec<Example> {
    let mut out = Vec::new();
    let mut pending: Vec<(String, String)> = Vec::new();
    let mut show = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(info) = line.strip_prefix("```slab") {
            let start = i + 1;
            let mut end = start;
            while !lines[end].starts_with("```") {
                end += 1;
            }
            let body = lines[start..end].join("\n");
            let file = attr(info, "file").unwrap_or("main.slab").to_owned();
            if let Some(s) = attr(info, "show") {
                show = Some(s.split(',').map(str::to_owned).collect());
            }
            pending.push((file, body));
            i = end + 1;
        } else if line.starts_with("```mcfunction") && !pending.is_empty() {
            let start = i + 1;
            let mut end = start;
            while !lines[end].starts_with("```") {
                end += 1;
            }
            out.push(Example { files: std::mem::take(&mut pending), show: show.take(), listing: (start, end) });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn compile_example(n: usize, ex: &Example) -> String {
    let dir: PathBuf = std::env::temp_dir().join(format!("slab-doc-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (name, body) in &ex.files {
        let text = format!("import std::*; import game::*; {body}");
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let compiled = match compile::compile(&dir, &Options { namespace: Some("demo".into()), ..Options::default() }) {
        Ok(c) => c,
        Err(e) => panic!("example {n} of {DOC} does not compile:\n{}", e.render()),
    };
    let _ = fs::remove_dir_all(&dir);
    let names: Vec<String> = match &ex.show {
        Some(s) => s.clone(),
        None => {
            let mut all: Vec<String> = compiled.pack.functions().map(|(id, _)| id.path().to_owned()).filter(|p| !p.starts_with('_') && !p.contains('/')).collect();
            all.sort();
            all
        }
    };
    let mut parts = Vec::new();
    for name in &names {
        let id = ResourceLocation::new("demo", name).unwrap();
        let f = compiled.pack.function(&id).unwrap_or_else(|| panic!("example {n} of {DOC}: no function `{name}` (it has: {:?})", compiled.pack.functions().map(|(i, _)| i.path().to_owned()).collect::<Vec<_>>()));
        parts.push(format!("# function demo:{name}\n{}", f.render().trim_end()));
    }
    parts.join("\n\n")
}

#[test]
fn the_compiled_output_document_is_up_to_date() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(DOC);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let lines: Vec<&str> = text.lines().collect();
    let examples = parse(&lines);
    assert!(!examples.is_empty(), "{DOC} has no examples");
    let update = std::env::var_os("UPDATE_DOCS").is_some();

    let mut out: Vec<String> = Vec::new();
    let mut cursor = 0;
    let mut stale = Vec::new();
    for (n, ex) in examples.iter().enumerate() {
        let generated = compile_example(n + 1, ex);
        let (start, end) = ex.listing;
        out.extend(lines[cursor..start].iter().map(|l| (*l).to_owned()));
        let current = lines[start..end].join("\n");
        if current != generated {
            stale.push(n + 1);
        }
        out.extend(generated.lines().map(str::to_owned));
        cursor = end;
    }
    out.extend(lines[cursor..].iter().map(|l| (*l).to_owned()));

    if update {
        let mut joined = out.join("\n");
        joined.push('\n');
        fs::write(&path, joined).unwrap();
        return;
    }
    assert!(stale.is_empty(), "the command listings of example(s) {stale:?} in {DOC} are out of date; run `UPDATE_DOCS=1 cargo test --test compiled_output` and review the diff");
}
