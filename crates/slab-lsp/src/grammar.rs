//! The TextMate grammar generator: `slab-lsp grammar [TARGET]` writes
//! `slab.tmLanguage.json`. The vocabulary comes from the compiler itself (keywords, operators,
//! builtin names, selector letters, time suffixes), so the grammar cannot drift from the
//! language: a keyword added to the lexer is invisible to the grammar until the generator
//! runs, and a drift test fails until then. Only structural rules (f-string holes, selector
//! argument brackets, attributes, escapes) are hand-written here.

use serde_json::{Map, Value, json};
use slab::syntax::lexer::{SELECTOR_VARS, TIME_SUFFIXES};
use slab::syntax::token::{Kw, KwClass};

/// The generated grammar as pretty JSON text (with a generator marker).
pub fn tm_language_text() -> String {
    let mut v = tm_language();
    v.as_object_mut().unwrap().insert(
        "__generated__".to_owned(),
        json!("by `slab-lsp grammar` from the compiler's definitions — regenerate, never edit"),
    );
    serde_json::to_string_pretty(&v).unwrap()
}

/// The grammar value (without the marker).
pub fn tm_language() -> Value {
    json!({
        "name": "slab",
        "scopeName": "source.slab",
        "fileTypes": ["slab"],
        "patterns": [
            { "include": "#doc-comment" },
            { "include": "#module-comment" },
            { "include": "#line-comment" },
            { "include": "#block-comment" },
            { "include": "#declaration-names" },
            { "include": "#decl-fn" },
            { "include": "#decl-struct" },
            { "include": "#decl-global" },
            { "include": "#decl-const" },
            { "include": "#decl-objective" },
            { "include": "#modifier-keyword" },
            { "include": "#call" },
            { "include": "#attribute" },
            { "include": "#fstring" },
            { "include": "#string" },
            { "include": "#selector" },
            { "include": "#relative-coordinate" },
            { "include": "#control-keywords" },
            { "include": "#context-keywords" },
            { "include": "#task-keywords" },
            { "include": "#boolean-keywords" },
            { "include": "#builtins" },
            { "include": "#builtin-modules" },
            { "include": "#numbers" },
            { "include": "#operators" },
        ],
        "repository": repository(),
    })
}

fn repository() -> Map<String, Value> {
    let mut r = Map::new();

    r.insert(
        "doc-comment".into(),
        json!({
            "comment": "/// documentation of the item below",
            "match": r"///.*$",
            "name": "comment.line.documentation.slab",
        }),
    );
    r.insert(
        "module-comment".into(),
        json!({
            "comment": "//! documentation of the file (module)",
            "match": r"//!.*$",
            "name": "comment.line.documentation.module.slab",
        }),
    );

    r.insert(
        "line-comment".into(),
        json!({
            "comment": "// to end of line",
            "match": "//.*$",
            "name": "comment.line.double-slash.slab",
        }),
    );
    r.insert(
        "block-comment".into(),
        json!({
            "comment": "nesting is not modelled (the lexer accepts nested block comments)",
            "begin": r"/\*",
            "end": r"\*/",
            "name": "comment.block.slab",
        }),
    );

    // One rule per declaring keyword, so the declared name gets a scope as well.
    for (keyword, name_scope) in [
        ("fn", "entity.name.function.slab"),
        ("struct", "entity.name.type.slab"),
        ("global", "variable.other.slab"),
        ("const", "variable.other.slab"),
        ("objective", "entity.name.objective.slab"),
    ] {
        r.insert(
            format!("decl-{keyword}"),
            json!({
                "match": format!(r"\b({keyword})\s+[A-Za-z_]\w*"),
                "captures": {
                    "1": { "name": "keyword.declaration.slab" },
                    "2": { "name": name_scope },
                },
            }),
        );
    }
    r.insert(
        "declaration-names".into(),
        json!({
            "match": r"\b(?:pub|import|let)\b",
            "name": "keyword.declaration.slab",
        }),
    );
    r.insert(
        "modifier-keyword".into(),
        keyword_rule(KwClass::Modifier),
    );
    r.insert(
        "attribute".into(),
        json!({
            "comment": "#[load], #[tick], #[every(1s)]",
            "match": r"#\s*\[([A-Za-z_]\w*)(?:\([^)]*\))?\s*\]",
            "name": "meta.attribute.slab",
            "captures": {
                "1": { "name": "entity.name.decorator.slab" },
                "2": { "name": "meta.attribute-arguments.slab" },
            },
        }),
    );

    r.insert(
        "fstring".into(),
        json!({
            "comment": r#"f"text {expression} text"; {{ and }} are literal braces"#,
            "name": "string.interpolated.slab",
            "begin": "f\"",
            "beginCaptures": { "0": { "name": "punctuation.definition.string.begin.slab" } },
            "end": "\"|(?=\\n)",
            "endCaptures": { "0": { "name": "punctuation.definition.string.end.slab" } },
            "patterns": [
                { "include": "#string-escape" },
                { "match": r"\{\{|\}\}", "name": "constant.character.escape.slab" },
                {
                    "begin": "\\{",
                    "beginCaptures": { "0": { "name": "punctuation.definition.interpolation.begin.slab" } },
                    "end": "\\}|(?=\\n)",
                    "endCaptures": { "0": { "name": "punctuation.definition.interpolation.end.slab" } },
                    "name": "meta.interpolation.slab",
                    "patterns": [ { "include": "#expr" } ],
                },
            ],
        }),
    );

    r.insert(
        "string".into(),
        json!({
            "comment": "strings may not contain a raw newline",
            "begin": "\"",
            "beginCaptures": { "0": { "name": "punctuation.definition.string.begin.slab" } },
            "end": "\"|(?=\\n)",
            "endCaptures": { "0": { "name": "punctuation.definition.string.end.slab" } },
            "name": "string.quoted.double.slab",
            "patterns": [ { "include": "#string-escape" } ],
        }),
    );
    r.insert(
        "string-escape".into(),
        json!({
            "match": r#"\\u\{[0-9a-fA-F_]+\}|\\[ntr0\\"']"#,
            "name": "constant.character.escape.slab",
        }),
    );

    let vars: String = SELECTOR_VARS.iter().collect();
    r.insert(
        "selector".into(),
        json!({
            "comment": format!(r"@{vars} with optional [args]: @e[type=zombie, distance=..10, tag=!boss]"),
            "match": format!(r#"(@[{vars}])(\[(?:[^\[\]{{}}"'\n]|"[^"\n]*"|'[^'\n]*'|\{{[^{{}}]*\}})*\])?"#),
            "name": "meta.selector.slab",
            "captures": {
                "1": { "name": "entity.name.selector.slab" },
                "2": { "name": "meta.selector-arguments.slab" },
            },
        }),
    );

    r.insert(
        "relative-coordinate".into(),
        json!({
            "comment": "~, ~1, ~-2.5, ^3 (also matched inside f-string holes)",
            "match": r"[~^](?:-?\d+(?:\.\d+)?)?",
            "name": "constant.other.coordinate.slab",
        }),
    );

    r.insert(
        "control-keywords".into(),
        json!({
            "match": keyword_alternation(KwClass::Control),
            "name": "keyword.control.slab",
        }),
    );
    // `task fn` and `start name(..)` are words only in these positions, so `start` stays usable as a name
    r.insert(
        "task-keywords".into(),
        json!({
            "comment": "task fn, and start before a task's name: contextual, not reserved",
            "match": r"\b(?:task(?=\s+fn\b)|start(?=\s+[A-Za-z_]))",
            "name": "keyword.declaration.slab",
        }),
    );
    r.insert(
        "context-keywords".into(),
        json!({
            "match": keyword_alternation(KwClass::Context),
            "name": "keyword.control.context.slab",
        }),
    );
    r.insert(
        "boolean-keywords".into(),
        json!({
            "match": keyword_alternation(KwClass::Literal),
            "name": "constant.language.boolean.slab",
        }),
    );

    r.insert(
        "builtins".into(),
        json!({
            "comment": "builtin functions in call position, longest names first",
            "match": builtin_call_pattern(),
            "name": "support.function.slab",
        }),
    );
    r.insert(
        "builtin-modules".into(),
        json!({
            "comment": "the built-in modules at the head of a path: std::, game::",
            "match": r"\b(?:std|game)\b(?=\s*::)",
            "name": "support.namespace.slab",
        }),
    );

    let suffix: String = TIME_SUFFIXES.iter().collect();
    r.insert(
        "numbers".into(),
        json!({
            "comment": format!("hex `0x…`, `_` separators, floats, and time literals ending in one of {suffix}"),
            "patterns": [
                { "match": r"\b0[xX][0-9a-fA-F_]+(?![\w])", "name": "constant.numeric.hex.slab" },
                { "match": format!(r"\b\d[\d_]*[{suffix}](?![\w])", ), "name": "constant.numeric.time.slab" },
                { "match": r"\b\d[\d_]*\.\d[\d_]*", "name": "constant.numeric.float.slab" },
                { "match": r"\b\d[\d_]*\b", "name": "constant.numeric.slab" },
            ],
        }),
    );

    r.insert(
        "expr".into(),
        json!({
            "comment": "what an f-string hole may contain",
            "patterns": [
                { "include": "#string-escape" },
                { "include": "#string" },
                { "include": "#selector" },
                { "include": "#relative-coordinate" },
                { "include": "#control-keywords" },
                { "include": "#boolean-keywords" },
                { "include": "#builtins" },
                { "include": "#builtin-modules" },
                { "include": "#numbers" },
                { "include": "#operators" },
                { "include": "#call" },
            ],
        }),
    );
    r.insert(
        "call".into(),
        json!({
            "comment": "any call-looking identifier, user function or method",
            "match": r"\b([A-Za-z_]\w*)\b(?=\s*\()",
            "captures": { "1": { "name": "entity.name.function.slab" } },
        }),
    );
    r.insert(
        "operators".into(),
        json!({
            "comment": "every operator token, longest first",
            "match": operators_pattern(),
            "name": "keyword.operator.slab",
        }),
    );

    r
}

/// An alternation of every keyword in a class, word-bounded.
fn keyword_alternation(class: KwClass) -> String {
    let names: Vec<String> = Kw::ALL.iter().filter(|k| k.class() == class).map(|k| escaped(k.as_str())).collect();
    format!(r"\b(?:{})\b", names.join("|"))
}

/// A keyword rule value for a class.
fn keyword_rule(class: KwClass) -> Value {
    json!({
        "match": keyword_alternation(class),
        "name": "storage.modifier.slab", // only the Modifier class uses this
    })
}

/// The builtin call rule: a builtin named right after `::` (`game::tag::add(`), or one of the
/// functions of `std`/`game` called bare (`print(`, after an import). Longest names first.
fn builtin_call_pattern() -> String {
    let sorted = |it: Vec<&str>| {
        let mut names: Vec<String> = it.into_iter().map(escaped).collect();
        names.sort_by_key(|a| std::cmp::Reverse(a.len()));
        names.join("|")
    };
    let fns = slab::stdlib::function_names();
    let qualified = sorted(fns.iter().map(|(_, n)| n.as_str()).collect());
    let bare = sorted(fns.iter().filter(|(m, _)| matches!(*m, "std" | "game")).map(|(_, n)| n.as_str()).collect());
    format!(r"(?:(?<=::)\s*(?:{qualified})|\b(?:{bare}))\b(?=\s*\()")
}

/// Operator alternation, longest first, regex-escaped.
fn operators_pattern() -> String {
    slab::syntax::token::operator_texts().into_iter().map(escaped).collect::<Vec<_>>().join("|")
}

/// Regex-escape grammar-special characters.
fn escaped(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        if "\\.()[]{}^$*+?|".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grammar is generated once and committed; regeneration that produces different
    /// bytes means it has drifted from the language. Fix: run `cargo run -p slab-lsp -- grammar`.
    #[test]
    fn the_committed_grammar_is_current() {
        let committed =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../editors/vscode/syntaxes/slab.tmLanguage.json");
        let current = format!("{}\n", tm_language_text());
        match std::fs::read_to_string(&committed) {
            Ok(existing) => assert_eq!(existing, current, "slab.tmLanguage.json is stale; run `cargo run -p slab-lsp -- grammar`"),
            Err(_) => panic!("missing {}; run `cargo run -p slab-lsp -- grammar`", committed.display()),
        }
    }
}