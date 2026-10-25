//! A per-module index of the items a file declares: their names, spans and one-line details.
//! Features use it for signatures, outline, go-to-definition and any name it cannot resolve
//! through the HIR (which disappears while errors exist). Built straight from the AST.

use slab::diag::Span;
use slab::syntax::ast::{self, ExprKind, ItemKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefKind {
    Fn,
    Global,
    Const,
    Struct,
    Enum,
    /// An `impl` block; the methods are further `Fn` entries named `Type::method`.
    Impl,
    Objective,
    Import,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub name: String,
    pub kind: DefKind,
    /// The declaration itself: the name span for most items.
    pub name_span: Span,
    pub full_span: Span,
    pub public: bool,
    /// `Some("load")` for `#[load]`, `"tick"`, `"every"`.
    pub entry: Option<String>,
    pub detail: String,
    /// The `///` documentation attached to the declaration, when present.
    pub docs: Option<String>,
    /// For structs: field (name, type as written).
    pub fields: Vec<(String, String)>,
    /// For structs: the field name spans, aligned with [`Def::fields`].
    pub field_spans: Vec<Span>,
    /// For enums: variant (name, how it is written: `Some(T)`).
    pub variants: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct DefIndex {
    defs: Vec<Def>,
}

impl DefIndex {
    pub fn build(program: &ast::Program) -> DefIndex {
        let mut defs = Vec::new();
        let mut method_defs = Vec::new();
        for item in &program.items {
            let mut entry = item.attrs.iter().find_map(entry_attr_name);
            let mut def = Def {
                name: String::new(),
                kind: DefKind::Fn,
                name_span: item.span,
                full_span: item.span,
                public: item.public,
                entry: entry.clone(),
                detail: String::new(),
                docs: None,
                fields: Vec::new(),
                field_spans: Vec::new(),
                variants: Vec::new(),
            };
            match &item.kind {
                ItemKind::Fn(f) => {
                    let params = f.params.iter().map(|p| format!("{}: {}", p.name.name, type_string(&p.ty))).collect::<Vec<_>>().join(", ");
                    let ret = f.ret.as_ref().map_or_else(String::new, |t| format!(" -> {}", type_string(t)));
                    def.kind = DefKind::Fn;
                    def.name = f.name.name.clone();
                    def.name_span = f.name.span;
                    let sig = format!("fn {}({params}){ret}", f.name.name);
                    def.detail = match entry.take() {
                        Some(e) => format!("#[{e}] {sig}"),
                        None => sig,
                    };
                }
                ItemKind::Global(v) => {
                    def.kind = DefKind::Global;
                    def.name = v.name.name.clone();
                    def.name_span = v.name.span;
                    def.detail = format!("global {}: {}{}", v.name.name, type_string(&v.ty), literal_value(&v.init));
                }
                ItemKind::Const(v) => {
                    def.kind = DefKind::Const;
                    def.name = v.name.name.clone();
                    def.name_span = v.name.span;
                    def.detail = format!("const {}: {}{}", v.name.name, type_string(&v.ty), literal_value(&v.init));
                }
                ItemKind::Struct(s) => {
                    def.kind = DefKind::Struct;
                    def.name = s.name.name.clone();
                    def.name_span = s.name.span;
                    def.detail = format!(
                        "struct {} {{ {} }}",
                        s.name.name,
                        s.fields.iter().map(|p| format!("{}: {}", p.name.name, type_string(&p.ty))).collect::<Vec<_>>().join(", ")
                    );
                    def.fields = s.fields.iter().map(|p| (p.name.name.clone(), type_string(&p.ty))).collect();
                    def.field_spans = s.fields.iter().map(|p| p.name.span).collect();
                }
                ItemKind::Enum(e) => {
                    def.kind = DefKind::Enum;
                    def.name = e.name.name.clone();
                    def.name_span = e.name.span;
                    let generics = if e.generics.is_empty() { String::new() } else { format!("<{}>", e.generics.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", ")) };
                    let variants: Vec<String> = e
                        .variants
                        .iter()
                        .map(|v| if v.fields.is_empty() { v.name.name.clone() } else { format!("{}({})", v.name.name, v.fields.iter().map(type_string).collect::<Vec<_>>().join(", ")) })
                        .collect();
                    def.detail = format!("enum {}{generics} {{ {} }}", e.name.name, variants.join(", "));
                    def.variants = e.variants.iter().zip(variants).map(|(v, written)| (v.name.name.clone(), written)).collect();
                }
                ItemKind::Impl(i) => {
                    def.kind = DefKind::Impl;
                    def.name = format!("impl {}", type_string(&i.target));
                    def.name_span = i.target.span;
                    let generics = if i.generics.is_empty() { String::new() } else { format!("<{}>", i.generics.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", ")) };
                    def.detail = format!("impl{generics} {}", type_string(&i.target));
                    for m in &i.methods {
                        method_defs.push(method_def(&i.target.name.name, m));
                    }
                }
                ItemKind::Objective { name, criteria } => {
                    def.kind = DefKind::Objective;
                    def.name = name.name.clone();
                    def.name_span = name.span;
                    def.detail = format!("objective {} = {}", name.name, expr_string(criteria));
                }
                ItemKind::Import(i) => {
                    def.kind = DefKind::Import;
                    let name_span = match (&i.alias, i.path.last()) {
                        (Some(a), _) => a.span,
                        (None, Some(last)) => last.span,
                        (None, None) => item.span,
                    };
                    def.name = i.alias.as_ref().map_or_else(
                        || i.path.last().map(|p| p.name.clone()).unwrap_or_default(),
                        |a| a.name.clone(),
                    );
                    def.name_span = name_span;
                    let path = i.path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("::");
                    def.detail = format!("import {path}");
                }
            }
            defs.push(def);
        }
        // the methods of every `impl`, after the one-per-item entries
        defs.extend(method_defs);
        DefIndex { defs }
    }

    pub fn defs(&self) -> &[Def] {
        &self.defs
    }

    pub fn by_name(&self, name: &str) -> Option<&Def> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn by_kind(&self, kind: DefKind) -> impl Iterator<Item = &Def> {
        self.defs.iter().filter(move |d| d.kind == kind)
    }

    /// Attaches the `///` documentation above every declaration in this source text.
    pub fn attach_docs(&mut self, text: &str) {
        for def in &mut self.defs {
            def.docs = crate::docs::doc_before(text, def.name_span.start as usize);
        }
    }
}

/// A method as a definition named `Type::method`.
fn method_def(type_name: &str, m: &ast::ImplFn) -> Def {
    let f = &m.decl;
    let params = f
        .params
        .iter()
        .map(|p| if f.has_self && p.name.name == "self" { "self".to_owned() } else { format!("{}: {}", p.name.name, type_string(&p.ty)) })
        .collect::<Vec<_>>()
        .join(", ");
    let ret = f.ret.as_ref().map_or_else(String::new, |t| format!(" -> {}", type_string(t)));
    Def {
        name: format!("{type_name}::{}", f.name.name),
        kind: DefKind::Fn,
        name_span: f.name.span,
        full_span: m.span,
        public: m.public,
        entry: None,
        detail: format!("fn {}({params}){ret}", f.name.name),
        docs: None,
        fields: Vec::new(),
        field_spans: Vec::new(),
        variants: Vec::new(),
    }
}

/// `#[load]` / `#[tick]` / `#[every(1s)]` -> `load` / `tick` / `every`.
fn entry_attr_name(attr: &ast::Attr) -> Option<String> {
    match attr.name.name.as_str() {
        "load" | "tick" | "every" => Some(attr.name.name.clone()),
        _ => None,
    }
}

/// A type annotation as written: `int`, `list<int>`, `util::Point`.
pub fn type_string(t: &ast::Type) -> String {
    let name = t.name.name.as_str();
    let name = t.module.as_ref().map_or_else(|| name.to_owned(), |m| format!("{}::{name}", m.name));
    let name = if t.constant { format!("const {name}") } else { name };
    if t.args.is_empty() {
        name
    } else {
        format!("{}<{}>", name, t.args.iter().map(type_string).collect::<Vec<_>>().join(", "))
    }
}

/// A short text form of an expression for details (`"dummy"`, `1`, ...) — literals only.
/// ` = value` for a literal initialiser, nothing for anything else.
fn literal_value(e: &ast::Expr) -> String {
    match &e.kind {
        ExprKind::Str(_) | ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) => format!(" = {}", expr_string(e)),
        _ => String::new(),
    }
}

fn expr_string(e: &ast::Expr) -> String {
    match &e.kind {
        ExprKind::Str(s) => format!("\"{s}\""),
        ExprKind::Int(n) => n.to_string(),
        ExprKind::Float(t) => t.clone(),
        ExprKind::Bool(b) => b.to_string(),
        _ => "?".to_owned(),
    }
}