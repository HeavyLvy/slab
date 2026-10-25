//! Position-based analysis over the AST and (when available) the HIR: the syntactic role of
//! the identifier under the cursor and what the checker resolved it to. Everything here works
//! on a *partial* program (an editor is often between states), so every use of HIR ids goes
//! through `.get()`, and `ExprKind::Error` is expected.

use slab::diag::Span;
use slab::sema::hir::{self as shir, Expr as HirExpr, ExprKind as HirExprKind, Stmt as HirStmt};
use slab::syntax::ast::{self, ExprKind, ItemKind, StmtKind};

/// Everything an identifier occurrence can be, decided purely by syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The declared name of a top-level item.
    Decl(crate::defs::DefKind),
    /// A field name in a `struct` declaration.
    StructFieldName,
    /// A field name in a struct literal or anonymous compound.
    FieldInitName,
    /// A parameter name in `fn(a: int)`.
    Param,
    /// A binding name in `let` or `for`.
    LetName,
    /// An identifier in callee position: `name(...)`.
    Callee,
    /// The member name of a method call: `xs.push(...)`.
    MethodName,
    /// The member name of a field access: `p.x`.
    FieldName,
    /// A type name in an annotation, `list<...>`, or a struct literal head.
    TypeName,
    /// The module part of `module::name`, a dotted builtin head (`effect.`), or an `import`.
    ModuleName,
    /// An `import` path segment.
    ImportModule,
    /// The tail of `module::name` in expression position.
    PathItem,
    /// An item name in `import a::{x, y}` or the alias of `import a as c`.
    ImportItem,
    /// `#[name]`.
    AttrName,
    /// Any other identifier: a local, global, const or objective use.
    Plain,
}

/// An identifier occurrence found by the AST walk.
#[derive(Clone, Debug)]
pub struct Hit {
    pub name: String,
    pub span: Span,
    pub role: Role,
}

/// The identifier under a cursor: the innermost occurrence whose span contains it.
pub fn hit_at(program: &ast::Program, offset: usize) -> Option<Hit> {
    let mut all = Vec::new();
    each_ident(program, &mut all);
    all.into_iter().filter(|h| contains(h.span, offset)).min_by_key(|h| h.span.end - h.span.start)
}

/// Walks every identifier occurrence in a program, declarations included.
pub fn each_ident(program: &ast::Program, out: &mut Vec<Hit>) {
    for item in &program.items {
        let mut w = Walker { out };
        for attr in &item.attrs {
            w.emit(attr.name.span, &attr.name.name, Role::AttrName);
            for arg in &attr.args {
                w.expr(arg, Role::Plain);
            }
        }
        match &item.kind {
            ItemKind::Fn(f) => {
                w.emit(f.name.span, &f.name.name, Role::Decl(crate::defs::DefKind::Fn));
                for p in &f.params {
                    w.emit(p.name.span, &p.name.name, Role::Param);
                    w.ty(&p.ty);
                }
                if let Some(t) = &f.ret {
                    w.ty(t);
                }
                for s in &f.body.stmts {
                    w.stmt(s);
                }
            }
            ItemKind::Global(v) => decl_var(&mut w, v, crate::defs::DefKind::Global),
            ItemKind::Const(v) => decl_var(&mut w, v, crate::defs::DefKind::Const),
            ItemKind::Struct(s) => {
                w.emit(s.name.span, &s.name.name, Role::Decl(crate::defs::DefKind::Struct));
                for p in &s.fields {
                    w.emit(p.name.span, &p.name.name, Role::StructFieldName);
                    w.ty(&p.ty);
                }
            }
            ItemKind::Enum(e) => {
                w.emit(e.name.span, &e.name.name, Role::Decl(crate::defs::DefKind::Enum));
                for g in &e.generics {
                    w.emit(g.span, &g.name, Role::TypeName);
                }
                for v in &e.variants {
                    w.emit(v.name.span, &v.name.name, Role::StructFieldName);
                    for t in &v.fields {
                        w.ty(t);
                    }
                }
            }
            ItemKind::Impl(i) => {
                for g in &i.generics {
                    w.emit(g.span, &g.name, Role::TypeName);
                }
                w.ty(&i.target);
                for m in &i.methods {
                    for attr in &m.attrs {
                        w.emit(attr.name.span, &attr.name.name, Role::AttrName);
                    }
                    let f = &m.decl;
                    w.emit(f.name.span, &f.name.name, Role::Decl(crate::defs::DefKind::Fn));
                    for p in &f.params {
                        w.emit(p.name.span, &p.name.name, Role::Param);
                        if !(f.has_self && p.name.name == "self") {
                            w.ty(&p.ty);
                        }
                    }
                    if let Some(t) = &f.ret {
                        w.ty(t);
                    }
                    for s in &f.body.stmts {
                        w.stmt(s);
                    }
                }
            }
            ItemKind::Objective { name, criteria } => {
                w.emit(name.span, &name.name, Role::Decl(crate::defs::DefKind::Objective));
                w.expr(criteria, Role::Plain);
            }
            ItemKind::Import(i) => {
                for seg in &i.path {
                    w.emit(seg.span, &seg.name, Role::ImportModule);
                }
                if let Some(a) = &i.alias {
                    w.emit(a.span, &a.name, Role::ImportItem);
                }
                if let Some(items) = &i.items {
                    for it in items {
                        w.emit(it.span, &it.name, Role::ImportItem);
                    }
                }
            }
        }
    }
}

fn decl_var(w: &mut Walker, v: &ast::VarDecl, kind: crate::defs::DefKind) {
    w.emit(v.name.span, &v.name.name, Role::Decl(kind));
    w.ty(&v.ty);
    w.expr(&v.init, Role::Plain);
}

struct Walker<'o> {
    out: &'o mut Vec<Hit>,
}

impl Walker<'_> {
    fn emit(&mut self, span: Span, name: &str, role: Role) {
        self.out.push(Hit { name: name.to_owned(), span, role });
    }

    fn block(&mut self, b: &ast::Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &ast::Stmt) {
        match &s.kind {
            StmtKind::Let { name, ty, init, .. } => {
                self.emit(name.span, &name.name, Role::LetName);
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(init, Role::Plain);
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, Role::Plain);
                self.expr(value, Role::Plain);
            }
            StmtKind::If { cond, then, els } => {
                self.expr(cond, Role::Plain);
                self.block(then);
                if let Some(boxed) = els {
                    match &**boxed {
                        ast::Else::Block(b) => self.block(b),
                        ast::Else::If(inner) => self.stmt(inner),
                    }
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond, Role::Plain);
                self.block(body);
            }
            StmtKind::Loop { body } => self.block(body),
            StmtKind::For { var, iter, body } => {
                self.emit(var.span, &var.name, Role::LetName);
                self.expr(iter, Role::Plain);
                self.block(body);
            }
            StmtKind::Return(Some(e)) => self.expr(e, Role::Plain),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Context { clauses, body } => {
                for c in clauses {
                    match &c.kind {
                        ast::CtxKind::As(e) | ast::CtxKind::At(e) => self.expr(e, Role::Plain),
                    ast::CtxKind::In(_) => {}
                    ast::CtxKind::Positioned(es) | ast::CtxKind::Facing(es) => {
                        for e in es {
                            self.expr(e, Role::Plain);
                        }
                    }
                    ast::CtxKind::Rotated(es) => {
                        for e in es {
                            self.expr(e, Role::Plain);
                        }
                    }
                    ast::CtxKind::Align(id) | ast::CtxKind::Anchored(id) => self.emit(id.span, &id.name, Role::Plain),
                    }
                }
                self.block(body);
            }
            StmtKind::Block(b) => self.block(b),
            StmtKind::Expr(e) => self.expr(e, Role::Plain),
        }
    }

    fn expr(&mut self, e: &ast::Expr, role: Role) {
        match &e.kind {
            ExprKind::Ident(name) => self.emit(e.span, name, role),
            ExprKind::Call { callee, args } => {
                self.callee(callee);
                for a in args {
                    self.expr(a, Role::Plain);
                }
            }
            ExprKind::Field { base, name } => {
                self.expr(base, Role::Plain);
                self.emit(name.span, &name.name, Role::FieldName);
            }
            ExprKind::Index { base, index } => {
                self.expr(base, Role::Plain);
                self.expr(index, Role::Plain);
            }
            ExprKind::Unary { expr, .. } => self.expr(expr, Role::Plain),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs, Role::Plain);
                self.expr(rhs, Role::Plain);
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.expr(s, Role::Plain);
                }
                if let Some(e) = end {
                    self.expr(e, Role::Plain);
                }
            }
            ExprKind::List(items) => {
                for i in items {
                    self.expr(i, Role::Plain);
                }
            }
            ExprKind::Compound(entries) | ExprKind::StructLit { fields: entries, .. } => {
                if let ExprKind::StructLit { module, name, .. } = &e.kind {
                    if let Some(m) = module {
                        self.emit(m.span, &m.name, Role::ModuleName);
                    }
                    self.emit(name.span, &name.name, Role::TypeName);
                }
                for fi in entries {
                    self.emit(fi.name_span, &fi.name, Role::FieldInitName);
                    self.expr(&fi.value, Role::Plain);
                }
            }
            ExprKind::Path { module, name } => {
                self.emit(module.span, &module.name, Role::ModuleName);
                self.emit(name.span, &name.name, Role::PathItem);
            }
            ExprKind::Format(parts) => {
                for p in parts {
                    if let ast::FormatPart::Expr(e) = p {
                        // A hole is a normal expression subtree with absolute spans.
                        self.expr(e, Role::Plain);
                    }
                }
            }
            ExprKind::Selector(lit) => {
                // `$name` / `${expr}` in an option value are ordinary expressions
                for a in &lit.args {
                    for h in &a.holes {
                        self.expr(h, Role::Plain);
                    }
                }
            }
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Unit | ExprKind::Error | ExprKind::Rel { .. } => {}
        }
    }

    fn callee(&mut self, c: &ast::Expr) {
        match &c.kind {
            ExprKind::Ident(name) => self.emit(c.span, name, Role::Callee),
            ExprKind::Field { base, name } => {
                self.expr(base, Role::Plain);
                self.emit(name.span, &name.name, Role::MethodName);
            }
            ExprKind::Path { module, name } => {
                self.emit(module.span, &module.name, Role::ModuleName);
                self.emit(name.span, &name.name, Role::Callee);
            }
            _ => self.expr(c, Role::Plain),
        }
    }

    fn ty(&mut self, t: &ast::Type) {
        if let Some(m) = &t.module {
            self.emit(m.span, &m.name, Role::ModuleName);
        }
        self.emit(t.name.span, &t.name.name, Role::TypeName);
        for a in &t.args {
            self.ty(a);
        }
    }
}

/// `offset` inside a byte span.
pub fn contains(span: Span, offset: usize) -> bool {
    (span.start as usize..span.end as usize).contains(&offset)
}

/// Every expression node in a program, pre-order (attributes, bodies, all subtrees). Used by
/// callers that care about specific node kinds (f-string holes for instance) without a
/// role-context of their own.
pub fn each_expr<'a>(program: &'a ast::Program, f: &mut impl FnMut(&'a ast::Expr)) {
    for item in &program.items {
        for attr in &item.attrs {
            for arg in &attr.args {
                visit(arg, f);
            }
        }
        match &item.kind {
            ItemKind::Fn(decl) => {
                for s in &decl.body.stmts {
                    stmt(s, f);
                }
            }
            ItemKind::Global(v) | ItemKind::Const(v) => visit(&v.init, f),
            ItemKind::Objective { criteria, .. } => visit(criteria, f),
            ItemKind::Impl(i) => {
                for m in &i.methods {
                    for s in &m.decl.body.stmts {
                        stmt(s, f);
                    }
                }
            }
            ItemKind::Struct(_) | ItemKind::Enum(_) | ItemKind::Import(_) => {}
        }
    }
}

fn stmt<'a>(s: &'a ast::Stmt, f: &mut impl FnMut(&'a ast::Expr)) {
    match &s.kind {
        StmtKind::Let { init, .. } => visit(init, f),
        StmtKind::Assign { target, value, .. } => {
            visit(target, f);
            visit(value, f);
        }
        StmtKind::If { cond, then, els } => {
            visit(cond, f);
            block(then, f);
            if let Some(boxed) = els {
                match &**boxed {
                    ast::Else::Block(b) => block(b, f),
                    ast::Else::If(inner) => stmt(inner, f),
                }
            }
        }
        StmtKind::While { cond, body } => {
            visit(cond, f);
            block(body, f);
        }
        StmtKind::Loop { body } => block(body, f),
        StmtKind::For { iter, body, .. } => {
            visit(iter, f);
            block(body, f);
        }
        StmtKind::Return(Some(e)) => visit(e, f),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
        StmtKind::Context { clauses, body } => {
            for c in clauses {
                match &c.kind {
                    ast::CtxKind::As(e) | ast::CtxKind::At(e) => visit(e, f),
                    ast::CtxKind::In(_) => {}
                    ast::CtxKind::Positioned(es) | ast::CtxKind::Facing(es) => {
                        for e in es {
                            visit(e, f);
                        }
                    }
                    ast::CtxKind::Rotated(es) => {
                        for e in es {
                            visit(e, f);
                        }
                    }
                    ast::CtxKind::Align(_) | ast::CtxKind::Anchored(_) => {}
                }
            }
            block(body, f);
        }
        StmtKind::Block(b) => block(b, f),
        StmtKind::Expr(e) => visit(e, f),
    }
}

fn block<'a>(b: &'a ast::Block, f: &mut impl FnMut(&'a ast::Expr)) {
    for s in &b.stmts {
        stmt(s, f);
    }
}

fn visit<'a>(e: &'a ast::Expr, f: &mut impl FnMut(&'a ast::Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Call { callee, args } => {
            visit(callee, f);
            for a in args {
                visit(a, f);
            }
        }
        ExprKind::Field { base, .. } => visit(base, f),
        ExprKind::Index { base, index } => {
            visit(base, f);
            visit(index, f);
        }
        ExprKind::Unary { expr, .. } => visit(expr, f),
        ExprKind::Binary { lhs, rhs, .. } => {
            visit(lhs, f);
            visit(rhs, f);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                visit(s, f);
            }
            if let Some(e) = end {
                visit(e, f);
            }
        }
        ExprKind::List(items) => {
            for i in items {
                visit(i, f);
            }
        }
        ExprKind::Compound(entries) | ExprKind::StructLit { fields: entries, .. } => {
            for fi in entries {
                visit(&fi.value, f);
            }
        }
        ExprKind::Format(parts) => {
            for p in parts {
                if let ast::FormatPart::Expr(e) = p {
                    visit(e, f);
                }
            }
        }
        _ => {}
    }
}

/// Span length in bytes ([`Span`] has no `len`).
pub fn span_len(span: Span) -> u32 {
    span.end - span.start
}

// ------------------------------------------------------------------ HIR walking

/// The smallest HIR expression whose span contains `offset`, with its function.
#[derive(Clone, Debug)]
pub struct HirCtx {
    pub fn_idx: usize,
    pub expr: HirExpr,
}

pub fn hir_ctx_at(hir: &shir::Hir, module: usize, offset: usize) -> Option<HirCtx> {
    let mut chosen: Option<HirCtx> = None;
    for (fi, f) in hir.fns.iter().enumerate() {
        if f.module != module || !contains(f.span, offset) {
            continue;
        }
        let mut best: Option<(u32, HirExpr)> = None;
        for s in &f.body.stmts {
            visit_hir_stmt(s, offset, &mut best);
        }
        if let Some((len, expr)) = best
            && chosen.as_ref().is_none_or(|c| span_len(c.expr.span) > len)
        {
            chosen = Some(HirCtx { fn_idx: fi, expr });
        }
    }
    chosen
}

/// The local of a `let`/parameter binding whose name span contains `offset`.
pub fn hir_local_at(hir: &shir::Hir, module: usize, offset: usize) -> Option<(usize, usize)> {
    for (fi, f) in hir.fns.iter().enumerate() {
        if f.module != module {
            continue;
        }
        for (li, l) in f.locals.iter().enumerate() {
            if contains(l.span, offset) {
                return Some((fi, li));
            }
        }
    }
    None
}

/// The parameters and `let` bindings of the function around `offset` that are declared before
/// it (innermost shadowing wins; block scopes are not tracked, so a binding from a block that
/// already ended can still be listed).
pub fn locals_before(hir: &shir::Hir, module: usize, offset: usize) -> Vec<(String, String, bool)> {
    let Some(f) = hir.fns.iter().find(|f| f.module == module && contains(f.span, offset)) else { return Vec::new() };
    let mut out: Vec<(String, String, bool)> = Vec::new();
    for l in f.locals.iter().filter(|l| (l.span.end as usize) <= offset && !l.name.is_empty()) {
        out.retain(|(n, ..)| *n != l.name);
        out.push((l.name.clone(), ty_string(&l.ty, hir), l.mutable));
    }
    out
}

fn visit_hir_stmt(s: &HirStmt, offset: usize, best: &mut Option<(u32, HirExpr)>) {
    match s {
        HirStmt::Let { init, .. } => visit_hir_expr(init, offset, best),
        HirStmt::Assign { target, value, .. } => {
            visit_hir_expr(target, offset, best);
            visit_hir_expr(value, offset, best);
        }
        HirStmt::If { cond, then, els } => {
            visit_hir_expr(cond, offset, best);
            visit_hir_block(then, offset, best);
            visit_hir_block(els, offset, best);
        }
        HirStmt::While { cond, body } => {
            visit_hir_expr(cond, offset, best);
            visit_hir_block(body, offset, best);
        }
        HirStmt::Loop { body } => visit_hir_block(body, offset, best),
        HirStmt::ForRange { start, end, body, .. } => {
            visit_hir_expr(start, offset, best);
            visit_hir_expr(end, offset, best);
            visit_hir_block(body, offset, best);
        }
        HirStmt::ForList { list, body, .. } => {
            visit_hir_expr(list, offset, best);
            visit_hir_block(body, offset, best);
        }
        HirStmt::Return(Some(e)) => visit_hir_expr(e, offset, best),
        HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
        HirStmt::Context { clauses, body } => {
            for c in clauses {
                match c {
                    shir::CtxClause::As(e) | shir::CtxClause::At(e) => visit_hir_expr(e, offset, best),
                    shir::CtxClause::In(_) | shir::CtxClause::Align(_) | shir::CtxClause::Anchored(_) => {}
                    shir::CtxClause::Positioned(es) | shir::CtxClause::Facing(es) => {
                        for e in es {
                            visit_hir_expr(e, offset, best);
                        }
                    }
                    shir::CtxClause::Rotated(es) => {
                        for e in es {
                            visit_hir_expr(e, offset, best);
                        }
                    }
                }
            }
            visit_hir_block(body, offset, best);
        }
        HirStmt::Block(b) => visit_hir_block(b, offset, best),
        HirStmt::Expr(e) => visit_hir_expr(e, offset, best),
    }
}

fn visit_hir_block(b: &shir::Block, offset: usize, best: &mut Option<(u32, HirExpr)>) {
    for s in &b.stmts {
        visit_hir_stmt(s, offset, best);
    }
}

fn visit_hir_expr(e: &HirExpr, offset: usize, best: &mut Option<(u32, HirExpr)>) {
    if !contains(e.span, offset) {
        return;
    }
    if best.as_ref().is_none_or(|(len, _)| span_len(e.span) < *len) {
        *best = Some((span_len(e.span), e.clone()));
    }
    match &e.kind {
        HirExprKind::Format(parts) => {
            for p in parts {
                if let shir::FormatPart::Expr(e) = p {
                    visit_hir_expr(e, offset, best);
                }
            }
        }
        HirExprKind::List(items) => {
            for i in items {
                visit_hir_expr(i, offset, best);
            }
        }
        HirExprKind::Struct { fields, .. } => {
            for f in fields {
                visit_hir_expr(f, offset, best);
            }
        }
        HirExprKind::ToEntity(inner) => visit_hir_expr(inner, offset, best),
        HirExprKind::Variant { args, .. } => {
            for a in args {
                visit_hir_expr(a, offset, best);
            }
        }
        HirExprKind::Unit => {}
        HirExprKind::Unary { expr, .. } => visit_hir_expr(expr, offset, best),
        HirExprKind::Binary { lhs, rhs, .. } => {
            visit_hir_expr(lhs, offset, best);
            visit_hir_expr(rhs, offset, best);
        }
        HirExprKind::Call { args, .. } | HirExprKind::Builtin { args, .. } => {
            for a in args {
                visit_hir_expr(a, offset, best);
            }
        }
        HirExprKind::Method { recv, args, .. } => {
            visit_hir_expr(recv, offset, best);
            for a in args {
                visit_hir_expr(a, offset, best);
            }
        }
        HirExprKind::Field { base, .. } => visit_hir_expr(base, offset, best),
        HirExprKind::Index { base, index } => {
            visit_hir_expr(base, offset, best);
            visit_hir_expr(index, offset, best);
        }
        HirExprKind::Score { holder, .. } => visit_hir_expr(holder, offset, best),
        HirExprKind::Int(_) | HirExprKind::Bool(_) | HirExprKind::Str(_) | HirExprKind::Global(_) | HirExprKind::Local(_) | HirExprKind::Rel { .. } | HirExprKind::Error => {}
        HirExprKind::Selector(_, holes) => {
            for h in holes {
                visit_hir_expr(h, offset, best);
            }
        }
    }
}

/// A type for display, with struct names resolved.
pub fn ty_string(ty: &slab::sema::ty::Ty, hir: &shir::Hir) -> String {
    let names: Vec<String> = hir.structs.iter().map(|s| s.name.clone()).collect();
    let enums: Vec<String> = hir.enums.iter().map(|e| e.name.clone()).collect();
    ty.display(&names, &enums).to_string()
}