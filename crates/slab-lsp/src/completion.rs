//! Completion: what could be typed at the cursor. Three modes — after `::` (module items
//! and module paths), after `.` (struct fields, list/string methods, builtin members) and
//! plain (keywords, builtins, project items). Plus type names after `: `.

use std::path::Path;

use slab::sema::builtins;
use slab::sema::hir::ExprKind as HirKind;
use slab::sema::ty::Ty;
use slab::syntax::token::Tok;

use crate::analysis::hir_ctx_at;
use crate::defs::{Def, DefKind};
use crate::resolve::Item;
use crate::state::{ProjectState, Workspace};

/// Completion for one position. `None` when there is nothing sensible (e.g. inside a string).
pub fn completions(ws: &Workspace, uri: &str, offset: usize) -> Option<Vec<Item>> {
    let (state, doc, module) = ws.locate(uri)?;
    let module = module?;
    if inside_string(&doc.text, offset) {
        return None;
    }
    let before = &doc.text[..offset.min(doc.text.len())];
    if before.ends_with("::") {
        return module_items(state, module, &chain_before(before, before.len() - 2));
    }
    if before.ends_with('.') {
        // `a..b` is a range, not a member access
        let is_range = before.strip_suffix('.').is_some_and(|b| b.ends_with('.'));
        if !is_range {
            let items = builtin_members(&ident_before(before, before.len() - 1))
                .or_else(|| member_items(ws, uri, module, offset))
                .or_else(|| any_fields(state));
            return items;
        }
    }
    if before.ends_with(':') {
        return type_items(state, module);
    }
    plain_items(state, module)
}

/// Which module chains to offer after `::`: deeper module names, plus the target module's items.
fn module_items(state: &ProjectState, module: usize, chain: &[String]) -> Option<Vec<Item>> {
    let prefix = if chain.is_empty() { String::new() } else { format!("{}::", chain.join("::")) };
    let mut out = Vec::new();
    for name in project_module_names(state) {
        if let Some(tail) = name.strip_prefix(&prefix)
            && !tail.is_empty()
            && !tail.contains("::")
            && tail != "main"
        {
            out.push(Item { label: tail.to_owned(), kind: 9, detail: Some(format!("module `{name}`")), docs: None, insert: None });
        }
    }
    if let Some(target) = module_index(state, &chain.join("::")) {
        let only_pub = target != module;
        for def in state.modules[target].defs.defs().iter().filter(|d| d.kind != DefKind::Import) {
            if only_pub && !def.public {
                continue;
            }
            out.push(Item { label: def.name.clone(), kind: item_kind(def), detail: Some(def.detail.clone()), docs: None, insert: None });
        }
    }
    dedupe(&mut out);
    Some(out)
}

/// Type names after ` : `.
fn type_items(state: &ProjectState, module: usize) -> Option<Vec<Item>> {
    let mut out = vec![
        Item { label: "int".into(), kind: 14, detail: None, docs: None, insert: None },
        Item { label: "bool".into(), kind: 14, detail: None, docs: None, insert: None },
        Item { label: "str".into(), kind: 14, detail: None, docs: None, insert: None },
        Item { label: "list<T>".into(), kind: 14, detail: None, docs: None, insert: Some("list<${1:int}>".to_owned()) },
    ];
    for m in &state.modules {
        for def in m.defs.by_kind(DefKind::Struct) {
            if m.name != state.modules[module].name && !def.public {
                continue;
            }
            out.push(Item { label: def.name.clone(), kind: 22, detail: Some(def.detail.clone()), docs: None, insert: None });
        }
    }
    dedupe(&mut out);
    Some(out)
}

/// Keywords, builtins and the project's items (pub ones when they live in other modules).
fn plain_items(state: &ProjectState, module: usize) -> Option<Vec<Item>> {
    use slab::syntax::token::Kw;
    let mut out: Vec<Item> = Kw::ALL.iter().map(|k| Item { label: k.as_str().to_owned(), kind: 14, detail: None, docs: None, insert: None }).collect();
    for b in builtins::BUILTINS {
        out.push(builtin_item(b));
    }
    for m in &state.modules {
        for def in m.defs.defs().iter() {
            if def.kind == DefKind::Import {
                continue;
            }
            if m.name != state.modules[module].name && !def.public {
                continue;
            }
            out.push(Item { label: def.name.clone(), kind: item_kind(def), detail: Some(def.detail.clone()), docs: None, insert: None });
        }
    }
    dedupe(&mut out);
    Some(out)
}

/// Members of a builtin module: `effect.` and `tag.`. Labels are the tail (`give`), since
/// the client completes the word right after the dot.
fn builtin_members(base: &Option<Ident>) -> Option<Vec<Item>> {
    let head = base.as_ref()?.content.clone();
    if !builtins::is_module(&head) {
        return None;
    }
    let prefix = format!("{head}.");
    Some(
        builtins::BUILTINS
            .iter()
            .filter(|b| b.name.starts_with(&prefix))
            .map(|b| {
                let mut item = builtin_item(b);
                item.label = b.name[prefix.len()..].to_owned();
                item.insert = item.insert.map(|i| i.strip_prefix(&prefix).unwrap_or(i.as_str()).to_owned());
                item
            })
            .collect(),
    )
}

/// Fields and methods after `a.`: resolved through a repaired HIR when possible.
fn member_items(ws: &Workspace, uri: &str, module: usize, offset: usize) -> Option<Vec<Item>> {
    let dot = offset.checked_sub(1)?;
    let hir = ws.hir_with_replacement(uri, dot + 1, offset, "_x_")?;
    let ctx = hir_ctx_at(&hir, module, dot)?;
    let ty = match &ctx.expr.kind {
        HirKind::Field { base, .. } => &base.ty,
        _ => &ctx.expr.ty,
    };
    let names: Vec<String> = hir.structs.iter().map(|s| s.name.clone()).collect();
    let display = |t: &Ty| t.display(&names).to_string();
    let mut out = Vec::new();
    match ty {
        Ty::Struct(id) => {
            let s = hir.structs.get(id.0)?;
            let owner = &s.name;
            for (n, t) in &s.fields {
                out.push(Item { label: n.clone(), kind: 5, detail: Some(format!("field of `{owner}`: `{}`", display(t))), docs: None, insert: None });
            }
        }
        Ty::List(t) => {
            let elem = display(t);
            out.push(method("push", "", &[&elem]));
            out.push(method("len", " -> int", &[]));
        }
        Ty::Str => out.push(method("len", " -> int", &[])),
        _ => return None,
    }
    Some(out)
}

/// Every struct's fields, when nothing better worked.
fn any_fields(state: &ProjectState) -> Option<Vec<Item>> {
    let mut out = Vec::new();
    for m in &state.modules {
        for def in m.defs.by_kind(DefKind::Struct) {
            for (n, t) in &def.fields {
                out.push(Item { label: n.clone(), kind: 5, detail: Some(format!("field of `{}`: `{t}`", def.name)), docs: None, insert: None });
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

fn builtin_item(b: &builtins::Builtin) -> Item {
    let mut placeholders = Vec::new();
    for (i, p) in b.params.iter().enumerate() {
        if i < b.min_args {
            placeholders.push(format!("${{{}:{}}}", i + 1, p.label()));
        }
    }
    let args = placeholders.join(", ");
    let ret = match b.ret {
        builtins::BRet::Unit => "",
        builtins::BRet::Bool => " -> bool",
        builtins::BRet::Entity => " -> entity",
    };
    Item {
        label: b.name.to_owned(),
        kind: 3,
        detail: Some(format!("{}({args}){ret}", b.name)),
        docs: Some(b.docs.to_owned()),
        insert: Some(format!("{}({args})", b.name)),
    }
}

fn method(name: &str, ret: &str, args: &[&str]) -> Item {
    let placeholders = args
        .iter()
        .enumerate()
        .map(|(i, label)| format!("${{{}:{}}}", i + 1, label))
        .collect::<Vec<_>>()
        .join(", ");
    Item {
        label: name.to_owned(),
        kind: 2,
        detail: Some(format!("{name}({}){ret}", args.join(", "))),
        docs: None,
        insert: Some(format!("{name}({placeholders})")),
    }
}

fn item_kind(def: &Def) -> u8 {
    match def.kind {
        DefKind::Fn => 3,
        DefKind::Global => 6,
        DefKind::Const => 21,
        DefKind::Struct => 22,
        DefKind::Objective => 10,
        DefKind::Import => 9,
    }
}

fn module_index(state: &ProjectState, name: &str) -> Option<usize> {
    state.modules.iter().position(|m| m.name == name)
}

/// Every module name of the project, plus modules on disk that nothing imports yet.
fn project_module_names(state: &ProjectState) -> Vec<String> {
    let mut out: Vec<String> = state.modules.iter().map(|m| m.name.clone()).collect();
    out.extend(fs_modules(&state.root));
    out.sort();
    out.dedup();
    out.retain(|n| n != "main");
    out
}

/// All `.slab` files under the project root, as dotted module names.
fn fs_modules(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("slab")
                && let Ok(rel) = path.strip_prefix(root)
            {
                let name = rel.with_extension("").components().filter_map(|c| c.as_os_str().to_str()).collect::<Vec<_>>().join("::");
                if !name.is_empty() {
                    out.push(name);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn dedupe(items: &mut Vec<Item>) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.label.clone()));
}

/// The identifier chain ending at `at`: `util::clamp` -> ["util", "clamp"].
fn chain_before(text: &str, mut at: usize) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(seg) = ident_before(text, at) {
        out.insert(0, seg.content);
        let start = seg.start;
        if start >= 2 && &text[start - 2..start] == "::" {
            at = start - 2;
        } else {
            break;
        }
    }
    out
}

/// An identifier that ends just before `at`.
struct Ident {
    start: usize,
    content: String,
}

fn ident_before(text: &str, at: usize) -> Option<Ident> {
    let bytes = text.as_bytes();
    let mut end = at;
    while end > 0 && (bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    if end == at {
        return None;
    }
    Some(Ident { start: end, content: text[end..at].to_owned() })
}

/// Whether `offset` lies inside a string or f-string token (completion offers nothing there).
fn inside_string(text: &str, offset: usize) -> bool {
    crate::tokens::lex_lenient(text).iter().any(|t| {
        matches!(t.tok, Tok::Str(_) | Tok::FStr(_))
            && (t.span.start as usize..t.span.end as usize).contains(&offset.saturating_sub(1))
    })
}