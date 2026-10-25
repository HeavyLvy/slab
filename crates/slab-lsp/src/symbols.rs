//! Document symbols (the outline view): the top-level items of a module. A module's
//! [`crate::defs::DefIndex`] has exactly one entry per item, in order, so symbols are a
//! zip of the two.

use slab::syntax::ast::ItemKind;

use crate::defs::DefKind;
use crate::lines::{Enc, Range};
use crate::state::ProjectState;

/// An outline entry (LSP `DocumentSymbol`).
#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    /// LSP `SymbolKind`: 2 module, 7 property, 12 function, 13 variable, 14 constant, 23 struct.
    pub kind: u8,
    pub detail: Option<String>,
    pub range: Range,
    pub selection: Range,
    pub children: Vec<Symbol>,
}

pub fn document_symbols(state: &ProjectState, module: usize, enc: Enc) -> Vec<Symbol> {
    let Some(m) = state.modules.get(module) else { return Vec::new() };
    let text = &state.sources.files[m.file_idx].text;
    let lines = &state.lines[m.file_idx];
    let span_range = |span: slab::diag::Span| lines.range(text, span, enc);
    let mut out = Vec::new();
    for (item, def) in m.program.items.iter().zip(m.defs.defs()) {
        let kind = match def.kind {
            DefKind::Fn => 12,
            DefKind::Global => 13,
            DefKind::Const => 14,
            DefKind::Struct => 23,
            DefKind::Enum => 10,
            DefKind::Impl => 5,
            DefKind::Objective => 7,
            DefKind::Import => 2,
        };
        let mut sym = Symbol {
            name: def.name.clone(),
            kind,
            detail: (!def.detail.is_empty()).then(|| def.detail.clone()),
            range: span_range(item.span),
            selection: span_range(def.name_span),
            children: Vec::new(),
        };
        if let ItemKind::Struct(s) = &item.kind {
            sym.children = s
                .fields
                .iter()
                .zip(&def.fields)
                .map(|(p, (_n, t))| {
                    let r = span_range(p.name.span);
                    Symbol { name: p.name.name.clone(), kind: 7, detail: Some(t.clone()), range: r, selection: r, children: Vec::new() }
                })
                .collect();
        }
        if let ItemKind::Impl(i) = &item.kind {
            sym.children = i
                .methods
                .iter()
                .map(|m| {
                    let range = span_range(m.span);
                    Symbol { name: m.decl.name.name.clone(), kind: 6, detail: None, range, selection: span_range(m.decl.name.span), children: Vec::new() }
                })
                .collect();
        }
        out.push(sym);
    }
    out
}