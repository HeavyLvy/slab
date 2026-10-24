//! HIR -> IR. Everything here has already been type checked; this pass only decides how checked
//! constructs map onto the IR's runtime model (docs/lang/ir.md): `int`/`bool` on the scoreboard,
//! `str`/`list`/structs in command storage.
//!
//! Notable translations (docs/lang/codegen.md):
//! * module-qualified names become flat `[a-z0-9_]` IR names (`game::arena::start` -> `game_arena__start`);
//! * `break`/`continue` become flag variables and the code after a statement that may jump is
//!   guarded by them;
//! * a non-constant list/struct literal is built in a temporary with `Append`/`SetField`
//!   (collected in `pre` and flushed in front of the statement that needs it);
//! * ints and bools are converted at every storage boundary (`ToData`/`ToInt`);
//! * `for x in list` consumes a copy of the list from the front, since indexing by a variable
//!   would need macros;
//! * `#[every(N)]` becomes a counter global and a tick function.

use std::collections::{BTreeSet, HashSet};

use crate::datapack::{self, Anchor, Axes, Clause, Command, Compound, Coord, Effect, HolderName, Hole, Nbt, NbtPath, Num, Pos, ResourceLocation, Rotation, ScoreHolder, Selector, TagAction, Word};
use crate::diag::{Diagnostic, Span};
use crate::ir;
use crate::sema::hir as h;
use crate::sema::ty::Ty;
use crate::syntax::ast::{AssignOp, BinOp as AstBin, UnOp as AstUn};
use crate::syntax::token::RelKind;

/// Translates a checked program. Errors are things the language accepts but the backend cannot
/// express yet (variable coordinates, variable list indices, ...).
pub fn generate(hir: &h::Hir, namespace: &str, description: &str) -> Result<ir::Program, Vec<Diagnostic>> {
    let mut g = Gen::new(hir, namespace);
    let mut program = ir::Program::new(namespace);
    program.description = description.to_owned();
    program.objectives = g.objectives();
    program.globals = g.globals();
    for i in 0..hir.fns.len() {
        g.function(i, &mut program.functions);
    }
    program.functions.append(&mut g.extra_fns);
    program.globals.append(&mut g.extra_globals);
    program.uninstall = g.uninstall_commands();
    if g.errors.is_empty() { Ok(program) } else { Err(g.errors) }
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c.to_ascii_lowercase() } else { '_' }).collect()
}

/// `game::arena` -> `game_arena__`; the root module has no prefix.
fn module_prefix(module: &str) -> String {
    if module == "main" { String::new() } else { format!("{}__", sanitize(module)) }
}

fn unique(used: &mut HashSet<String>, base: String) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    (2..).map(|n| format!("{base}_{n}")).find(|c| used.insert(c.clone())).expect("infinite iterator")
}

fn kind_of(ty: &Ty) -> ir::Kind {
    match ty {
        Ty::Int => ir::Kind::Int,
        Ty::Bool => ir::Kind::Bool,
        _ => ir::Kind::Data,
    }
}

fn is_scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Bool)
}

fn bin_op(op: AstBin) -> ir::BinOp {
    match op {
        AstBin::Add => ir::BinOp::Add,
        AstBin::Sub => ir::BinOp::Sub,
        AstBin::Mul => ir::BinOp::Mul,
        AstBin::Div => ir::BinOp::Div,
        AstBin::Rem => ir::BinOp::Mod,
        AstBin::Eq => ir::BinOp::Eq,
        AstBin::Ne => ir::BinOp::Ne,
        AstBin::Lt => ir::BinOp::Lt,
        AstBin::Le => ir::BinOp::Le,
        AstBin::Gt => ir::BinOp::Gt,
        AstBin::Ge => ir::BinOp::Ge,
        AstBin::And => ir::BinOp::And,
        AstBin::Or => ir::BinOp::Or,
    }
}

fn assign_bin_op(op: AssignOp) -> Option<ir::BinOp> {
    match op {
        AssignOp::Assign => None,
        AssignOp::Add => Some(ir::BinOp::Add),
        AssignOp::Sub => Some(ir::BinOp::Sub),
        AssignOp::Mul => Some(ir::BinOp::Mul),
        AssignOp::Div => Some(ir::BinOp::Div),
        AssignOp::Rem => Some(ir::BinOp::Mod),
    }
}

#[derive(Clone)]
struct LoopCtx {
    brk: Option<String>,
    cont: Option<String>,
}

/// The run-time values of one command's [`Hole`]s: hole `n` is filled with `args[n]`.
#[derive(Default)]
struct Holes {
    args: Vec<(Hole, ir::Expr)>,
}

struct Gen<'a> {
    hir: &'a h::Hir,
    namespace: String,
    /// The program summoned an entity, which `summon` tagged as ours.
    uses_summon: bool,
    /// Tags the program adds to entities (`tag.add`), removed again by `_uninstall`.
    added_tags: BTreeSet<String>,
    fn_names: Vec<String>,
    global_names: Vec<String>,
    objective_names: Vec<String>,
    errors: Vec<Diagnostic>,
    extra_fns: Vec<ir::Function>,
    extra_globals: Vec<ir::Global>,
    used_fns: HashSet<String>,
    used_globals: HashSet<String>,
    // --- per function
    cur_fn: usize,
    local_names: Vec<String>,
    used: HashSet<String>,
    pre: Vec<ir::Stmt>,
    loops: Vec<LoopCtx>,
}

impl<'a> Gen<'a> {
    fn new(hir: &'a h::Hir, namespace: &str) -> Self {
        let mut used_fns = HashSet::from(["_init".to_owned(), "_uninstall".to_owned()]);
        let fn_names = hir.fns.iter().map(|f| unique(&mut used_fns, format!("{}{}", module_prefix(&hir.modules[f.module]), sanitize(&f.name)))).collect();
        let mut used_globals = HashSet::new();
        let global_names = hir.globals.iter().map(|g| unique(&mut used_globals, format!("{}{}", module_prefix(&hir.modules[g.module]), sanitize(&g.name)))).collect();
        let objective_names = hir
            .objectives
            .iter()
            .map(|o| if hir.modules[o.module] == "main" { o.name.clone() } else { format!("{}_{}", sanitize(&hir.modules[o.module]), o.name) })
            .collect();
        Gen {
            hir,
            namespace: namespace.to_owned(),
            uses_summon: false,
            added_tags: BTreeSet::new(),
            fn_names,
            global_names,
            objective_names,
            errors: Vec::new(),
            extra_fns: Vec::new(),
            extra_globals: Vec::new(),
            used_fns,
            used_globals,
            cur_fn: 0,
            local_names: Vec::new(),
            used: HashSet::new(),
            pre: Vec::new(),
            loops: Vec::new(),
        }
    }

    /// A new hole in the command being built, filled at run time with `v`. The value may be read
    /// again (`xs[i] += 1` reads and writes the same element), so anything but a literal or a
    /// variable is first bound to a temporary.
    fn hole_ir(&mut self, holes: &mut Holes, v: ir::Expr) -> Hole {
        let v = match v {
            ir::Expr::Int(_) | ir::Expr::Var(_) | ir::Expr::Global(_) => v,
            other => {
                let t = self.fresh("hv");
                self.pre.push(ir::Stmt::Let { name: t.clone(), kind: ir::Kind::Int, init: Some(other) });
                ir::Expr::var(&t)
            }
        };
        let h = Hole(holes.args.len() as u16);
        holes.args.push((h, v));
        h
    }

    fn hole(&mut self, holes: &mut Holes, e: &h::Expr) -> Hole {
        let v = self.expr(e);
        self.hole_ir(holes, v)
    }

    /// The tag put on every entity the program summons, so `_uninstall` can find them again.
    fn owned_tag(&self) -> String {
        format!("{}.owned", self.namespace)
    }

    /// Commands for `_uninstall`: remove what the program put into the world. (Scoreboard and
    /// storage state is removed by the lowering.)
    fn uninstall_commands(&self) -> Vec<Command> {
        let mut cmds = Vec::new();
        if self.uses_summon
            && let Ok(tag) = Word::new(&self.owned_tag())
        {
            cmds.push(Command::Kill(Some(Selector::e().tag(tag))));
        }
        for t in &self.added_tags {
            if let Ok(tag) = Word::new(t) {
                cmds.push(Command::Tag { targets: Selector::e().tag(tag.clone()), action: TagAction::Remove(tag) });
            }
        }
        cmds
    }

    fn err(&mut self, span: Span, msg: impl Into<String>) {
        let module = self.hir.fns.get(self.cur_fn).map_or(0, |f| f.module);
        self.errors.push(Diagnostic::new(msg, span).in_file(self.hir.files[module]));
    }

    fn fresh(&mut self, base: &str) -> String {
        unique(&mut self.used, base.to_owned())
    }

    // ------------------------------------------------------------- program parts

    fn objectives(&mut self) -> Vec<ir::ObjectiveDecl> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (i, o) in self.hir.objectives.iter().enumerate() {
            let name = self.objective_names[i].clone();
            if !seen.insert(name.clone()) {
                self.errors.push(Diagnostic::new(format!("objective `{name}` is defined in more than one module"), Span::default()).in_file(self.hir.files[o.module]));
                continue;
            }
            out.push(ir::ObjectiveDecl { name, criteria: o.criteria.clone() });
        }
        out
    }

    fn globals(&mut self) -> Vec<ir::Global> {
        self.hir
            .globals
            .iter()
            .enumerate()
            .map(|(i, g)| ir::Global { name: self.global_names[i].clone(), kind: kind_of(&g.ty), init: self.const_expr(&g.init, &g.ty) })
            .collect()
    }

    fn const_expr(&self, v: &h::ConstVal, ty: &Ty) -> ir::Expr {
        match (v, ty) {
            (h::ConstVal::Int(n), _) => ir::Expr::Int(*n),
            (h::ConstVal::Bool(b), _) => ir::Expr::Bool(*b),
            _ => ir::Expr::Data(self.const_nbt(v)),
        }
    }

    fn const_nbt(&self, v: &h::ConstVal) -> Nbt {
        match v {
            h::ConstVal::Int(n) => Nbt::Int(*n),
            h::ConstVal::Bool(b) => Nbt::Int(*b as i32),
            h::ConstVal::Str(s) => Nbt::String(s.clone()),
            h::ConstVal::List(items) => Nbt::List(items.iter().map(|i| self.const_nbt(i)).collect()),
            h::ConstVal::Struct(id, vals) => {
                let mut c = Compound::new();
                for ((name, _), val) in self.hir.structs[id.0].fields.iter().zip(vals) {
                    c.insert(name.clone(), self.const_nbt(val));
                }
                Nbt::Compound(c)
            }
        }
    }

    fn function(&mut self, idx: usize, out: &mut Vec<ir::Function>) {
        let f = &self.hir.fns[idx];
        self.cur_fn = idx;
        self.used = HashSet::new();
        self.loops.clear();
        self.pre.clear();
        self.local_names = f.locals.iter().map(|l| unique(&mut self.used, sanitize(&l.name))).collect();
        let params = f.params.iter().map(|id| ir::Param { name: self.local_names[id.0].clone(), kind: kind_of(&f.locals[id.0].ty) }).collect();
        let body = self.block(&f.body);
        let name = self.fn_names[idx].clone();
        let entry = match f.entry {
            Some(h::EntryKind::Load) => Some(ir::Entry::Load),
            Some(h::EntryKind::Tick) | Some(h::EntryKind::Every(1)) => Some(ir::Entry::Tick),
            _ => None,
        };
        out.push(ir::Function { name: name.clone(), params, ret: (f.ret != Ty::Unit).then(|| kind_of(&f.ret)), body, entry });
        if let Some(h::EntryKind::Every(n)) = f.entry
            && n > 1
        {
            // a counter global and a tick function that calls `f` once every `n` ticks
            let counter = unique(&mut self.used_globals, format!("{name}__every"));
            self.extra_globals.push(ir::Global { name: counter.clone(), kind: ir::Kind::Int, init: ir::Expr::Int(0) });
            let tick_name = unique(&mut self.used_fns, format!("{name}__every_tick"));
            let c = || ir::Expr::Global(counter.clone());
            let body = vec![
                ir::Stmt::Assign { target: ir::VarRef::Global(counter.clone()), value: ir::Expr::bin(ir::BinOp::Add, c(), ir::Expr::Int(1)) },
                ir::Stmt::If {
                    cond: ir::Expr::bin(ir::BinOp::Ge, c(), ir::Expr::Int(n as i32)),
                    then: vec![ir::Stmt::Assign { target: ir::VarRef::Global(counter.clone()), value: ir::Expr::Int(0) }, ir::Stmt::Expr(ir::Expr::call(&name, vec![]))],
                    els: vec![],
                },
            ];
            self.extra_fns.push(ir::Function { name: tick_name, params: vec![], ret: None, body, entry: Some(ir::Entry::Tick) });
        }
    }

    // ---------------------------------------------------------------- statements

    fn flush(&mut self, out: &mut Vec<ir::Stmt>) {
        out.append(&mut self.pre);
    }

    /// Lowers `e` and returns the statements it needs before it along with the expression.
    fn expr_pre(&mut self, e: &h::Expr) -> (Vec<ir::Stmt>, ir::Expr) {
        let saved = std::mem::take(&mut self.pre);
        let v = self.expr(e);
        let pre = std::mem::replace(&mut self.pre, saved);
        (pre, v)
    }

    fn block(&mut self, b: &h::Block) -> Vec<ir::Stmt> {
        self.stmts(&b.stmts)
    }

    fn stmts(&mut self, stmts: &[h::Stmt]) -> Vec<ir::Stmt> {
        let mut out = Vec::new();
        for (i, s) in stmts.iter().enumerate() {
            self.stmt(s, &mut out);
            let (brk, cont) = jumps(s);
            if (brk || cont) && i + 1 < stmts.len() {
                // the rest of the block only runs if the statement did not break or continue
                let rest = self.stmts(&stmts[i + 1..]);
                let ctx = self.loops.last().cloned();
                let flag = |name: &Option<String>| name.as_ref().map(|n| ir::Expr::bin(ir::BinOp::Eq, ir::Expr::var(n), ir::Expr::Int(0)));
                let guards: Vec<ir::Expr> = ctx.iter().flat_map(|c| [if brk { flag(&c.brk) } else { None }, if cont { flag(&c.cont) } else { None }]).flatten().collect();
                match guards.into_iter().reduce(|a, b| ir::Expr::bin(ir::BinOp::And, a, b)) {
                    Some(cond) => out.push(ir::Stmt::If { cond, then: rest, els: vec![] }),
                    None => out.extend(rest),
                }
                break;
            }
        }
        out
    }

    fn set_flag(&self, name: &str, v: i32) -> ir::Stmt {
        ir::Stmt::Assign { target: ir::VarRef::Local(name.to_owned()), value: ir::Expr::Int(v) }
    }

    fn stmt(&mut self, s: &h::Stmt, out: &mut Vec<ir::Stmt>) {
        debug_assert!(self.pre.is_empty());
        match s {
            h::Stmt::Let { local, init } => {
                let v = self.expr(init);
                self.flush(out);
                let info = &self.hir.fns[self.cur_fn].locals[local.0];
                out.push(ir::Stmt::Let { name: self.local_names[local.0].clone(), kind: kind_of(&info.ty), init: Some(v) });
            }
            h::Stmt::Assign { target, op, value } => self.assign(target, *op, value, out),
            h::Stmt::If { cond, then, els } => {
                let c = self.expr(cond);
                self.flush(out);
                let then = self.block(then);
                let els = self.block(els);
                out.push(ir::Stmt::If { cond: c, then, els });
            }
            h::Stmt::While { cond, body } => self.while_loop(cond, body, out),
            h::Stmt::Loop { body } => {
                let ctx = self.begin_loop(body, out);
                let body_ir = self.block(body);
                self.loops.pop();
                let cond = ctx.brk.as_ref().map_or(ir::Expr::Bool(true), |b| ir::Expr::bin(ir::BinOp::Eq, ir::Expr::var(b), ir::Expr::Int(0)));
                out.push(ir::Stmt::While { cond, body: self.with_reset(&ctx, body_ir) });
            }
            h::Stmt::ForRange { var, start, end, inclusive, body } => {
                let s = self.expr(start);
                let e = self.expr(end);
                self.flush(out);
                let var_name = self.local_names[var.0].clone();
                let end_name = self.fresh("end");
                out.push(ir::Stmt::Let { name: var_name.clone(), kind: ir::Kind::Int, init: Some(s) });
                out.push(ir::Stmt::Let { name: end_name.clone(), kind: ir::Kind::Int, init: Some(e) });
                let ctx = self.begin_loop(body, out);
                let mut body_ir = self.block(body);
                self.loops.pop();
                body_ir.push(ir::Stmt::Assign { target: ir::VarRef::Local(var_name.clone()), value: ir::Expr::bin(ir::BinOp::Add, ir::Expr::var(&var_name), ir::Expr::Int(1)) });
                let cmp = ir::Expr::bin(if *inclusive { ir::BinOp::Le } else { ir::BinOp::Lt }, ir::Expr::var(&var_name), ir::Expr::var(&end_name));
                out.push(ir::Stmt::While { cond: self.with_break_guard(&ctx, cmp), body: self.with_reset(&ctx, body_ir) });
            }
            h::Stmt::ForList { var, list, body } => {
                let l = self.expr(list);
                self.flush(out);
                let iter = self.fresh("iter");
                out.push(ir::Stmt::Let { name: iter.clone(), kind: ir::Kind::Data, init: Some(l) });
                let elem_ty = self.hir.fns[self.cur_fn].locals[var.0].ty.clone();
                let ctx = self.begin_loop(body, out);
                let first = NbtPath::new().index(0);
                let mut body_ir = vec![
                    ir::Stmt::Let {
                        name: self.local_names[var.0].clone(),
                        kind: kind_of(&elem_ty),
                        init: Some(from_data(ir::Expr::Field(ir::VarRef::Local(iter.clone()), first.clone()), &elem_ty)),
                    },
                    ir::Stmt::Remove { target: ir::VarRef::Local(iter.clone()), path: first },
                ];
                body_ir.extend(self.block(body));
                self.loops.pop();
                let has_items = ir::Expr::bin(ir::BinOp::Gt, ir::Expr::Len(ir::VarRef::Local(iter), NbtPath::new()), ir::Expr::Int(0));
                out.push(ir::Stmt::While { cond: self.with_break_guard(&ctx, has_items), body: self.with_reset(&ctx, body_ir) });
            }
            h::Stmt::Return(value) => {
                let v = value.as_ref().map(|e| self.expr(e));
                self.flush(out);
                out.push(ir::Stmt::Return(v));
            }
            h::Stmt::Break => {
                if let Some(Some(b)) = self.loops.last().map(|l| l.brk.clone()) {
                    out.push(self.set_flag(&b, 1));
                }
            }
            h::Stmt::Continue => {
                if let Some(Some(c)) = self.loops.last().map(|l| l.cont.clone()) {
                    out.push(self.set_flag(&c, 1));
                }
            }
            h::Stmt::Context { clauses, body } => {
                let mut cs = Vec::new();
                let mut holes = Holes::default();
                for c in clauses {
                    if let Some(c) = self.clause(c, &mut holes) {
                        cs.push(c);
                    }
                }
                self.flush(out);
                let body = self.block(body);
                out.push(ir::Stmt::Context { clauses: cs, args: holes.args, body });
            }
            h::Stmt::Block(b) => {
                let inner = self.block(b);
                out.extend(inner);
            }
            h::Stmt::Expr(e) => self.expr_stmt(e, out),
        }
    }

    /// Declares the loop's `break`/`continue` flags (if the body uses them) and enters it.
    fn begin_loop(&mut self, body: &h::Block, out: &mut Vec<ir::Stmt>) -> LoopCtx {
        let (b, c) = jumps_block(body);
        let brk = b.then(|| self.fresh("brk"));
        let cont = c.then(|| self.fresh("cont"));
        for n in brk.iter().chain(cont.iter()) {
            out.push(ir::Stmt::Let { name: n.clone(), kind: ir::Kind::Int, init: Some(ir::Expr::Int(0)) });
        }
        let ctx = LoopCtx { brk, cont };
        self.loops.push(ctx.clone());
        ctx
    }

    /// `continue` only skips the rest of one iteration, so its flag is cleared at the start of each.
    fn with_reset(&self, ctx: &LoopCtx, mut body: Vec<ir::Stmt>) -> Vec<ir::Stmt> {
        if let Some(c) = &ctx.cont {
            body.insert(0, self.set_flag(c, 0));
        }
        body
    }

    fn with_break_guard(&self, ctx: &LoopCtx, cond: ir::Expr) -> ir::Expr {
        match &ctx.brk {
            Some(b) => ir::Expr::bin(ir::BinOp::And, ir::Expr::bin(ir::BinOp::Eq, ir::Expr::var(b), ir::Expr::Int(0)), cond),
            None => cond,
        }
    }

    fn while_loop(&mut self, cond: &h::Expr, body: &h::Block, out: &mut Vec<ir::Stmt>) {
        let ctx = self.begin_loop(body, out);
        let body_ir = self.block(body);
        self.loops.pop();
        let body_ir = self.with_reset(&ctx, body_ir);
        let (pre, c) = self.expr_pre(cond);
        match &ctx.brk {
            None if pre.is_empty() => out.push(ir::Stmt::While { cond: c, body: body_ir }),
            None => {
                // the condition needs statements (a temporary), so it is recomputed at the end of
                // every iteration into a flag
                let flag = self.fresh("cond");
                out.extend(pre);
                out.push(ir::Stmt::Let { name: flag.clone(), kind: ir::Kind::Bool, init: Some(c) });
                let (pre2, c2) = self.expr_pre(cond);
                let mut body = body_ir;
                body.extend(pre2);
                body.push(ir::Stmt::Assign { target: ir::VarRef::Local(flag.clone()), value: c2 });
                out.push(ir::Stmt::While { cond: ir::Expr::var(&flag), body });
            }
            Some(b) => {
                // evaluate the condition inside the loop, so a call in it is not re-run after a break
                let mut inner = pre;
                inner.push(ir::Stmt::If { cond: c, then: body_ir, els: vec![self.set_flag(b, 1)] });
                out.push(ir::Stmt::While { cond: ir::Expr::bin(ir::BinOp::Eq, ir::Expr::var(b), ir::Expr::Int(0)), body: inner });
            }
        }
    }

    fn assign(&mut self, target: &h::Expr, op: AssignOp, value: &h::Expr, out: &mut Vec<ir::Stmt>) {
        let bop = assign_bin_op(op);
        match &target.kind {
            h::ExprKind::Local(_) | h::ExprKind::Global(_) => {
                let var = match &target.kind {
                    h::ExprKind::Local(id) => ir::VarRef::Local(self.local_names[id.0].clone()),
                    h::ExprKind::Global(id) => ir::VarRef::Global(self.global_names[id.0].clone()),
                    _ => unreachable!(),
                };
                let mut v = self.expr(value);
                if let Some(bop) = bop {
                    let cur = match &var {
                        ir::VarRef::Local(n) => ir::Expr::var(n),
                        ir::VarRef::Global(n) => ir::Expr::global(n),
                    };
                    v = ir::Expr::bin(bop, cur, v);
                }
                self.flush(out);
                out.push(ir::Stmt::Assign { target: var, value: v });
            }
            h::ExprKind::Field { .. } | h::ExprKind::Index { .. } => {
                let mut holes = Holes::default();
                let (var, path) = self.place(target, &mut holes);
                let mut v = self.expr(value);
                if let Some(bop) = bop {
                    let cur = self.read_place(&var, &path, &holes, &target.ty);
                    v = ir::Expr::bin(bop, cur, v);
                }
                let v = to_data(v, &target.ty);
                self.flush(out);
                if holes.args.is_empty() {
                    out.push(ir::Stmt::SetField { target: var, path, value: v });
                } else {
                    out.push(ir::Stmt::WriteAt { target: var, path, value: v, args: holes.args });
                }
            }
            h::ExprKind::Score { objective, holder } => {
                let obj = self.objective_names[objective.0].clone();
                let holder = self.holder(holder);
                let v = self.expr(value);
                self.flush(out);
                out.push(ir::Stmt::SetScore { objective: obj, holder, op: bop, value: v });
            }
            _ => self.err(target.span, "cannot assign to this expression"),
        }
    }

    fn expr_stmt(&mut self, e: &h::Expr, out: &mut Vec<ir::Stmt>) {
        match &e.kind {
            h::ExprKind::Builtin { name, args } => self.builtin_stmt(name, args, e.span, out),
            h::ExprKind::Method { kind: h::MethodKind::ListPush, recv, args } => {
                let mut holes = Holes::default();
                let (var, path) = self.place(recv, &mut holes);
                if !holes.args.is_empty() {
                    // appending to a copy would silently do nothing
                    self.err(recv.span, "cannot `push` to a list chosen by a variable index yet (it would only modify a copy); use a constant index");
                }
                let v = to_data(self.expr(&args[0]), &args[0].ty);
                self.flush(out);
                out.push(ir::Stmt::Append { target: var, path, value: v });
            }
            _ => {
                let v = self.expr(e);
                self.flush(out);
                out.push(ir::Stmt::Expr(v));
            }
        }
    }

    // --------------------------------------------------------------- expressions

    fn expr(&mut self, e: &h::Expr) -> ir::Expr {
        match &e.kind {
            h::ExprKind::Int(n) => ir::Expr::Int(*n),
            h::ExprKind::Bool(b) => ir::Expr::Bool(*b),
            h::ExprKind::Str(s) => ir::Expr::Data(Nbt::String(s.clone())),
            h::ExprKind::Format(_) => {
                self.err(e.span, "an f-string can only be passed to `print` for now");
                ir::Expr::Int(0)
            }
            h::ExprKind::List(_) | h::ExprKind::Struct { .. } => self.aggregate(e),
            h::ExprKind::Local(id) => ir::Expr::var(&self.local_names[id.0]),
            h::ExprKind::Global(id) => ir::Expr::global(&self.global_names[id.0]),
            h::ExprKind::Selector(_) | h::ExprKind::Rel { .. } => {
                self.err(e.span, "a selector or coordinate cannot be used as a value here");
                ir::Expr::Int(0)
            }
            h::ExprKind::Unary { op, expr } => {
                let v = self.expr(expr);
                match op {
                    AstUn::Neg => ir::Expr::negate(v),
                    AstUn::Not => ir::Expr::logical_not(v),
                }
            }
            h::ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                ir::Expr::bin(bin_op(*op), l, r)
            }
            h::ExprKind::Call { func, args } => {
                let args = args.iter().map(|a| self.expr(a)).collect();
                ir::Expr::Call(self.fn_names[func.0].clone(), args)
            }
            h::ExprKind::Builtin { name, args } => match *name {
                "exists" => match self.selector(&args[0]) {
                    Some(sel) => ir::Expr::EntityExists(sel),
                    None => ir::Expr::Bool(false),
                },
                "tag.has" => match (self.selector(&args[0]), const_str(&args[1]).and_then(|t| Word::new(&t).ok())) {
                    (Some(sel), Some(tag)) => ir::Expr::EntityExists(sel.tag(tag)),
                    _ => ir::Expr::Bool(false),
                },
                "print" | "say" => {
                    self.err(e.span, format!("`{}` does not produce a value", name));
                    ir::Expr::Int(0)
                }
                // a world-acting builtin used as a value: did the command succeed?
                other => {
                    let mut holes = Holes::default();
                    match self.builtin_command(other, args, e.span, &mut holes) {
                        Some(command) if datapack::Check::check(&command).is_ok() => ir::Expr::CommandSuccess { command, args: holes.args },
                        Some(command) => {
                            let msg = datapack::Check::check(&command).unwrap_err().message().to_owned();
                            self.err(e.span, msg);
                            ir::Expr::Bool(false)
                        }
                        None => ir::Expr::Bool(false),
                    }
                }
            },
            h::ExprKind::Method { kind, recv, .. } => match kind {
                h::MethodKind::ListLen | h::MethodKind::StrLen => {
                    let (var, path) = self.place_static(recv);
                    ir::Expr::Len(var, path)
                }
                h::MethodKind::ListPush => {
                    self.err(e.span, "`push` can only be used as a statement");
                    ir::Expr::Int(0)
                }
            },
            h::ExprKind::Field { .. } | h::ExprKind::Index { .. } => {
                let mut holes = Holes::default();
                let (var, path) = self.place(e, &mut holes);
                self.read_place(&var, &path, &holes, &e.ty)
            }
            h::ExprKind::Score { objective, holder } => ir::Expr::PlayerScore { objective: self.objective_names[objective.0].clone(), holder: self.holder(holder) },
            h::ExprKind::Error => ir::Expr::Int(0),
        }
    }

    /// A storage location for a Field/Index/variable expression. A variable index becomes a
    /// path hole (`xs[$(h0)]`) whose value is added to `holes`. A value that is not a place
    /// (a function result, a literal) is first bound to a temporary.
    fn place(&mut self, e: &h::Expr, holes: &mut Holes) -> (ir::VarRef, NbtPath) {
        match &e.kind {
            h::ExprKind::Local(id) => (ir::VarRef::Local(self.local_names[id.0].clone()), NbtPath::new()),
            h::ExprKind::Global(id) => (ir::VarRef::Global(self.global_names[id.0].clone()), NbtPath::new()),
            h::ExprKind::Field { base, name, .. } => {
                let (v, p) = self.place(base, holes);
                (v, p.key(name.clone()))
            }
            h::ExprKind::Index { base, index } => {
                let (v, p) = self.place(base, holes);
                match &index.kind {
                    h::ExprKind::Int(n) => (v, p.index(*n)),
                    _ => {
                        let hole = self.hole(holes, index);
                        (v, p.index_hole(hole))
                    }
                }
            }
            _ => {
                let t = self.fresh("tmp");
                let value = self.expr(e);
                self.pre.push(ir::Stmt::Let { name: t.clone(), kind: ir::Kind::Data, init: Some(value) });
                (ir::VarRef::Local(t), NbtPath::new())
            }
        }
    }

    /// Like [`Self::place`] for reading: a path with holes is first read into a temporary, so
    /// the result is a plain location.
    fn place_static(&mut self, e: &h::Expr) -> (ir::VarRef, NbtPath) {
        let mut holes = Holes::default();
        let (var, path) = self.place(e, &mut holes);
        if holes.args.is_empty() {
            return (var, path);
        }
        let t = self.fresh("tmp");
        self.pre.push(ir::Stmt::ReadAt { dest: t.clone(), source: var, path, args: holes.args });
        (ir::VarRef::Local(t), NbtPath::new())
    }

    /// The value at `var.path` of type `ty`; a path with holes is read through a macro first.
    fn read_place(&mut self, var: &ir::VarRef, path: &NbtPath, holes: &Holes, ty: &Ty) -> ir::Expr {
        if holes.args.is_empty() {
            return from_data(ir::Expr::Field(var.clone(), path.clone()), ty);
        }
        let t = self.fresh("tmp");
        self.pre.push(ir::Stmt::ReadAt { dest: t.clone(), source: var.clone(), path: path.clone(), args: holes.args.clone() });
        from_data(ir::Expr::var(&t), ty)
    }

    fn try_const(&self, e: &h::Expr) -> Option<Nbt> {
        Some(match &e.kind {
            h::ExprKind::Int(n) => Nbt::Int(*n),
            h::ExprKind::Bool(b) => Nbt::Int(*b as i32),
            h::ExprKind::Str(s) => Nbt::String(s.clone()),
            h::ExprKind::List(items) => Nbt::List(items.iter().map(|i| self.try_const(i)).collect::<Option<_>>()?),
            h::ExprKind::Struct { id, fields } => {
                let mut c = Compound::new();
                for ((name, _), f) in self.hir.structs[id.0].fields.iter().zip(fields) {
                    c.insert(name.clone(), self.try_const(f)?);
                }
                Nbt::Compound(c)
            }
            _ => return None,
        })
    }

    /// List and struct literals: a constant NBT value when possible, otherwise built in a temporary.
    fn aggregate(&mut self, e: &h::Expr) -> ir::Expr {
        if let Some(n) = self.try_const(e) {
            return ir::Expr::Data(n);
        }
        let t = self.fresh("tmp");
        let start = if matches!(e.kind, h::ExprKind::List(_)) { Nbt::List(vec![]) } else { Nbt::Compound(Compound::new()) };
        self.pre.push(ir::Stmt::Let { name: t.clone(), kind: ir::Kind::Data, init: Some(ir::Expr::Data(start)) });
        match &e.kind {
            h::ExprKind::List(items) => {
                for it in items {
                    let v = to_data(self.expr(it), &it.ty);
                    self.pre.push(ir::Stmt::Append { target: ir::VarRef::Local(t.clone()), path: NbtPath::new(), value: v });
                }
            }
            h::ExprKind::Struct { id, fields } => {
                for ((name, _), f) in self.hir.structs[id.0].fields.iter().zip(fields) {
                    let v = to_data(self.expr(f), &f.ty);
                    self.pre.push(ir::Stmt::SetField { target: ir::VarRef::Local(t.clone()), path: NbtPath::new().key(name.clone()), value: v });
                }
            }
            _ => unreachable!(),
        }
        ir::Expr::var(&t)
    }

    // ------------------------------------------------- builtins, selectors, context

    fn selector(&mut self, e: &h::Expr) -> Option<Selector> {
        match &e.kind {
            h::ExprKind::Selector(s) => Some(s.clone()),
            _ => {
                self.err(e.span, "expected a selector literal such as `@a[tag=x]`");
                None
            }
        }
    }

    fn holder(&mut self, e: &h::Expr) -> ScoreHolder {
        match &e.kind {
            h::ExprKind::Selector(s) => ScoreHolder::Selector(s.clone()),
            h::ExprKind::Str(s) => match HolderName::new(s) {
                Ok(n) => ScoreHolder::Name(n),
                Err(err) => {
                    self.err(e.span, err.message().to_owned());
                    ScoreHolder::All
                }
            },
            _ => {
                self.err(e.span, "a score holder must be a selector or a string literal for now (a variable name needs macros)");
                ScoreHolder::All
            }
        }
    }

    /// A coordinate: a literal or `~`/`^` is written into the command; any other integer
    /// expression becomes a hole filled at run time.
    fn coord(&mut self, e: &h::Expr, holes: &mut Holes) -> Option<Coord> {
        match &e.kind {
            h::ExprKind::Int(n) => Some(Coord::Abs(*n as f64)),
            h::ExprKind::Rel { kind, offset } => {
                let v = offset.as_deref().map_or(0.0, |o| o.parse().unwrap_or(0.0));
                Some(if *kind == RelKind::Tilde { Coord::Rel(v) } else { Coord::Local(v) })
            }
            _ if e.ty == Ty::Int => Some(Coord::Hole(self.hole(holes, e))),
            _ => {
                self.err(e.span, "expected a coordinate");
                None
            }
        }
    }

    fn pos(&mut self, es: &[h::Expr], holes: &mut Holes) -> Option<Pos> {
        let cs: Vec<Option<Coord>> = es.iter().map(|e| self.coord(e, holes)).collect();
        match cs.as_slice() {
            [Some(x), Some(y), Some(z)] => Some(Pos::new(*x, *y, *z)),
            _ => None,
        }
    }

    fn clause(&mut self, c: &h::CtxClause, holes: &mut Holes) -> Option<Clause> {
        Some(match c {
            h::CtxClause::As(e) => Clause::As(self.selector(e)?),
            h::CtxClause::At(e) => Clause::At(self.selector(e)?),
            h::CtxClause::In(s) => Clause::In(ResourceLocation::parse(s).ok()?),
            h::CtxClause::Positioned(es) => Clause::Positioned(self.checked_pos(es, holes)?),
            h::CtxClause::Facing(es) => Clause::Facing(self.checked_pos(es, holes)?),
            h::CtxClause::Rotated([a, b]) => {
                let (yaw, pitch) = (self.coord(a, holes)?, self.coord(b, holes)?);
                let r = Rotation { yaw, pitch };
                if let Err(e) = datapack::Check::check(&r) {
                    self.err(a.span.to(b.span), e.message().to_owned());
                    return None;
                }
                Clause::Rotated(r)
            }
            h::CtxClause::Align(axes) => Clause::Align(Axes { x: axes.contains('x'), y: axes.contains('y'), z: axes.contains('z') }),
            h::CtxClause::Anchored(a) => Clause::Anchored(if a == "eyes" { Anchor::Eyes } else { Anchor::Feet }),
        })
    }

    fn checked_pos(&mut self, es: &[h::Expr], holes: &mut Holes) -> Option<Pos> {
        let p = self.pos(es, holes)?;
        if let Err(e) = datapack::Check::check(&p) {
            self.err(es[0].span.to(es[2].span), e.message().to_owned());
            return None;
        }
        Some(p)
    }

    /// Validates a built command now, so a mistake is reported at the call that caused it.
    fn push_cmd(&mut self, cmd: Command, holes: Holes, span: Span, out: &mut Vec<ir::Stmt>) {
        match datapack::Check::check(&cmd) {
            Ok(()) => {
                self.flush(out);
                if holes.args.is_empty() {
                    out.push(ir::Stmt::Command(cmd));
                } else {
                    out.push(ir::Stmt::MacroCommand { command: cmd, args: holes.args });
                }
            }
            Err(e) => self.err(span, e.message().to_owned()),
        }
    }

    /// Statement form of a builtin. World-acting builtins are emitted as plain commands, so their
    /// success result costs nothing unless the program uses it (see [`Self::expr`]).
    fn builtin_stmt(&mut self, name: &str, args: &[h::Expr], span: Span, out: &mut Vec<ir::Stmt>) {
        match name {
            "print" => {
                if let h::ExprKind::Format(parts) = &args[0].kind {
                    let parts = parts
                        .iter()
                        .map(|p| match p {
                            h::FormatPart::Lit(s) => ir::PrintPart::Lit(s.clone()),
                            h::FormatPart::Expr(e) => ir::PrintPart::Value(self.expr(e)),
                        })
                        .collect();
                    self.flush(out);
                    out.push(ir::Stmt::PrintParts(parts));
                    return;
                }
                let v = self.expr(&args[0]);
                self.flush(out);
                out.push(ir::Stmt::Print(v));
            }
            "say" => self.push_cmd(Command::Say(const_str(&args[0]).unwrap_or_default()), Holes::default(), span, out),
            // value-producing builtins used as statements have no effect
            "exists" | "tag.has" => {}
            other => {
                let mut holes = Holes::default();
                match self.builtin_command(other, args, span, &mut holes) {
                    Some(cmd) => self.push_cmd(cmd, holes, span, out),
                    None => {
                        if !self.errors.is_empty() {
                            return;
                        }
                        self.err(span, format!("the builtin `{other}` is not supported by the backend yet"));
                    }
                }
            }
        }
    }

    /// The command a world-acting builtin runs. Run-time values go into `holes`.
    fn builtin_command(&mut self, name: &str, args: &[h::Expr], span: Span, holes: &mut Holes) -> Option<Command> {
        let _ = span;
        let s = |i: usize| const_str(&args[i]).unwrap_or_default();
        let n = |i: usize| match args.get(i).map(|a| &a.kind) {
            Some(h::ExprKind::Int(n)) => Some(*n),
            _ => None,
        };
        let rl = |this: &mut Self, i: usize| match ResourceLocation::parse(&s(i)) {
            Ok(r) => Some(r),
            Err(e) => {
                this.err(args[i].span, e.message().to_owned());
                None
            }
        };
        Some(match name {
            "kill" => Command::Kill(Some(self.selector(&args[0])?)),
            "tag.add" | "tag.remove" => {
                let (sel, tag) = (self.selector(&args[0])?, Word::new(&s(1)).ok()?);
                if name == "tag.add" {
                    self.added_tags.insert(tag.as_str().to_owned());
                }
                let action = if name == "tag.add" { TagAction::Add(tag) } else { TagAction::Remove(tag) };
                Command::Tag { targets: sel, action }
            }
            "effect.give" => {
                let (sel, id) = (self.selector(&args[0])?, rl(self, 1)?);
                // the duration argument is in ticks; the game takes whole seconds, rounded up
                let seconds = match n(2) {
                    Some(t) => Num::Lit((t.max(1) as u32).div_ceil(20)),
                    None => {
                        let ticks = self.expr(&args[2]);
                        let secs = ir::Expr::bin(ir::BinOp::Div, ir::Expr::bin(ir::BinOp::Add, ticks, ir::Expr::Int(19)), ir::Expr::Int(20));
                        Num::Hole(self.hole_ir(holes, secs))
                    }
                };
                let amplifier = args.get(3).map(|a| match n(3) {
                    Some(v) => Num::Lit(v.max(0) as u32),
                    None => Num::Hole(self.hole(holes, a)),
                });
                Command::Effect(Effect::Give { targets: sel, effect: id, seconds: Some(seconds), amplifier, hide_particles: false })
            }
            "effect.clear" => Command::Effect(Effect::Clear { targets: Some(self.selector(&args[0])?), effect: None }),
            "tp" => {
                let sel = self.selector(&args[0])?;
                Command::Teleport { targets: sel, dest: self.pos(&args[1..4], holes)? }
            }
            "setblock" => {
                let pos = self.pos(&args[0..3], holes)?;
                Command::Setblock { pos, block: datapack::BlockState::new(rl(self, 3)?), mode: None }
            }
            "summon" => {
                let id = rl(self, 0)?;
                let pos = if args.len() == 4 { Some(self.pos(&args[1..4], holes)?) } else { None };
                // every summoned entity is tagged as ours so `_uninstall` can remove it; an NBT
                // argument needs an explicit position, and `~ ~ ~` is "here"
                self.uses_summon = true;
                let nbt = Compound::new().with("Tags", vec![Nbt::str(self.owned_tag())]);
                Command::Summon { entity: id, pos: Some(pos.unwrap_or_else(Pos::here)), nbt: Some(nbt) }
            }
            _ => return None,
        })
    }
}

fn const_str(e: &h::Expr) -> Option<String> {
    match &e.kind {
        h::ExprKind::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// Wraps a scalar in the conversion needed to store it in an NBT container.
fn to_data(e: ir::Expr, ty: &Ty) -> ir::Expr {
    if is_scalar(ty) { ir::Expr::ToData(Box::new(e)) } else { e }
}

/// Reads a scalar back out of an NBT container.
fn from_data(e: ir::Expr, ty: &Ty) -> ir::Expr {
    if is_scalar(ty) { ir::Expr::ToInt(Box::new(e)) } else { e }
}

/// Whether the statement contains a `break` / `continue` that targets the enclosing loop
/// (nested loops have their own).
fn jumps(s: &h::Stmt) -> (bool, bool) {
    match s {
        h::Stmt::Break => (true, false),
        h::Stmt::Continue => (false, true),
        h::Stmt::If { then, els, .. } => {
            let (a, b) = jumps_block(then);
            let (c, d) = jumps_block(els);
            (a || c, b || d)
        }
        h::Stmt::Block(b) => jumps_block(b),
        _ => (false, false),
    }
}

fn jumps_block(b: &h::Block) -> (bool, bool) {
    b.stmts.iter().map(jumps).fold((false, false), |(a, b), (c, d)| (a || c, b || d))
}
