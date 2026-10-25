//! Name resolution for language-server features: where is the identifier under the cursor
//! defined and what do we know about it. Works against the partial HIR (with `.get()`
//! everywhere) and falls back to AST-level name matching when the HIR is gone, which happens
//! between keystrokes.

use slab::diag::Span;
use slab::sema::hir::{self as shir, ExprKind as HirKind, MethodKind};
use slab::sema::ty::Ty;
use slab::syntax::ast;

use crate::analysis::{self, Hit, Role, hir_ctx_at};
use crate::defs::{Def, DefKind};
use crate::state::ProjectState;

/// Where a name was declared.
#[derive(Clone, Copy, Debug)]
pub struct Definition {
    pub file_idx: usize,
    pub span: Span,
}

/// Completion candidates assembled for one cursor position.
#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    /// LSP `CompletionItemKind`: 2 method, 3 function, 5 field, 6 variable, 9 module,
    /// 14 keyword, 21 constant, 22 struct.
    pub kind: u8,
    pub detail: Option<String>,
    pub docs: Option<String>,
    /// Snippet insert text (like `tp(${1:selector}, ...)`) when it differs from the label.
    pub insert: Option<String>,
}

impl ProjectState {
    // ------------------------------------------------------------------ go to definition

    pub fn goto_definition(&self, module: usize, offset: usize) -> Option<Definition> {
        let program = &self.modules.get(module)?.program;
        let hit = analysis::hit_at(program, offset)?;
        let own_file = self.modules[module].file_idx;
        match hit.role {
            // Declarations and binding sites point at themselves.
            Role::StructFieldName | Role::FieldInitName | Role::Param | Role::LetName => {
                Some(Definition { file_idx: own_file, span: hit.span })
            }
            Role::Decl(DefKind::Import) | Role::ModuleName | Role::ImportModule => self.module_def(program, &hit),
            Role::Decl(_) => Some(Definition { file_idx: own_file, span: hit.span }),
            Role::AttrName | Role::MethodName => None,
            Role::ImportItem => self.imported_item_def(program, &hit),
            Role::Callee | Role::PathItem | Role::Plain => self.value_def(module, offset, &hit),
            Role::FieldName => self.field_def(module, offset, &hit),
            Role::TypeName => {
                let (mi, d) = self.struct_by_name(module, program, &hit.name)?;
                Some(Definition { file_idx: self.modules[mi].file_idx, span: d.name_span })
            }
        }
    }

    /// Where the value under the cursor is declared: HIR first, name matching second.
    fn value_def(&self, module: usize, offset: usize, hit: &Hit) -> Option<Definition> {
        if let Some(hir) = &self.hir
            && let Some(ctx) = hir_ctx_at(hir, module, offset)
            && !matches!(ctx.expr.kind, HirKind::Error)
            && let Some(d) = self.hir_def(hir, module, &ctx)
        {
            return Some(d);
        }
        self.name_value_def(module, offset, hit)
    }

    /// A definition derived from the HIR node at the cursor.
    fn hir_def(&self, hir: &shir::Hir, module: usize, ctx: &analysis::HirCtx) -> Option<Definition> {
        let def_of = |name: &str, module: usize| {
            let mi = self.module_by_name(&hir.modules[module])?;
            self.modules.get(mi)?.defs.by_name(name).map(|d| Definition { file_idx: self.modules[mi].file_idx, span: d.name_span })
        };
        match &ctx.expr.kind {
            HirKind::Call { func, .. } => {
                let f = hir.fns.get(func.0)?;
                def_of(&f.name, f.module)
            }
            HirKind::Builtin { name, .. } => {
                let (mi, def) = self.intrinsic_def(name)?;
                Some(Definition { file_idx: self.modules[mi].file_idx, span: def.name_span })
            }
            HirKind::Variant { enum_id, .. } => {
                let e = hir.enums.get(enum_id.0)?;
                def_of(&e.name, e.module)
            }
            HirKind::Global(id) => {
                let g = hir.globals.get(id.0)?;
                def_of(&g.name, g.module)
            }
            HirKind::Score { objective, .. } => {
                let o = hir.objectives.get(objective.0)?;
                def_of(&o.name, o.module)
            }
            HirKind::Local(id) => {
                let f = hir.fns.get(ctx.fn_idx)?;
                let l = f.locals.get(id.0)?;
                Some(Definition { file_idx: self.modules[module].file_idx, span: l.span })
            }
            HirKind::Struct { id, .. } => {
                let s = hir.structs.get(id.0)?;
                def_of(&s.name, s.module)
            }
            HirKind::Field { base, index, .. } => {
                if *index == usize::MAX {
                    return None;
                }
                let Ty::Struct(sid) = &base.ty else { return None };
                let s = hir.structs.get(sid.0)?;
                let mi = self.module_by_name(&hir.modules[s.module])?;
                let def = self.modules[mi].defs.by_name(&s.name)?;
                let span = def.field_spans.get(*index).copied()?;
                Some(Definition { file_idx: self.modules[mi].file_idx, span })
            }
            _ => None,
        }
    }

    /// A field access definition without the HIR: search the module's own structs for a
    /// declaration whose field name has exactly this span.
    fn field_def(&self, module: usize, offset: usize, hit: &Hit) -> Option<Definition> {
        if let Some(hir) = &self.hir
            && let Some(ctx) = hir_ctx_at(hir, module, offset)
            && let Some(d) = self.hir_def(hir, module, &ctx)
        {
            return Some(d);
        }
        self.field_decl_in_own_module(module, &hit.name, hit.span)
            .map(|span| Definition { file_idx: self.modules[module].file_idx, span })
    }

    /// A struct field declaration by exact name span (the use has no HIR left).
    pub fn field_decl_in_own_module(&self, module: usize, name: &str, span: Span) -> Option<Span> {
        for def in self.modules[module].defs.defs().iter().filter(|d| d.kind == DefKind::Struct) {
            for (i, s) in def.field_spans.iter().enumerate() {
                if s.start == span.start && s.end == span.end && def.fields.get(i).map(|(n, _)| n.as_str()) == Some(name) {
                    return Some(*s);
                }
            }
        }
        None
    }

    /// Name-based resolution when the HIR is unavailable (or an error poisoned the node).
    fn name_value_def(&self, module: usize, offset: usize, hit: &Hit) -> Option<Definition> {
        let program = &self.modules.get(module)?.program;
        let name = hit.name.as_str();
        let own_file = self.modules[module].file_idx;

        // Locals/parameters declared in the function containing the cursor, before it.
        if let Some(span) = local_decls(program, &hit.name, offset).into_iter().next() {
            return Some(Definition { file_idx: own_file, span });
        }
        // Own module items.
        if let Some(d) = self.modules[module].defs.by_name(name)
            && matches!(d.kind, DefKind::Fn | DefKind::Global | DefKind::Const | DefKind::Objective)
        {
            return Some(Definition { file_idx: own_file, span: d.name_span });
        }
        // Imported items, module names, struct types.
        self.imported_item_def(program, hit)
            .or_else(|| self.module_def(program, hit))
            .or_else(|| {
                self.struct_by_name(module, program, name)
                    .map(|(mi, d)| Definition { file_idx: self.modules[mi].file_idx, span: d.name_span })
            })
    }

    /// The module a name in this file stands for: an import whose alias (or last path segment)
    /// is `hit.name`, a submodule brought in by a glob (`import game::*;` -> `tag`), or a full
    /// path such as `game::tag` (the standard library needs no import).
    fn import_target(&self, program: &ast::Program, hit: &Hit) -> Option<usize> {
        for item in &program.items {
            let ast::ItemKind::Import(imp) = &item.kind else { continue };
            let segs: Vec<&str> = imp.path.iter().map(|p| p.name.as_str()).collect();
            if imp.glob {
                if let Some(i) = self.module_by_name(&format!("{}::{}", segs.join("::"), hit.name)) {
                    return Some(i);
                }
                continue;
            }
            if imp.items.is_some() {
                continue;
            }
            let alias = imp.alias.as_ref().map(|a| a.name.as_str()).or_else(|| segs.last().copied());
            if alias == Some(hit.name.as_str()) {
                return self.module_by_name(&segs.join("::"));
            }
        }
        self.module_by_name(&hit.name).filter(|&i| slab::stdlib::is_std_module(&self.modules[i].name))
    }

    /// The module and declaration of the standard-library function the compiler implements
    /// under this intrinsic id (`#[intrinsic("tag.add")]`).
    fn intrinsic_def(&self, id: &str) -> Option<(usize, Def)> {
        for (mi, m) in self.modules.iter().enumerate() {
            for item in &m.program.items {
                let ast::ItemKind::Fn(f) = &item.kind else { continue };
                let is_it = item.attrs.iter().any(|a| a.name.name == "intrinsic" && matches!(a.args.first().map(|e| &e.kind), Some(ast::ExprKind::Str(s)) if s == id));
                if is_it && let Some(def) = m.defs.by_name(&f.name.name) {
                    return Some((mi, def.clone()));
                }
            }
        }
        None
    }

    fn intrinsic_hover(&self, id: &str) -> Option<String> {
        let (mi, def) = self.intrinsic_def(id)?;
        Some(def_hover(&def, &self.modules[mi].name, DefKind::Fn))
    }

    /// `util` (of `import util;`) -> the module's file.
    fn module_def(&self, program: &ast::Program, hit: &Hit) -> Option<Definition> {
        let target = self.import_target(program, hit)?;
        Some(Definition { file_idx: self.modules[target].file_idx, span: Span::new(0, 0) })
    }

    /// A name imported as `import a::{x}`: the item's definition in the target module.
    fn imported_item_def(&self, program: &ast::Program, hit: &Hit) -> Option<Definition> {
        for item in &program.items {
            let ast::ItemKind::Import(imp) = &item.kind else { continue };
            let Some(items) = &imp.items else { continue };
            if !items.iter().any(|i| i.name == hit.name) {
                continue;
            }
            let segs: Vec<&str> = imp.path.iter().map(|p| p.name.as_str()).collect();
            let target = self.modules.iter().position(|m| m.name == segs.join("::"))?;
            let def = self.modules[target].defs.by_name(&hit.name)?;
            return Some(Definition { file_idx: self.modules[target].file_idx, span: def.name_span });
        }
        self.import_target(program, hit)
            .and_then(|target| {
                self.modules[target]
                    .defs
                    .by_name(&hit.name)
                    .cloned()
                    .map(|d| Definition { file_idx: self.modules[target].file_idx, span: d.name_span })
            })
    }

    /// A struct by simple name: own module first, then through this module's imports.
    pub fn struct_by_name(&self, module: usize, program: &ast::Program, name: &str) -> Option<(usize, Def)> {
        if let Some(d) = self.modules.get(module).and_then(|m| m.defs.by_name(name))
            && matches!(d.kind, DefKind::Struct | DefKind::Enum)
        {
            return Some((module, d.clone()));
        }
        for item in &program.items {
            let ast::ItemKind::Import(imp) = &item.kind else { continue };
            let segs: Vec<&str> = imp.path.iter().map(|p| p.name.as_str()).collect();
            if let Some(target) = self.modules.iter().position(|m| m.name == segs.join("::"))
                && let Some(d) = self.modules[target].defs.by_name(name)
                && matches!(d.kind, DefKind::Struct | DefKind::Enum)
                && d.public
            {
                return Some((target, d.clone()));
            }
        }
        // `Option` and `Result` are in scope everywhere
        if matches!(name, "Option" | "Result")
            && let Some(std) = self.module_by_name("std")
            && let Some(d) = self.modules[std].defs.by_name(name)
        {
            return Some((std, d.clone()));
        }
        None
    }

    fn module_by_name(&self, name: &str) -> Option<usize> {
        self.modules.iter().position(|m| m.name == name)
    }

    // ------------------------------------------------------------------ hover

    /// Markdown hover text at a position.
    pub fn hover(&self, module: usize, offset: usize) -> Option<String> {
        let program = &self.modules.get(module)?.program;
        let hit = analysis::hit_at(program, offset)?;
        let module_name = &self.modules[module].name;
        match hit.role {
            Role::AttrName => None,
            Role::Decl(kind) => {
                let def = self.modules[module].defs.by_name(&hit.name)?.clone();
                Some(def_hover(&def, module_name, kind))
            }
            Role::StructFieldName => {
                let span = self.field_decl_in_own_module(module, &hit.name, hit.span).unwrap_or(hit.span);
                let def = self.modules[module].defs.defs().iter().find(|d| d.field_spans.contains(&span))?;
                let i = def.field_spans.iter().position(|s| *s == span)?;
                let (n, t) = &def.fields[i];
                let doc = self.docs_of(module, def.field_spans[i]);
                Some(match doc {
                    Some(d) => format!("```slab\n{n}: {t}\n```\nfield of `struct {}`\n\n{d}", def.name),
                    None => format!("```slab\n{n}: {t}\n```\nfield of `struct {}`", def.name),
                })
            }
            Role::FieldInitName => None,
            Role::Param => {
                let sig = format!("{}: {}", hit.name, param_type(program, offset)?);
                let owner = self.modules[module].defs.defs().iter().find(|d| d.kind == DefKind::Fn && analysis::contains(d.full_span, offset))?;
                let doc = self.docs_of(module, hit.span);
                Some(match doc {
                    Some(d) => format!("```slab\n{sig}\n```\na parameter of `{}`\n\n{d}", owner.detail),
                    None => format!("```slab\n{sig}\n```\na parameter of `{}`", owner.detail),
                })
            }
            Role::LetName => {
                if let Some(hir) = &self.hir
                    && let Some((fi, li)) = crate::analysis::hir_local_at(hir, module, offset)
                {
                    let l = hir.fns.get(fi)?.locals.get(li)?;
                    let mutable = if l.mutable { "mut " } else { "" };
                    let doc = self.docs_of(module, l.span);
                    return Some(match doc {
                        Some(d) => format!("```slab\nlet {mutable}{}: {}\n```\na local variable\n\n{d}", l.name, analysis::ty_string(&l.ty, hir)),
                        None => format!("```slab\nlet {mutable}{}: {}\n```\na local variable", l.name, analysis::ty_string(&l.ty, hir)),
                    });
                }
                Some(format!("a local variable `{}`", hit.name))
            }
            Role::Callee | Role::PathItem | Role::Plain => self.value_hover(module, offset, &hit),
            Role::MethodName => {
                let hir = self.hir.as_ref()?;
                let ctx = hir_ctx_at(hir, module, offset)?;
                match &ctx.expr.kind {
                    HirKind::Method { kind, .. } => Some(method_text(*kind)),
                    HirKind::Builtin { name, .. } => self.intrinsic_hover(name),
                    _ => None,
                }
            }
            Role::FieldName => {
                let hir = self.hir.as_ref()?;
                let ctx = hir_ctx_at(hir, module, offset)?;
                let HirKind::Field { index, .. } = &ctx.expr.kind else { return None };
                let (mi, def, i) = self.field_decl(hir, &ctx, *index)?;
                let (n, t) = &def.fields[i];
                let _ = mi;
                let doc = self.docs_of(mi, *def.field_spans.get(i)?);
                Some(match doc {
                    Some(d) => format!("```slab\n{n}: {t}\n```\nfield of `struct {}`\n\n{d}", def.name),
                    None => format!("```slab\n{n}: {t}\n```\nfield of `struct {}`", def.name),
                })
            }
            Role::TypeName => {
                let (mi, def) = self.struct_by_name(module, program, &hit.name)?;
                Some(def_hover(&def, &self.modules[mi].name, DefKind::Struct))
            }
            Role::ModuleName | Role::ImportModule => {
                let target = self.import_target(program, &hit)?;
                let text = self.module_text(self.modules[target].file_idx);
                Some(module_hover(&self.modules[target], text))
            }
            Role::ImportItem => {
                let d = self.imported_item_def(program, &hit)?;
                let mi = self.module_of_file(d.file_idx)?;
                let def = self.modules[mi].defs.defs().iter().find(|x| x.name_span == d.span)?.clone();
                Some(def_hover(&def, &self.modules[mi].name, def.kind))
            }
        }
    }

    /// Hover for a value-like use.
    fn value_hover(&self, module: usize, offset: usize, hit: &Hit) -> Option<String> {
        if let Some(hir) = &self.hir
            && let Some(ctx) = hir_ctx_at(hir, module, offset)
            && let Some(text) = self.hir_hover_text(hir, module, &ctx)
        {
            return Some(text);
        }
        self.modules.get(module)?;
        self.name_value_def(module, offset, hit)
            .and_then(|d| {
                let mi = self.module_of_file(d.file_idx)?;
                self.modules[mi].defs.defs().iter().find(|x| x.name_span == d.span).cloned().map(|def| (mi, def))
            })
            .map(|(mi, def)| def_hover(&def, &self.modules[mi].name, def.kind))
    }

    fn hir_hover_text(&self, hir: &shir::Hir, module: usize, ctx: &analysis::HirCtx) -> Option<String> {
        let ty = |t: &Ty| analysis::ty_string(t, hir);
        match &ctx.expr.kind {
            HirKind::Call { func, .. } => {
                let f = hir.fns.get(func.0)?;
                let sig = fn_signature(f, hir);
                let mod_name = hir.modules.get(f.module)?.clone();
                let entry = f.entry.as_ref().map(entry_label).unwrap_or_default();
                let entry = if entry.is_empty() { String::new() } else { format!("\nin module `{mod_name}`  —  entry point `#[{entry}]`") };
                let docs = named_def_docs(self, hir, f.module, &f.name).map_or_else(String::new, |d| format!("\n\n{d}"));
                Some(if entry.is_empty() { format!("```slab\n{sig}\n```\nin module `{mod_name}`{docs}") } else { format!("```slab\n{sig}\n```{entry}{docs}") })
            }
            HirKind::Builtin { name, .. } => self.intrinsic_hover(name),
            HirKind::Variant { enum_id, index, .. } => {
                let e = hir.enums.get(enum_id.0)?;
                let v = e.variants.get(*index)?;
                let generics = if e.generics.is_empty() { String::new() } else { format!("<{}>", e.generics.join(", ")) };
                let payload = if v.fields.is_empty() { String::new() } else { format!("({})", v.fields.iter().map(|t| payload_string(t, e, hir)).collect::<Vec<_>>().join(", ")) };
                let docs = named_def_docs(self, hir, e.module, &e.name).map_or_else(String::new, |d| format!("\n\n{d}"));
                Some(format!("```slab\n{}::{}{payload}\n```\nvariant {} of `enum {}{generics}`, in module `{}`{docs}", e.name, v.name, index, e.name, hir.modules.get(e.module)?))
            }
            HirKind::Method { kind, .. } => Some(method_text(*kind)),
            HirKind::Local(id) => {
                let f = hir.fns.get(ctx.fn_idx)?;
                let l = f.locals.get(id.0)?;
                let kind = if f.params.contains(id) {
                    "a parameter"
                } else if l.mutable {
                    "a mutable local"
                } else {
                    "a local variable"
                };
                let mutable = if l.mutable { "mut " } else { "" };
                let docs = self.docs_of(module, l.span).map_or_else(String::new, |d| format!("\n\n{d}"));
                Some(format!("```slab\nlet {mutable}{}: {}\n```\n{kind}{docs}", l.name, ty(&l.ty)))
            }
            HirKind::Global(id) => {
                let g = hir.globals.get(id.0)?;
                let docs = named_def_docs(self, hir, g.module, &g.name).map_or_else(String::new, |d| format!("\n\n{d}"));
                Some(format!("```slab\nglobal {}: {}\n```\na global (persists across ticks and `/reload`){docs}", g.name, ty(&g.ty)))
            }
            HirKind::Score { objective, .. } => {
                let o = hir.objectives.get(objective.0)?;
                let docs = named_def_docs(self, hir, o.module, &o.name).map_or_else(String::new, |d| format!("\n\n{d}"));
                Some(format!("```slab\nobjective {}\n```\na scoreboard score; read or write with `{}[holder]`{docs}", o.name, o.name))
            }
            HirKind::Selector(..) => {
                let raw = self.expr_text(module, &ctx.expr);
                Some(format!("```slab\n{raw}\n```\n{} — usable where a selector is expected", ty(&ctx.expr.ty)))
            }
            HirKind::Field { index, .. } => {
                let (mi, def, i) = self.field_decl(hir, ctx, *index)?;
                let (n, t) = &def.fields[i];
                let _ = mi;
                Some(format!("```slab\n{n}: {t}\n```\nfield of `struct {}`", def.name))
            }
            HirKind::Error => None,
            _ => {
                if ctx.expr.ty.is_error() {
                    return None;
                }
                Some(format!("`{}`", analysis::ty_string(&ctx.expr.ty, hir)))
            }
        }
    }

    /// (module with the struct, its Def, field index) for a field expression in the HIR.
    fn field_decl(&self, hir: &shir::Hir, ctx: &analysis::HirCtx, index: usize) -> Option<(usize, &Def, usize)> {
        if index == usize::MAX {
            return None;
        }
        let HirKind::Field { base, .. } = &ctx.expr.kind else { return None };
        let Ty::Struct(sid) = &base.ty else { return None };
        let s = hir.structs.get(sid.0)?;
        let mi = self.module_by_name(&hir.modules[s.module])?;
        let def = self.modules[mi].defs.by_name(&s.name)?;
        Some((mi, def, index))
    }

    /// The source text of an HIR expression (best effort).
    pub fn expr_text(&self, module: usize, expr: &shir::Expr) -> String {
        let file = self.modules[module].file_idx;
        self.sources
            .files
            .get(file)
            .and_then(|f| f.text.get(expr.span.start as usize..expr.span.end as usize))
            .unwrap_or("")
            .to_owned()
    }

    /// The `///` documentation above a span in a module's source.
    fn docs_of(&self, module: usize, span: slab::diag::Span) -> Option<String> {
        let text = self.module_text(self.modules[module].file_idx);
        crate::docs::doc_before(text, span.start as usize)
    }

    /// The source text of a module's file.
    fn module_text(&self, file_idx: usize) -> &str {
        &self.sources.files[file_idx].text
    }
}

/// A declaration's documentation resolved through a module's own index.
fn named_def_docs<'a>(state: &'a ProjectState, hir: &shir::Hir, module: usize, name: &str) -> Option<&'a str> {
    let mi = state.module_by_name(&hir.modules[module])?;
    state.modules[mi].defs.by_name(name)?.docs.as_deref()
}

// ------------------------------------------------------------------ helpers

/// (module with the struct, its Def, field index) for a field expression in the HIR.
fn fn_signature(f: &shir::FnDef, hir: &shir::Hir) -> String {
    let params = f
        .params
        .iter()
        .map(|p| {
            let l = &f.locals[p.0];
            format!("{}: {}", l.name, analysis::ty_string(&l.ty, hir))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ret = if matches!(f.ret, Ty::Unit) { String::new() } else { format!(" -> {}", analysis::ty_string(&f.ret, hir)) };
    format!("fn {}({params}){ret}", f.name)
}

/// A payload type of an enum's variant, with the enum's own type parameters by name.
fn payload_string(ty: &Ty, e: &shir::EnumDef, hir: &shir::Hir) -> String {
    match ty {
        Ty::Param(i) => e.generics.get(*i).cloned().unwrap_or_else(|| "?".to_owned()),
        Ty::List(t) => format!("list<{}>", payload_string(t, e, hir)),
        Ty::Enum(id, args) => {
            let name = hir.enums.get(id.0).map_or("?", |x| x.name.as_str());
            if args.is_empty() { name.to_owned() } else { format!("{name}<{}>", args.iter().map(|a| payload_string(a, e, hir)).collect::<Vec<_>>().join(", ")) }
        }
        other => analysis::ty_string(other, hir),
    }
}

fn method_text(kind: MethodKind) -> String {
    match kind {
        MethodKind::ListPush => "```slab\nxs.push(value)\n```\nAdd a value to the end of a list.".to_owned(),
        MethodKind::ListLen => "```slab\nlen() -> int\n```\nThe number of elements in the list.".to_owned(),
        MethodKind::StrLen => "```slab\nlen() -> int\n```\nThe number of characters in the string.".to_owned(),
    }
}

fn def_hover(def: &Def, module_name: &str, kind: DefKind) -> String {
    let scope = match kind {
        DefKind::Import => "module".to_owned(),
        DefKind::Objective => "objective".to_owned(),
        _ => module_name.to_owned(),
    };
    let vis = if def.public { " `pub`" } else { "" };
    let mut out = format!("```slab\n{}\n```{vis}\nin `{scope}`", def.detail);
    if let Some(docs) = &def.docs {
        out.push_str("\n\n");
        out.push_str(docs);
    }
    out
}

fn module_hover(m: &crate::state::ModuleInfo, text: &str) -> String {
    let items: Vec<&str> = m
        .defs
        .defs()
        .iter()
        .filter(|d| d.public && d.kind != DefKind::Import)
        .map(|d| d.detail.as_str())
        .collect();
    let body = if items.is_empty() { "(no public items)".to_owned() } else { items.join("\n") };
    let doc = crate::docs::module_doc(text).map_or_else(String::new, |d| format!("\n\n{d}"));
    format!("**module** `{}`{doc}\n```slab\n{body}\n```", m.name)
}

/// The type of the parameter declaration at `offset`.
fn param_type(program: &ast::Program, offset: usize) -> Option<String> {
    for item in &program.items {
        if let ast::ItemKind::Fn(f) = &item.kind
            && analysis::contains(item.span, offset)
        {
            for p in &f.params {
                if analysis::contains(p.name.span, offset) {
                    return Some(crate::defs::type_string(&p.ty));
                }
            }
        }
    }
    None
}

/// A `let` (or `for`) declaration the cursor can see: same module, last one before `offset`.
fn local_decls(program: &ast::Program, name: &str, offset: usize) -> Vec<Span> {
    let mut out = Vec::new();
    for item in &program.items {
        if let ast::ItemKind::Fn(f) = &item.kind
            && analysis::contains(item.span, offset)
        {
            let mut w = LocalWalker { name, out: &mut out };
            w.block(&f.body);
        }
    }
    out.sort_by_key(|s| s.start);
    out.iter().rev().filter(|s| (s.start as usize) < offset).copied().collect()
}

/// `#[load]` / `#[tick]` / `#[every(N)]` labels.
fn entry_label(e: &shir::EntryKind) -> String {
    match e {
        shir::EntryKind::Load => "load".to_owned(),
        shir::EntryKind::Tick => "tick".to_owned(),
        shir::EntryKind::Every(n) => format!("every({n})"),
    }
}

struct LocalWalker<'a> {
    name: &'a str,
    out: &'a mut Vec<Span>,
}

impl LocalWalker<'_> {
    fn block(&mut self, b: &ast::Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &ast::Stmt) {
        match &s.kind {
            ast::StmtKind::Let { name, init, .. } => {
                if name.name == self.name {
                    self.out.push(name.span);
                }
                self.expr(init);
            }
            ast::StmtKind::For { var, iter, body } => {
                if var.name == self.name {
                    self.out.push(var.span);
                }
                self.expr(iter);
                self.block(body);
            }
            ast::StmtKind::If { cond, then, els } => {
                self.expr(cond);
                self.block(then);
                if let Some(boxed) = els {
                    match &**boxed {
                        ast::Else::Block(b) => self.block(b),
                        ast::Else::If(inner) => self.stmt(inner),
                    }
                }
            }
            ast::StmtKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            ast::StmtKind::Loop { body } => self.block(body),
            other => self.stmt_other(other),
        }
    }

    fn stmt_other(&mut self, s: &ast::StmtKind) {
        match s {
            ast::StmtKind::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            ast::StmtKind::Return(Some(e)) => self.expr(e),
            ast::StmtKind::Context { clauses, body } => {
                for c in clauses {
                    match &c.kind {
                        ast::CtxKind::As(e) | ast::CtxKind::At(e) => self.expr(e),
                        ast::CtxKind::In(_) => {}
                        ast::CtxKind::Positioned(es) | ast::CtxKind::Facing(es) => {
                            for e in es {
                                self.expr(e);
                            }
                        }
                        ast::CtxKind::Rotated(es) => {
                            for e in es {
                                self.expr(e);
                            }
                        }
                        ast::CtxKind::Align(_) | ast::CtxKind::Anchored(_) => {}
                    }
                }
                self.block(body);
            }
            ast::StmtKind::Block(b) => self.block(b),
            ast::StmtKind::Expr(e) => self.expr(e),
            _ => {}
        }
    }

    fn expr(&mut self, e: &ast::Expr) {
        use ast::ExprKind::*;
        match &e.kind {
            Call { callee, args } => {
                self.expr(callee);
                for a in args {
                    self.expr(a);
                }
            }
            Field { base, .. } => self.expr(base),
            Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            Unary { expr, .. } => self.expr(expr),
            Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            Range { start, end, .. } => {
                if let Some(s) = start {
                    self.expr(s);
                }
                if let Some(e) = end {
                    self.expr(e);
                }
            }
            List(items) => {
                for i in items {
                    self.expr(i);
                }
            }
            Compound(entries) | StructLit { fields: entries, .. } => {
                for fi in entries {
                    self.expr(&fi.value);
                }
            }
            Format(parts) => {
                for p in parts {
                    if let ast::FormatPart::Expr(e) = p {
                        self.expr(e);
                    }
                }
            }
            _ => {}
        }
    }
}