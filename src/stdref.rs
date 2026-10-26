//! The reference page of the standard library, written from the `.slab` sources themselves: the
//! declarations and their `///` comments are the documentation, so the page cannot go stale.
//! `docs/reference/std.md` is this output (a test fails when it differs;
//! `UPDATE_DOCS=1 cargo test --test stdref` rewrites it).

use crate::stdlib::{MODULES, StdModule, parsed};
use crate::syntax::ast::{self, ItemKind};

/// What each module is for, in the words of the page (the file's own `//!` talks about the compiler).
fn blurb(name: &str) -> &'static str {
    match name {
        "std" => "Things that have nothing to do with Minecraft: printing, `Option` and `Result`, and the numbers with decimals. `Option`, `Result`, `Some`, `None`, `Ok` and `Err` need no import; everything else does: `import std::*;` or `import std::print;`.",
        "game" => "Everything that touches the world: chat, entities, blocks, particles, commands. `import game::*;` brings in all of it, or import single names (`import game::{say, tp};`). A full path always works without an import: `game::say(\"hi\")`.",
        "game::tag" => "Entity tags (`/tag`). `import game::tag;` then `tag::add(e, \"boss\")`.",
        "game::effect" => "Status effects (`/effect`). `import game::effect;` then `effect::give(@s, \"minecraft:speed\", 10s, 1)`.",
        _ => "",
    }
}

/// The `///` lines directly above byte `at`, as Markdown text.
fn docs_above(source: &str, at: usize) -> String {
    let before = &source[..at.min(source.len())];
    let mut lines: Vec<&str> = Vec::new();
    // the line `at` starts on is the item itself; look at the lines above it
    let start_of_line = before.rfind('\n').map_or(0, |i| i + 1);
    for line in source[..start_of_line].lines().rev() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("///") {
            lines.push(rest.strip_prefix(' ').unwrap_or(rest));
        } else {
            break;
        }
    }
    lines.reverse();
    lines.join("\n")
}

/// The line of the source the declaration starts on, without the body or the `;`.
fn signature(source: &str, name_at: usize) -> String {
    let start = source[..name_at].rfind('\n').map_or(0, |i| i + 1);
    let end = source[name_at..].find('\n').map_or(source.len(), |i| name_at + i);
    let line = source[start..end].trim();
    let line = line.strip_suffix('{').unwrap_or(line).trim_end();
    line.strip_suffix(';').unwrap_or(line).to_owned()
}

/// A struct or enum as written, from its line to its closing brace.
fn declaration(source: &str, name_at: usize, item_end: usize) -> String {
    let start = source[..name_at].rfind('\n').map_or(0, |i| i + 1);
    source[start..item_end.min(source.len())].trim_end().to_owned()
}

fn push_item(out: &mut String, heading: &str, code: &str, docs: &str) {
    out.push_str(&format!("{heading}\n\n```slab\n{code}\n```\n\n"));
    if !docs.is_empty() {
        out.push_str(docs);
        out.push_str("\n\n");
    }
}

fn module(out: &mut String, m: &StdModule, program: &ast::Program) {
    out.push_str(&format!("## `{}`\n\n{}\n\n", m.name, blurb(m.name)));
    for item in &program.items {
        match &item.kind {
            ItemKind::Fn(f) if item.public => {
                let at = f.name.span.start as usize;
                push_item(out, &format!("### `{}`", f.name.name), &signature(m.source, at), &docs_above(m.source, item.span.start as usize));
            }
            ItemKind::Struct(s) if item.public => {
                let at = s.name.span.start as usize;
                push_item(out, &format!("### `{}`", s.name.name), &declaration(m.source, at, item.span.end as usize), &docs_above(m.source, item.span.start as usize));
            }
            ItemKind::Enum(e) if item.public => {
                let at = e.name.span.start as usize;
                push_item(out, &format!("### `{}`", e.name.name), &declaration(m.source, at, item.span.end as usize), &docs_above(m.source, item.span.start as usize));
            }
            ItemKind::Impl(i) => {
                let methods: Vec<_> = i.methods.iter().filter(|f| f.public).collect();
                if methods.is_empty() {
                    continue;
                }
                let target = &i.target.name.name;
                out.push_str(&format!("### Methods of `{target}`\n\n"));
                for f in methods {
                    let at = f.decl.name.span.start as usize;
                    push_item(out, &format!("#### `{target}::{}`", f.decl.name.name), &signature(m.source, at), &docs_above(m.source, f.span.start as usize));
                }
            }
            _ => {}
        }
    }
}

/// The whole page.
pub fn reference() -> String {
    let mut out = String::from(
        "<!-- generated from src/stdlib/*.slab by `UPDATE_DOCS=1 cargo test --test stdref`; edit the `///` comments there, not this file -->\n\n# Standard library reference\n\nEvery function, type and method of `std` and `game`, with the documentation written next to it in the library's source. You can read the same text in your editor: hover a name, or ctrl+click it to open its declaration.\n\n`Target` in a signature means a selector (`@a[tag=x]`) or an `Entity`; `Coord` an `int` expression or a relative coordinate (`~`, `^2`); `Num` a number written out (`0.2`); `const` a value known at compile time.\n\n",
    );
    for (m, program) in parsed() {
        module(&mut out, m, program);
    }
    debug_assert!(MODULES.len() == parsed().len());
    out.trim_end().to_owned() + "\n"
}
