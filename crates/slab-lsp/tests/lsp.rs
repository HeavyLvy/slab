//! End-to-end tests for the language server's analysis: they drive the *real* compiler on
//! the repo's own examples plus throwaway projects in a temp dir.

use std::path::PathBuf;

use slab_lsp::lines::{Enc, LineIndex};
use slab_lsp::state::Workspace;
use slab_lsp::{analysis, completion, state, symbols, tokens};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn examples() -> PathBuf {
    repo().join("examples")
}

/// A temp project directory, removed on drop (like the compiler's own project tests).
struct Dir(std::path::PathBuf);

impl Dir {
    fn new(tag: &str, files: &[(&str, &str)]) -> Dir {
        let dir = std::env::temp_dir().join(format!("slab-lsp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (name, text) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        Dir(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MAIN: &str = "import std::print;\nimport util;\n\nstruct Point { x: int, y: int }\n\nglobal ticks: int = 0;\n\n#[load]\nfn boot() {\n    let p = Point { x: 0, y: 1 };\n    print(f\"booted {p.x}\");\n    ticks = util::clamp(ticks, 0, 100);\n}\n";
const UTIL: &str = "pub fn clamp(x: int, lo: int, hi: int) -> int {\n    if x < lo { return lo; }\n    if x > hi { return hi; }\n    return x;\n}\n";

fn file_uri(path: &std::path::Path) -> String {
    state::path_to_uri(path)
}

#[test]
fn project_diagnostics_and_symbols() {
    let d = Dir::new("sym", &[("main.slab", MAIN), ("util.slab", UTIL)]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("main.slab"));
    let publishes = ws.open(&uri, MAIN, 1);
    assert_eq!(publishes.len(), 1);
    assert!(publishes[0].diagnostics.is_empty(), "clean project: {:?}", publishes[0].diagnostics.iter().map(|x| x.message.clone()).collect::<Vec<_>>());

    let (state, _doc, module) = ws.locate(&uri).expect("located");
    assert_eq!(module, Some(0));
    let symbols = symbols::document_symbols(state, module.unwrap(), Enc::Utf16);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["print", "util", "Point", "ticks", "boot"]);
    let boot = symbols.iter().find(|s| s.name == "boot").unwrap();
    assert_eq!(boot.kind, 12);
    assert_eq!(boot.detail.as_deref(), Some("#[load] fn boot()"));
}

#[test]
fn diagnostics_follow_edits_and_clears() {
    let d = Dir::new("edits", &[("main.slab", "import util;\n"), ("util.slab", "pub fn f() {}")]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("util.slab"));
    let publishes = ws.open(&uri, "pub fn f() {}", 1);
    assert!(publishes[0].diagnostics.is_empty());

    // a broken edit: a type error is reported (valid syntax, wrong types)
    let broken = "pub fn f() -> str { return \"s\"; }\nfn g() -> int { return \"no\"; }\n";
    let publishes = ws.change(&uri, broken, 2);
    let msgs: Vec<&str> = publishes[0].diagnostics.iter().map(|x| x.message.as_str()).collect();
    assert!(msgs.iter().any(|m| m.contains("mismatched types")), "{msgs:?}");

    // fixed again: publishes is empty and clears the document
    let publishes = ws.change(&uri, "pub fn f() {}", 3);
    assert!(publishes[0].diagnostics.is_empty(), "{:?}", publishes[0].diagnostics);
}

#[test]
fn goto_definition_across_modules() {
    let d = Dir::new("xmod", &[("main.slab", MAIN), ("util.slab", UTIL)]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("main.slab"));
    ws.open(&uri, MAIN, 1);

    let text = MAIN;
    // find `util::clamp`'s `clamp` in MAIN
    let at = text.find("util::clamp").unwrap() + "util::".len();
    let (state, _doc, module) = ws.locate(&uri).unwrap();
    let module = module.unwrap();
    let def = state.goto_definition(module, at).expect("resolves");
    let target_file = def.file_idx;
    let target_name = state.sources.files[target_file].name.clone();
    let target_text = state.sources.files[target_file].text.clone();
    assert!(target_name.ends_with("util.slab"));
    let range = LineIndex::new(&target_text).position(&target_text, def.span.start as usize, Enc::Utf8);
    assert_eq!((range.line, range.character), (0, 7), "the `clamp` declaration in util.slab (after `pub fn `)");

    // a local: `p` in `let p = Point { .. }`
    let at = text.find("p.x").unwrap();
    let def = state.goto_definition(module, at).expect("resolves");
    assert_eq!(def.file_idx, state.modules[module].file_idx);
    let line = LineIndex::new(text).position(text, def.span.start as usize, Enc::Utf8);
    assert_eq!(line.line, 9, "the `let p` line");

    // a type name: `Point` in the literal
    let at = text.find("Point {").unwrap();
    state.goto_definition(module, at).expect("resolves");
}

#[test]
fn go_to_definition_opens_the_standard_library_source() {
    let d = Dir::new("stddef", &[("main.slab", MAIN), ("util.slab", UTIL)]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("main.slab"));
    ws.open(&uri, MAIN, 1);
    let (state, _doc, module) = ws.locate(&uri).unwrap();
    let module = module.unwrap();
    // `print(` is declared in the embedded std.slab, whose file is the cache copy
    let at = MAIN.find("print(").unwrap();
    let def = state.goto_definition(module, at + 1).expect("resolves");
    let path = &state.sources.files[def.file_idx].name;
    assert!(path.ends_with("std.slab"), "{path}");
    let text = &state.sources.files[def.file_idx].text;
    assert_eq!(&text[def.span.start as usize..def.span.end as usize], "print");
    // the server writes the embedded sources where that path points
    slab::stdlib::write_cache().expect("cache dir");
    assert!(std::fs::read_to_string(path).is_ok_and(|t| &t == text), "{path}");
}

#[test]
fn hover_shows_signatures_and_builtins() {
    let d = Dir::new("hover", &[("main.slab", MAIN), ("util.slab", UTIL)]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("main.slab"));
    ws.open(&uri, MAIN, 1);
    let (state, _doc, module) = ws.locate(&uri).unwrap();
    let module = module.unwrap();

    // a standard-library function: its declaration and docs, written in slab
    let at = MAIN.find("print(").unwrap();
    let hover = state.hover(module, at + 1).expect("hover");
    assert!(hover.contains("fn print(value: any)") && hover.contains("in `std`") && hover.contains("tellraw"), "{hover}");

    // a user function
    let at = MAIN.find("util::clamp").unwrap() + "util::".len();
    let hover = state.hover(module, at).expect("hover");
    assert!(hover.contains("fn clamp(x: int, lo: int, hi: int) -> int"), "{hover}");
    assert!(hover.contains("module `util`"), "{hover}");

    // a struct field
    let at = MAIN.find("p.x").unwrap() + 2;
    let hover = state.hover(module, at).expect("hover");
    assert!(hover.contains("x: int"), "{hover}");
    assert!(hover.contains("Point"), "{hover}");

    // a local with its inferred type
    let at = MAIN.find("let p").unwrap() + 4;
    let hover = state.hover(module, at).expect("hover");
    assert!(hover.contains("let p"), "{hover}");

    // `f"booted {p.x}"` hole content still resolves: the `x` inside the hole is a field
    let at = MAIN.find("{p.x}").unwrap() + 3;
    let hover = state.hover(module, at).expect("hover");
    assert!(hover.contains("x: int"), "{hover}");
}

#[test]
fn semantic_tokens_classify() {
    let src = "import std::print;\nimport game::effect;\nimport util;\n\n#[load]\nfn boot() {\n    let s = \"a\\\"b\";\n    print(f\"x {s}\");\n    as @a at @s {\n        effect::give(@s, \"minecraft:speed\", 10s);\n    }\n    util::clamp(0, 0, 1);\n}\n";
    let file = slab::diag::SourceFile::new("t.slab", src);
    let program = slab::syntax::parse(&file.text).expect("parses");
    let toks = tokens::document_tokens(&file.text, &program);
    let tok_at = |find: &str, delta: usize| -> Option<tokens::ST> {
        let start = src.find(find)? + delta;
        toks.iter().find(|t| t.start == start).map(|t| t.ty)
    };
    assert_eq!(tok_at("import", 0), Some(tokens::ST::Keyword));
    assert_eq!(tok_at("load", 0), Some(tokens::ST::Decorator));
    assert_eq!(tok_at("boot", 0), Some(tokens::ST::Function));
    assert_eq!(tok_at("\"a", 0), Some(tokens::ST::Str));
    assert_eq!(tok_at("@a", 0), None, "selectors stay with the TextMate grammar");
    assert_eq!(tok_at("10s", 0), Some(tokens::ST::Number));
    let builtin_start = src.find("effect::give").unwrap();
    let t = toks.iter().find(|t| t.start == builtin_start).expect("effect head");
    assert_eq!(t.ty, tokens::ST::Namespace);
    let give = toks.iter().find(|t| t.start == builtin_start + "effect::".len()).unwrap();
    assert_eq!(give.ty, tokens::ST::Function);
    // f-string: the hole's identifier is a variable token with the right text
    let hole_start = src.find("f\"").unwrap() + "f\"x {".len();
    let t = toks.iter().find(|t| t.start == hole_start).unwrap();
    assert_eq!(&src[t.start..t.start + t.len], "s");
    assert_eq!(t.ty, tokens::ST::Var);
}

#[test]
fn completion_modes() {
    let d = Dir::new("toknone", &[("main.slab", MAIN), ("util.slab", UTIL)]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("main.slab"));
    ws.open(&uri, MAIN, 1);

    // plain: keywords + builtins + items
    let items = completion::completions(&ws, &uri, MAIN.len()).unwrap();
    assert!(items.iter().any(|i| i.label == "fn" && i.kind == 14));
    assert!(items.iter().any(|i| i.label == "std" && i.kind == 9), "built-in module heads are offered");
    assert!(items.iter().any(|i| i.label == "boot" && i.kind == 3));

    // after `.` on a struct: fields
    let src = "import std::print;\nstruct Point { x: int, y: int }\n#[load]\nfn boot() {\n    let p = Point { x: 0, y: 0 };\n    print(p.x);\n}\n";
    let publishes = ws.change(&uri, src, 2);
    assert!(publishes[0].diagnostics.is_empty(), "{:?}", publishes[0].diagnostics.iter().map(|x| x.message.clone()).collect::<Vec<_>>());
    let at = src.find("p.x").unwrap() + 2; // right after the dot
    let items = completion::completions(&ws, &uri, at).unwrap();
    assert!(items.iter().any(|i| i.label == "y" && i.kind == 5), "{items:?}");

    // after `::` on an imported module: its public items
    let publishes = ws.change(&uri, MAIN, 3);
    assert!(publishes[0].diagnostics.is_empty());
    let at = MAIN.find("util::clamp").unwrap() + "util::".len();
    let items = completion::completions(&ws, &uri, at).unwrap();
    assert!(items.iter().any(|i| i.label == "clamp" && i.kind == 3), "{items:?}");

    // the standard library is made of modules: a full path, and an imported alias
    let src = "import game::effect;\nfn f() {\n    game::\n    effect::\n    std::\n}\n";
    ws.change(&uri, src, 4);
    let at = src.find("game::").unwrap() + "game::".len();
    let items = completion::completions(&ws, &uri, at).unwrap();
    assert!(items.iter().any(|i| i.label == "summon" && i.kind == 3 && i.detail.as_deref().is_some_and(|d| d.starts_with("fn summon(kind: str"))), "{items:?}");
    assert!(items.iter().any(|i| i.label == "Entity" && i.kind == 22) && items.iter().any(|i| i.label == "tag" && i.kind == 9), "{items:?}");
    let at = src.find("effect::\n").unwrap() + "effect::".len();
    let items = completion::completions(&ws, &uri, at).unwrap();
    assert!(items.iter().any(|i| i.label == "give") && items.iter().any(|i| i.label == "clear"), "{items:?}");
    let at = src.find("std::").unwrap() + "std::".len();
    let items = completion::completions(&ws, &uri, at).unwrap();
    assert!(items.iter().any(|i| i.label == "print"), "{items:?}");
}

#[test]
fn tolerant_analysis_keeps_working_under_errors() {
    let d = Dir::new("plain", &[("main.slab", MAIN), ("util.slab", UTIL)]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("main.slab"));
    // a type error mid-file: diagnostics arrive, hover still knows the builtin
    let broken = "import std::print;\nimport util;\nfn f() {\n    print(util::clamp(\"a\", 0, 1));\n}\n";
    let publishes = ws.open(&uri, broken, 1);
    assert!(publishes[0].diagnostics.iter().any(|x| x.message.contains("mismatched types")));
    let (_, doc, module) = ws.locate(&uri).unwrap();
    let at = broken.find("print(").unwrap() + 1;
    let hover = ws.project(&crate::state::root_for(&doc.path).0).and_then(|s| s.hover(module.unwrap(), at)).expect("hover still works");
    assert!(hover.contains("fn print(value: any)"), "{hover}");
}

#[test]
fn standalone_files_check_without_main_slab() {
    let d = Dir::new("fields", &[("loose.slab", "fn f() -> int { return 1; }\nfn g() -> int { return \"s\"; }\n")]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("loose.slab"));
    let publishes = ws.open(&uri, "fn g() -> str { return 1; }", 1);
    assert!(publishes[0].diagnostics.iter().any(|x| x.message.contains("mismatched types")));
}

#[test]
fn line_positions_round_trip_on_the_examples() {
    for file in [examples().join("projects/e2e/main.slab"), examples().join("projects/e2e/mathx.slab")] {
        let text = std::fs::read_to_string(&file).unwrap();
        let idx = LineIndex::new(&text);
        let p = idx.position(&text, text.len() / 2, Enc::Utf8);
        let back = idx.offset(&text, p, Enc::Utf8).unwrap();
        assert!(back >= text.len() / 2 - 4 && back <= text.len() / 2 + 4);
    }
}

#[test]
fn the_examples_parse_and_the_e2e_project_checks() {
    let e2e = examples().join("projects/e2e/main.slab");
    let mut ws = Workspace::default();
    let uri = file_uri(&e2e);
    let text = std::fs::read_to_string(&e2e).unwrap();
    let publishes = ws.open(&uri, &text, 1);
    let messages: Vec<String> = publishes.iter().flat_map(|p| p.diagnostics.iter().map(|d| d.message.clone())).collect();
    assert!(messages.is_empty(), "{messages:?}");
    let (state, _doc, module) = ws.locate(&uri).unwrap();
    let symbols = symbols::document_symbols(state, module.unwrap(), Enc::Utf16);
    assert!(!symbols.is_empty());
    // and analysis at every identifier position never panics
    let (state, doc, module) = ws.locate(&uri).unwrap();
    let module = module.unwrap();
    let mut hits = Vec::new();
    analysis::each_ident(&state.modules[module].program, &mut hits);
    for h in hits {
        let _ = state.hover(module, (h.span.start as usize + h.span.end as usize) / 2);
        let _ = state.goto_definition(module, (h.span.start as usize + h.span.end as usize) / 2);
        let _ = completion::completions(&ws, &uri, h.span.end as usize);
        let _ = doc;
    }
}

#[test]
fn hover_shows_doc_comments() {
    // on the declaration, on a call of it, on a global, and the file's //! doc on the module
    let text = "//! Utilities.\n\n/// Adds one.\n/// (and only that)\nfn add1(a: int) -> int {\n    return a + 1;\n}\n\n/// a counter\nglobal count: int = 0;\n\nfn main() {\n    let n = add1(1);\n    count += n;\n}\n";
    let d = Dir::new("docs", &[("main.slab", text)]);
    let mut ws = Workspace::default();
    let uri = file_uri(&d.0.join("main.slab"));
    ws.open(&uri, text, 1);

    let (state, _doc, module) = ws.locate(&uri).unwrap();
    let module = module.unwrap();

    // on the declaration
    let at = text.find("add1").unwrap();
    let hover = state.hover(module, at).unwrap();
    assert!(hover.contains("Adds one."), "{hover}");
    assert!(hover.contains("(and only that)"), "{hover}");

    // on a call of it
    let at = text.find("add1(1)").unwrap();
    let hover = state.hover(module, at).unwrap();
    assert!(hover.contains("Adds one."), "{hover}");

    // on a use of the documented global
    let at = text.find("count += n").unwrap();
    let hover = state.hover(module, at).unwrap();
    assert!(hover.contains("a counter"), "{hover}");

    // the module doc: another file whose//!-documented module is imported
    let d2 = Dir::new("docs-mod", &[
        ("main.slab", "import other;\n\nfn go() {\n    other::helper();\n}\n"),
        ("other.slab", "//! A module of its own.\n\npub fn helper() {}\n"),
    ]);
    let mut ws2 = Workspace::default();
    let uri3 = file_uri(&d2.0.join("main.slab"));
    ws2.open(&uri3, "import other;\n\nfn go() {\n    other::helper();\n}\n", 1);
    let (state2, _doc2, module2) = ws2.locate(&uri3).unwrap();
    let at = "import other;\n\nfn go() {\n    other::helper();\n}\n".find("other::helper").unwrap();
    let hover = state2.hover(module2.unwrap(), at).unwrap();
    assert!(hover.contains("A module of its own."), "{hover}");
}