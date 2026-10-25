//! Completion and hover inside a selector's brackets: `@e[type=zombie, distance=..10]`.
//! Works on text, because a selector being typed (`@e[dis`) is not a complete token yet.

use crate::help::{self, SELECTOR_KEYS};
use crate::resolve::Item;
use crate::state::ProjectState;
use crate::defs::DefKind;

/// Where in a selector the cursor is.
#[derive(Debug, PartialEq, Eq)]
pub enum Ctx {
    /// Typing an option name; `taken` are the options already written in this selector.
    Key { taken: Vec<String> },
    /// Typing the value of the named option.
    Value { key: String },
    /// Typing an objective name inside `scores={ }`.
    Objective,
    /// Typing a score range inside `scores={name= }`.
    ScoreRange,
}

/// The selector the cursor is inside of, if any.
pub fn context(text: &str, offset: usize) -> Option<Ctx> {
    let before = text.get(..offset)?;
    let bytes = before.as_bytes();
    // the last `@x[` before the cursor
    let open = (0..bytes.len().saturating_sub(2)).rev().find(|&i| bytes[i] == b'@' && b"aeprsn".contains(&bytes[i + 1]) && bytes[i + 2] == b'[')? + 2;
    // walk to the cursor: the selector must still be open, and we need the pieces of it
    let mut stack: Vec<usize> = vec![open]; // positions of the open brackets
    let mut seg_start: Vec<usize> = vec![open + 1]; // start of the current piece at each depth
    let mut taken = Vec::new();
    let mut in_str = false;
    let mut i = open + 1;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'[' | b'{' | b'(' => {
                    stack.push(i);
                    seg_start.push(i + 1);
                }
                b']' | b'}' | b')' => {
                    stack.pop();
                    seg_start.pop();
                    if stack.is_empty() {
                        return None;
                    }
                }
                b',' => {
                    if stack.len() == 1 {
                        let piece = &before[*seg_start.last()?..i];
                        if let Some((k, _)) = piece.split_once('=') {
                            taken.push(k.trim().to_owned());
                        }
                    }
                    *seg_start.last_mut()? = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    if in_str {
        return None;
    }
    let piece = &before[*seg_start.last()?..];
    match stack.len() {
        1 => Some(match piece.split_once('=') {
            Some((k, _)) => Ctx::Value { key: k.trim().to_owned() },
            None => Ctx::Key { taken },
        }),
        2 if bytes[stack[1]] == b'{' && enclosing_key(before, stack[1]).as_deref() == Some("scores") => {
            Some(if piece.contains('=') { Ctx::ScoreRange } else { Ctx::Objective })
        }
        _ => None,
    }
}

/// The option name in front of the `{` at `brace`: `scores={` -> `scores`.
fn enclosing_key(before: &str, brace: usize) -> Option<String> {
    let head = before[..brace].trim_end().strip_suffix('=')?.trim_end();
    let start = head.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).map_or(0, |p| p + 1);
    Some(head[start..].to_owned())
}

/// Completion items for the cursor, `None` when it is not inside a selector.
pub fn completions(state: &ProjectState, module: usize, text: &str, offset: usize) -> Option<Vec<Item>> {
    let ctx = context(text, offset)?;
    let mut out = Vec::new();
    match ctx {
        Ctx::Key { taken } => {
            for k in SELECTOR_KEYS.iter().filter(|k| k.repeatable || !taken.iter().any(|t| t == k.name)) {
                out.push(Item { label: k.name.to_owned(), kind: 10, detail: Some(format!("{}={}", k.name, k.value)), docs: Some(k.doc.to_owned()), insert: Some(k.snippet.to_owned()) });
            }
        }
        Ctx::Value { key } => {
            let choices: &[&str] = match key.as_str() {
                "sort" => &["nearest", "furthest", "random", "arbitrary"],
                "gamemode" => &["survival", "creative", "adventure", "spectator"],
                _ => &[],
            };
            for c in choices {
                out.push(Item { label: (*c).to_owned(), kind: 20, detail: Some(format!("{key}={c}")), docs: None, insert: None });
            }
        }
        Ctx::Objective => {
            for (mi, m) in state.modules.iter().enumerate() {
                for d in m.defs.defs().iter().filter(|d| d.kind == DefKind::Objective && (mi == module || d.public)) {
                    out.push(Item { label: d.name.clone(), kind: 6, detail: Some(d.detail.clone()), docs: d.docs.clone(), insert: Some(format!("{}=${{1:1..}}", d.name)) });
                }
            }
        }
        Ctx::ScoreRange => {}
    }
    Some(out)
}

/// Hover text for the option name or value at `offset`, given the selector's bracket contents
/// (`args`) and where they start in the file.
pub fn key_hover(args: &str, args_start: usize, offset: usize) -> Option<String> {
    let rel = offset.checked_sub(args_start)?;
    if rel > args.len() {
        return None;
    }
    // the piece between top-level commas that holds the cursor
    let (mut depth, mut start, mut in_str) = (0i32, 0usize, false);
    let mut piece = None;
    for (i, c) in args.char_indices().chain(std::iter::once((args.len(), ','))) {
        match c {
            '"' => in_str = !in_str,
            '[' | '{' | '(' if !in_str => depth += 1,
            ']' | '}' | ')' if !in_str => depth -= 1,
            ',' if depth == 0 && !in_str => {
                if (start..=i).contains(&rel) {
                    piece = Some(&args[start..i]);
                    break;
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let key = piece?.split_once('=')?.0.trim();
    let k = help::selector_key(key)?;
    Some(format!("```slab\n{}={}\n```\n{}", k.name, k.value, k.doc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Option<Ctx> {
        context(text, text.len())
    }

    #[test]
    fn finds_where_in_a_selector_the_cursor_is() {
        assert_eq!(at("let a = @e[ty"), Some(Ctx::Key { taken: vec![] }));
        assert_eq!(at("@e[type=zombie, dis"), Some(Ctx::Key { taken: vec!["type".into()] }));
        assert_eq!(at("@a[sort="), Some(Ctx::Value { key: "sort".into() }));
        assert_eq!(at("@a[tag=\"a,b\", limit="), Some(Ctx::Value { key: "limit".into() }));
        assert_eq!(at("@e[scores={kil"), Some(Ctx::Objective));
        assert_eq!(at("@e[scores={kills=1.., de"), Some(Ctx::Objective));
        assert_eq!(at("@e[scores={kills="), Some(Ctx::ScoreRange));
        assert_eq!(at("@e[type=zombie] "), None, "closed");
        assert_eq!(at("let a = [1, "), None);
    }

    #[test]
    fn hovers_the_option_under_the_cursor() {
        let args = "type=zombie, distance=..10";
        let h = key_hover(args, 100, 100 + args.find("distance").unwrap() + 2).unwrap();
        assert!(h.contains("Distance from where"), "{h}");
        assert!(key_hover(args, 100, 101).unwrap().contains("Only entities of this type"));
        assert!(key_hover("nbt={}", 0, 1).is_none());
    }
}
