//! Signature help: while typing the arguments of a call, show the callee's signature with the
//! parameter under the cursor highlighted. Works from the text before the cursor, because a call
//! being typed rarely parses.

use crate::completion::{chain_before, inside_string, module_index, resolve_aliases};
use crate::defs::{Def, DefKind};
use crate::state::{ProjectState, Workspace};

pub struct Signature {
    pub label: String,
    pub docs: Option<String>,
    /// Byte ranges of each parameter inside `label`.
    pub params: Vec<(usize, usize)>,
    pub active: usize,
}

pub fn signature_help(ws: &Workspace, uri: &str, offset: usize) -> Option<Signature> {
    let (state, doc, module) = ws.locate(uri)?;
    let module = module?;
    if inside_string(&doc.text, offset) {
        return None;
    }
    let before = &doc.text[..offset.min(doc.text.len())];
    let (paren, commas) = enclosing_call(before)?;
    let name_end = before[..paren].trim_end().len();
    let chain = chain_before(before, name_end);
    let name = chain.last()?;
    let is_method = before[..name_end - name.len()].ends_with('.');
    let def = if is_method {
        find_method(state, name)?
    } else if chain.len() == 1 {
        find_plain(state, module, name)?
    } else {
        let head = resolve_aliases(state, &doc.text, chain[..chain.len() - 1].to_vec());
        find_path(state, &head, name)?
    };
    let (label, mut params) = split_signature(&def.detail)?;
    // `self` is given by the receiver in `x.method(..)`, so it is not an argument to type
    let mut active = commas;
    if is_method && params.first().is_some_and(|&(a, b)| &label[a..b] == "self") {
        params.remove(0);
    }
    if !params.is_empty() {
        active = active.min(params.len() - 1);
    }
    Some(Signature { label, docs: def.docs.clone(), params, active })
}

/// The `(` of the call the cursor is inside and how many top-level commas lie between it and the cursor.
fn enclosing_call(before: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut commas = 0usize;
    let bytes = before.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' => {
                if depth == 0 {
                    return (bytes[i] == b'(').then_some((i, commas));
                }
                depth -= 1;
            }
            b',' if depth == 0 => commas += 1,
            b';' if depth == 0 => return None,
            b'"' => {
                // skip back over a string literal
                while i > 0 {
                    i -= 1;
                    if bytes[i] == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn defs_named<'a>(state: &'a ProjectState, module: usize, name: &'a str) -> impl Iterator<Item = &'a Def> {
    state.modules.iter().enumerate().flat_map(move |(mi, m)| {
        m.defs.defs().iter().filter(move |d| d.kind == DefKind::Fn && d.name == name && (mi == module || d.public))
    })
}

fn find_plain(state: &ProjectState, module: usize, name: &str) -> Option<Def> {
    let own = state.modules[module].defs.by_name(name).filter(|d| d.kind == DefKind::Fn);
    own.or_else(|| defs_named(state, module, name).next()).cloned()
}

fn find_path(state: &ProjectState, head: &[String], name: &str) -> Option<Def> {
    if let Some(target) = module_index(state, &head.join("::")) {
        return state.modules[target].defs.defs().iter().find(|d| d.kind == DefKind::Fn && d.name == name).cloned();
    }
    // `Type::function`
    let method = format!("{}::{name}", head.last()?);
    state.modules.iter().flat_map(|m| m.defs.defs()).find(|d| d.kind == DefKind::Fn && d.name == method).cloned()
}

/// `x.name(..)`: the receiver's type is not known from the text, so the first method of that name.
fn find_method(state: &ProjectState, name: &str) -> Option<Def> {
    let suffix = format!("::{name}");
    state.modules.iter().flat_map(|m| m.defs.defs()).find(|d| d.kind == DefKind::Fn && d.name.ends_with(&suffix)).cloned()
}

/// Splits `fn name(a: int, b: str) -> int` into the label and the byte range of each parameter.
fn split_signature(detail: &str) -> Option<(String, Vec<(usize, usize)>)> {
    let label = detail.split_once("] ").filter(|_| detail.starts_with("#[")).map_or(detail, |(_, rest)| rest).to_owned();
    let open = label.find('(')?;
    let mut depth = 0usize;
    let mut params = Vec::new();
    let mut start = open + 1;
    for (i, c) in label.char_indices().skip_while(|(i, _)| *i <= open) {
        match c {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' if depth > 0 && !(c == '>' && label[..i].ends_with('-')) => depth -= 1,
            ',' if depth == 0 => {
                params.push((start, i));
                start = i + 1;
            }
            ')' => {
                if !label[start..i].trim().is_empty() {
                    params.push((start, i));
                }
                break;
            }
            _ => {}
        }
    }
    // trim the spaces after commas
    let params = params.into_iter().map(|(a, b)| (a + (label[a..b].len() - label[a..b].trim_start().len()), b)).collect();
    Some((label, params))
}
