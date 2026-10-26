//! HIR -> IR. Everything here has already been type checked; this pass only decides how checked
//! constructs map onto the IR's runtime model (docs/lang/ir.md): `int`/`bool` on the scoreboard,
//! `str`/`list`/structs in command storage.
//!
//! Notable translations (docs/lang/codegen.md):
//! * module-qualified names become flat `[a-z0-9_]` IR names (`maps::arena::start` -> `maps_arena__start`);
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
use crate::diag::{Diagnostic, SourceMap, Span};
use crate::ir;
use crate::sema::hir as h;
use crate::sema::ty::Ty;
use crate::syntax::ast::{AssignOp, BinOp as AstBin, UnOp as AstUn};
use crate::syntax::token::RelKind;

/// What code generation needs besides the program.
pub struct GenOptions<'a> {
    /// For the `file:line` of an `unwrap` that fails.
    pub sources: &'a SourceMap,
    /// A failed `unwrap` also stops the pack with `datapack disable`.
    pub abort_on_unwrap: bool,
    /// The pack id for `datapack disable`, such as `file/my_pack`.
    pub pack_id: String,
}

/// Translates a checked program. Errors are things the language accepts but the backend cannot
/// express yet (variable coordinates, variable list indices, ...).
pub fn generate(hir: &h::Hir, namespace: &str, description: &str, opts: &GenOptions) -> Result<ir::Program, Vec<Diagnostic>> {
    let mut g = Gen::new(hir, namespace, opts);
    let mut program = ir::Program::new(namespace);
    program.description = description.to_owned();
    program.objectives = g.objectives();
    program.globals = g.globals();
    // functions of the standard library are only generated when something reaches them
    let mut reached: Vec<bool> = hir.fns.iter().map(|f| !f.std).collect();
    let mut work: Vec<usize> = (0..hir.fns.len()).filter(|&i| reached[i]).collect();
    while let Some(i) = work.pop() {
        let mut calls = Vec::new();
        hir.fns[i].body.calls(&mut calls);
        for c in calls {
            if !std::mem::replace(&mut reached[c.0], true) {
                work.push(c.0);
            }
        }
    }
    for i in (0..hir.fns.len()).filter(|&i| reached[i]) {
        g.function(i, &mut program.functions);
    }
    g.watchdog();
    program.functions.append(&mut g.extra_fns);
    program.globals.append(&mut g.extra_globals);
    program.uninstall = g.uninstall_commands();
    if g.entities {
        program.entities = Some(ir::EntityConfig { objective: g.id_objective(), owned_tag: g.owned_tag() });
    }
    if g.errors.is_empty() { Ok(program) } else { Err(g.errors) }
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c.to_ascii_lowercase() } else { '_' }).collect()
}

/// `maps::arena` -> `maps_arena__`; the root module has no prefix.
fn module_prefix(module: &str) -> String {
    if module == "main" { String::new() } else { format!("{}__", sanitize(module)) }
}

fn unique(used: &mut HashSet<String>, base: String) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    (2..).map(|n| format!("{base}_{n}")).find(|c| used.insert(c.clone())).expect("infinite iterator")
}

/// An arm ready to be placed: its condition (`None`: always), the statements run when it is
/// chosen (bindings first), and its guard with the statements the guard needs.
type ArmCode = (Option<ir::Expr>, Vec<ir::Stmt>, Option<(Vec<ir::Stmt>, ir::Expr)>);

/// The value a pattern is matched against, readable as often as needed.
#[derive(Clone)]
enum MatchVal {
    /// An int-like value (int, bool, entity handle) as an expression.
    Score(ir::Expr),
    /// A stored value: a variable and the path inside it.
    Data(ir::VarRef, NbtPath),
}

impl MatchVal {
    fn read(&self, ty: &Ty) -> ir::Expr {
        match self {
            MatchVal::Score(e) => e.clone(),
            MatchVal::Data(var, path) => from_data(ir::Expr::Field(var.clone(), path.clone()), ty),
        }
    }
}

fn kind_of(ty: &Ty) -> ir::Kind {
    match ty {
        // an entity handle is an int id
        Ty::Int | Ty::Entity => ir::Kind::Int,
        Ty::Bool => ir::Kind::Bool,
        _ => ir::Kind::Data,
    }
}

fn is_scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Bool | Ty::Entity)
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
    /// The program uses entity handles (needs the id objective and helper functions).
    entities: bool,
    /// Tags the program adds to entities (`tag.add`), removed again by `_uninstall`.
    added_tags: BTreeSet<String>,
    fn_names: Vec<String>,
    global_names: Vec<String>,
    objective_names: Vec<String>,
    errors: Vec<Diagnostic>,
    extra_fns: Vec<ir::Function>,
    extra_globals: Vec<ir::Global>,
    used_fns: HashSet<String>,
    /// Entry points and the global that says whether a run reached its end (`_watchdog`).
    watched: Vec<(String, String)>,
    used_globals: HashSet<String>,
    // --- per function
    cur_fn: usize,
    local_names: Vec<String>,
    used: HashSet<String>,
    pre: Vec<ir::Stmt>,
    loops: Vec<LoopCtx>,
    /// How many context blocks (`as @a { .. }`) the code being generated is inside: a `return`
    /// cannot leave one.
    ctx_depth: usize,
    sources: &'a SourceMap,
    abort_on_unwrap: bool,
    pack_id: String,
}

impl<'a> Gen<'a> {
    fn new(hir: &'a h::Hir, namespace: &str, opts: &GenOptions<'a>) -> Self {
        let mut used_fns = HashSet::from(["_init", "_uninstall", "_alloc_id", "_new_entity", "_ensure_id", "_watchdog"].map(str::to_owned));
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
            entities: false,
            added_tags: BTreeSet::new(),
            fn_names,
            global_names,
            objective_names,
            errors: Vec::new(),
            extra_fns: Vec::new(),
            extra_globals: Vec::new(),
            used_fns,
            watched: Vec::new(),
            used_globals,
            cur_fn: 0,
            local_names: Vec::new(),
            used: HashSet::new(),
            pre: Vec::new(),
            loops: Vec::new(),
            ctx_depth: 0,
            sources: opts.sources,
            abort_on_unwrap: opts.abort_on_unwrap,
            pack_id: opts.pack_id.clone(),
        }
    }

    /// A new hole in the command being built, filled at run time with `v`. The value may be read
    /// again (`xs[i] += 1` reads and writes the same element), so anything but a literal or a
    /// variable is first bound to a temporary.
    fn hole_ir(&mut self, holes: &mut Holes, v: ir::Expr) -> Hole {
        self.hole_of(holes, v, ir::Kind::Int)
    }

    /// Like [`Self::hole_ir`] for a string value.
    fn hole_str(&mut self, holes: &mut Holes, v: ir::Expr) -> Hole {
        self.hole_of(holes, v, ir::Kind::Data)
    }

    fn hole_of(&mut self, holes: &mut Holes, v: ir::Expr, kind: ir::Kind) -> Hole {
        let v = match v {
            ir::Expr::Int(_) | ir::Expr::Var(_) | ir::Expr::Global(_) => v,
            other => {
                let t = self.fresh("hv");
                self.pre.push(ir::Stmt::Let { name: t.clone(), kind, init: Some(other) });
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

    /// The objective that holds entity ids.
    fn id_objective(&self) -> String {
        format!("{}_id", self.namespace)
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
                cmds.push(Command::Tag { targets: Selector::e().tag(tag.clone()), action: TagAction::Remove(tag.into()) });
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
            // never stored: a `()` payload has no field
            h::ConstVal::Unit => Nbt::Compound(Compound::new()),
            h::ConstVal::Enum(_, index, vals) => {
                let mut c = Compound::new();
                c.insert("t".to_owned(), Nbt::Int(*index as i32));
                for (i, val) in vals.iter().enumerate() {
                    if !matches!(val, h::ConstVal::Unit) {
                        c.insert(format!("f{i}"), self.const_nbt(val));
                    }
                }
                Nbt::Compound(c)
            }
        }
    }

    /// `_watchdog`, run every tick: tells the players when an entry point was cut off by the
    /// command limit (its flag is still set) and counts the reports in `_cutoffs`.
    fn watchdog(&mut self) {
        if self.watched.is_empty() {
            return;
        }
        let counter = unique(&mut self.used_globals, "_cutoffs".to_owned());
        self.extra_globals.push(ir::Global { name: counter.clone(), kind: ir::Kind::Int, init: ir::Expr::Int(0) });
        let mut body = Vec::new();
        for (name, flag) in self.watched.clone() {
            let message = format!("slab: `{name}` was cut off: the game stops a function after 65536 commands in one tick. Split the work over several ticks or do less per call.");
            body.push(ir::Stmt::If {
                cond: ir::Expr::bin(ir::BinOp::Eq, ir::Expr::Global(flag.clone()), ir::Expr::Int(1)),
                then: vec![
                    ir::Stmt::Assign { target: ir::VarRef::Global(flag), value: ir::Expr::Int(0) },
                    ir::Stmt::Assign { target: ir::VarRef::Global(counter.clone()), value: ir::Expr::bin(ir::BinOp::Add, ir::Expr::Global(counter.clone()), ir::Expr::Int(1)) },
                    ir::Stmt::PrintParts(vec![ir::PrintPart::Lit(message)]),
                ],
                els: Vec::new(),
            });
        }
        self.extra_fns.push(ir::Function { name: "_watchdog".to_owned(), params: Vec::new(), ret: None, body, entry: Some(ir::Entry::Tick) });
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
        if f.entry.is_some() && !f.std {
            // The game stops a function chain after 65,536 commands without saying so. An entry
            // point sets a flag before its body and clears it after; a flag still set later means
            // the body never reached its end (`_watchdog` reports it).
            let run = unique(&mut self.used_fns, format!("{name}__run"));
            let flag = unique(&mut self.used_globals, format!("{name}__cut"));
            self.extra_globals.push(ir::Global { name: flag.clone(), kind: ir::Kind::Int, init: ir::Expr::Int(0) });
            let set = |v: i32| ir::Stmt::Assign { target: ir::VarRef::Global(flag.clone()), value: ir::Expr::Int(v) };
            out.push(ir::Function { name: run.clone(), params: Vec::new(), ret: None, body, entry: None });
            out.push(ir::Function { name: name.clone(), params, ret: None, body: vec![set(1), ir::Stmt::Expr(ir::Expr::call(&run, vec![])), set(0)], entry });
            self.watched.push((f.name.clone(), flag));
        } else {
            out.push(ir::Function { name: name.clone(), params, ret: (f.ret != Ty::Unit).then(|| kind_of(&f.ret)), body, entry });
        }
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
                self.ctx_depth += 1;
                let body = self.block(body);
                self.ctx_depth -= 1;
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
                    v = if target.ty == Ty::Str { self.concat(vec![cur, v]) } else { ir::Expr::bin(bop, cur, v) };
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
                    v = if target.ty == Ty::Str { self.concat(vec![cur, v]) } else { ir::Expr::bin(bop, cur, v) };
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
                let mut holes = Holes::default();
                let holder = self.holder(holder, &mut holes);
                let v = self.expr(value);
                self.flush(out);
                out.push(ir::Stmt::SetScore { objective: obj, holder, op: bop, value: v, args: holes.args });
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
            h::ExprKind::Match { scrut, arms } => {
                self.match_value(e, scrut, arms);
                self.flush(out);
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
            h::ExprKind::Format(parts) => {
                let parts = parts
                    .iter()
                    .map(|p| match p {
                        h::FormatPart::Lit(s) => ir::Expr::Data(Nbt::String(s.clone())),
                        h::FormatPart::Expr(e) => self.text_part(e),
                    })
                    .collect();
                self.concat(parts)
            }
            h::ExprKind::List(_) | h::ExprKind::Struct { .. } | h::ExprKind::Variant { .. } => self.aggregate(e),
            // only ever the payload of a variant, which has no field for it
            h::ExprKind::Unit => ir::Expr::Int(0),
            h::ExprKind::Local(id) => ir::Expr::var(&self.local_names[id.0]),
            h::ExprKind::Global(id) => ir::Expr::global(&self.global_names[id.0]),
            h::ExprKind::Selector(..) | h::ExprKind::Rel { .. } => {
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
            h::ExprKind::Binary { op: AstBin::Add, lhs, rhs } if lhs.ty == Ty::Str => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                self.concat(vec![l, r])
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
            h::ExprKind::Match { scrut, arms } => self.match_value(e, scrut, arms),
            h::ExprKind::Builtin { name, args } => match *name {
                "exists" | "tag.has" => {
                    let mut holes = Holes::default();
                    let Some(mut sel) = self.target(&args[0], &mut holes) else { return ir::Expr::Bool(false) };
                    if *name == "tag.has" {
                        match const_str(&args[1]).and_then(|t| Word::new(&t).ok()) {
                            Some(tag) => sel = sel.tag(tag),
                            None => return ir::Expr::Bool(false),
                        }
                    }
                    if holes.args.is_empty() {
                        ir::Expr::EntityExists(sel)
                    } else {
                        // an entity value is found by its id, which is only known at run time
                        match Command::execute().if_(datapack::Condition::Entity(sel)).into_test() {
                            Ok(command) => ir::Expr::CommandSuccess { command: Box::new(command), args: holes.args },
                            Err(err) => {
                                self.err(e.span, err.message().to_owned());
                                ir::Expr::Bool(false)
                            }
                        }
                    }
                }
                // `summon` as a value is the new entity's handle
                "summon" => {
                    let mut holes = Holes::default();
                    match self.new_entity_command(args, e.span, &mut holes) {
                        Some(command) => ir::Expr::NewEntity { command: Box::new(command), args: holes.args },
                        None => ir::Expr::Int(0),
                    }
                }
                // `Result<int, ()>`: Ok(the command's result) when it succeeded, else Err(())
                "run_command" => {
                    let mut holes = Holes::default();
                    let Some(command) = self.builtin_command(name, args, e.span, &mut holes) else { return ir::Expr::Int(0) };
                    let (ok, value, out) = (self.fresh("ok"), self.fresh("val"), self.fresh("res"));
                    self.pre.push(ir::Stmt::Let { name: ok.clone(), kind: ir::Kind::Int, init: None });
                    self.pre.push(ir::Stmt::Let { name: value.clone(), kind: ir::Kind::Int, init: None });
                    self.pre.push(ir::Stmt::StoreResult { success: ok.clone(), result: value.clone(), command, args: holes.args });
                    // Result::Err is variant 1 (no payload for `()`), Result::Ok is variant 0
                    self.pre.push(ir::Stmt::Let { name: out.clone(), kind: ir::Kind::Data, init: Some(ir::Expr::Data(Nbt::Compound(Compound::new().with("t", 1)))) });
                    let then = vec![
                        ir::Stmt::Assign { target: ir::VarRef::Local(out.clone()), value: ir::Expr::Data(Nbt::Compound(Compound::new().with("t", 0))) },
                        ir::Stmt::SetField { target: ir::VarRef::Local(out.clone()), path: NbtPath::new().key("f0"), value: ir::Expr::ToData(Box::new(ir::Expr::var(&value))) },
                    ];
                    self.pre.push(ir::Stmt::If { cond: ir::Expr::bin(ir::BinOp::Ne, ir::Expr::var(&ok), ir::Expr::Int(0)), then, els: Vec::new() });
                    ir::Expr::var(&out)
                }
                "print" | "say" => {
                    self.err(e.span, format!("`{}` does not produce a value", name));
                    ir::Expr::Int(0)
                }
                // a world-acting builtin used as a value: did the command succeed?
                other => {
                    let mut holes = Holes::default();
                    match self.builtin_command(other, args, e.span, &mut holes) {
                        Some(command) if datapack::Check::check(&command).is_ok() => ir::Expr::CommandSuccess { command: Box::new(command), args: holes.args },
                        Some(command) => {
                            let msg = datapack::Check::check(&command).unwrap_err().message().to_owned();
                            self.err(e.span, msg);
                            ir::Expr::Bool(false)
                        }
                        None => ir::Expr::Bool(false),
                    }
                }
            },
            h::ExprKind::Method { kind, recv, args } => match kind {
                h::MethodKind::Intrinsic(id) => self.method_intrinsic(id, recv, args, e.span),
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
            h::ExprKind::Score { objective, holder } => {
                let mut holes = Holes::default();
                let holder = self.holder(holder, &mut holes);
                ir::Expr::PlayerScore { objective: self.objective_names[objective.0].clone(), holder, args: holes.args }
            }
            h::ExprKind::ToEntity(inner) => {
                let mut holes = Holes::default();
                match self.selector(inner, &mut holes) {
                    Some(selector) => {
                        self.entities = true;
                        ir::Expr::EntityOf { selector, args: holes.args }
                    }
                    None => ir::Expr::Int(0),
                }
            }
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
            h::ExprKind::Variant { index, args, .. } => {
                let mut c = Compound::new();
                c.insert("t".to_owned(), Nbt::Int(*index as i32));
                for (i, a) in args.iter().enumerate() {
                    if a.ty != Ty::Unit {
                        c.insert(format!("f{i}"), self.try_const(a)?);
                    }
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
        let start = match &e.kind {
            h::ExprKind::List(_) => Nbt::List(vec![]),
            h::ExprKind::Variant { index, .. } => Nbt::Compound(Compound::new().with("t", *index as i32)),
            _ => Nbt::Compound(Compound::new()),
        };
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
            h::ExprKind::Variant { args, .. } => {
                for (i, a) in args.iter().enumerate() {
                    if a.ty == Ty::Unit {
                        continue;
                    }
                    let v = to_data(self.expr(a), &a.ty);
                    self.pre.push(ir::Stmt::SetField { target: ir::VarRef::Local(t.clone()), path: NbtPath::new().key(format!("f{i}")), value: v });
                }
            }
            _ => unreachable!(),
        }
        ir::Expr::var(&t)
    }

    // ------------------------------------------------- builtins, selectors, context

    // ------------------------------------------------ methods of Option and Result

    /// A stored value as a variable: a variable or global as it is, anything else bound to a
    /// temporary first.
    fn data_var(&mut self, v: ir::Expr) -> ir::VarRef {
        match v {
            ir::Expr::Var(n) => ir::VarRef::Local(n),
            ir::Expr::Global(n) => ir::VarRef::Global(n),
            other => {
                let t = self.fresh("recv");
                self.pre.push(ir::Stmt::Let { name: t.clone(), kind: ir::Kind::Data, init: Some(other) });
                ir::VarRef::Local(t)
            }
        }
    }

    fn tag_of(r: &ir::VarRef) -> ir::Expr {
        ir::Expr::ToInt(Box::new(ir::Expr::Field(r.clone(), NbtPath::new().key("t"))))
    }

    fn is_variant(r: &ir::VarRef, index: i32) -> ir::Expr {
        ir::Expr::bin(ir::BinOp::Eq, Self::tag_of(r), ir::Expr::Int(index))
    }

    fn payload_field(r: &ir::VarRef, i: usize) -> ir::Expr {
        ir::Expr::Field(r.clone(), NbtPath::new().key(format!("f{i}")))
    }

    /// `file.slab:line` of a source position, for messages shown to the player.
    fn location(&self, span: Span) -> String {
        let module = self.hir.fns[self.cur_fn].module;
        let name = format!("{}.slab", self.hir.modules[module].replace("::", "/"));
        let line = self.sources.files.get(self.hir.files[module]).map(|f| f.line_col(span.start as usize).0);
        match line {
            Some(l) => format!("{name}:{l}"),
            None => name,
        }
    }

    /// The value a function returns after a failed `unwrap`: zero of its return type.
    fn default_nbt(&self, ty: &Ty, depth: usize) -> Nbt {
        match ty {
            Ty::Str => Nbt::String(String::new()),
            Ty::List(_) => Nbt::List(Vec::new()),
            Ty::Struct(id) => {
                let mut c = Compound::new();
                for (name, t) in &self.hir.structs[id.0].fields {
                    c.insert(name.clone(), self.default_nbt(t, depth + 1));
                }
                Nbt::Compound(c)
            }
            Ty::Enum(id, targs) if depth < 6 => {
                let mut c = Compound::new();
                c.insert("t".to_owned(), Nbt::Int(0));
                for (i, f) in self.hir.enums[id.0].variants[0].fields.iter().enumerate() {
                    let t = f.subst(targs);
                    if t != Ty::Unit {
                        c.insert(format!("f{i}"), self.default_nbt(&t, depth + 1));
                    }
                }
                Nbt::Compound(c)
            }
            Ty::Enum(..) => Nbt::Compound(Compound::new().with("t", 0)),
            _ => Nbt::Int(0),
        }
    }

    fn default_ir(&self, ty: &Ty) -> ir::Expr {
        match ty {
            Ty::Bool => ir::Expr::Bool(false),
            t if is_scalar(t) => ir::Expr::Int(0),
            t => ir::Expr::Data(self.default_nbt(t, 0)),
        }
    }

    /// `unwrap` / `expect` on a value that is not what it needs to be: tell the players, stop
    /// the pack if asked to, and leave the function (not from inside a context block, which
    /// cannot be left; the code after goes on with a zero value).
    fn unwrap_check(&mut self, bad: ir::Expr, what: &str, message: Option<ir::Expr>, span: Span) {
        let at = self.location(span);
        let parts = match message {
            None => vec![ir::PrintPart::Lit(format!("slab: unwrap on {what} ({at})"))],
            Some(m) => vec![ir::PrintPart::Lit("slab: ".to_owned()), ir::PrintPart::Value(m), ir::PrintPart::Lit(format!(" ({at})"))],
        };
        let mut then = vec![ir::Stmt::PrintParts(parts)];
        if self.abort_on_unwrap {
            then.push(ir::Stmt::Command(Command::DatapackDisable(self.pack_id.clone())));
        }
        if self.ctx_depth == 0 {
            let ret = self.hir.fns[self.cur_fn].ret.clone();
            then.push(ir::Stmt::Return(if ret == Ty::Unit { None } else { Some(self.default_ir(&ret)) }));
        }
        self.pre.push(ir::Stmt::If { cond: bad, then, els: Vec::new() });
    }

    // ------------------------------------------------ match

    /// A pattern against a value: the condition for it to fit (`None`: always) and the bindings
    /// it makes, each a local with the expression that reads its value.
    fn pattern_test(&self, p: &h::Pat, val: &MatchVal, ty: &Ty, binds: &mut Vec<(h::LocalId, ir::Expr)>) -> Option<ir::Expr> {
        let and = |a: Option<ir::Expr>, b: Option<ir::Expr>| match (a, b) {
            (Some(a), Some(b)) => Some(ir::Expr::bin(ir::BinOp::And, a, b)),
            (a, None) => a,
            (None, b) => b,
        };
        match p {
            h::Pat::Wild => None,
            h::Pat::Bind(l) => {
                binds.push((*l, val.read(ty)));
                None
            }
            h::Pat::Int(n) => Some(ir::Expr::bin(ir::BinOp::Eq, val.read(ty), ir::Expr::Int(*n))),
            h::Pat::Bool(b) => Some(ir::Expr::bin(ir::BinOp::Eq, val.read(ty), ir::Expr::Int(*b as i32))),
            h::Pat::Str(s) => Some(ir::Expr::bin(ir::BinOp::Eq, val.read(ty), ir::Expr::Data(Nbt::str(s.clone())))),
            h::Pat::Variant { index, fields } => {
                let MatchVal::Data(var, path) = val else { return None };
                let Ty::Enum(id, targs) = ty else { return None };
                let mut cond = Some(ir::Expr::bin(
                    ir::BinOp::Eq,
                    ir::Expr::ToInt(Box::new(ir::Expr::Field(var.clone(), path.clone().key("t")))),
                    ir::Expr::Int(*index as i32),
                ));
                for (i, (sub, fty)) in fields.iter().zip(&self.hir.enums[id.0].variants[*index].fields).enumerate() {
                    let fty = fty.subst(targs);
                    if fty == Ty::Unit {
                        continue;
                    }
                    let at = path.clone().key(format!("f{i}"));
                    let inner = if is_scalar(&fty) {
                        MatchVal::Score(ir::Expr::ToInt(Box::new(ir::Expr::Field(var.clone(), at))))
                    } else {
                        MatchVal::Data(var.clone(), at)
                    };
                    cond = and(cond, self.pattern_test(sub, &inner, &fty, binds));
                }
                cond
            }
            h::Pat::Or(alts) => {
                let mut out: Option<Option<ir::Expr>> = None;
                for a in alts {
                    let c = self.pattern_test(a, val, ty, &mut Vec::new());
                    out = Some(match (out, c) {
                        (None, c) => c,
                        (Some(Some(x)), Some(c)) => Some(ir::Expr::bin(ir::BinOp::Or, x, c)),
                        _ => None,
                    });
                }
                out.flatten()
            }
        }
    }

    /// `match` / `if let`: arms tried in order. Without guards it becomes an `if / else` chain;
    /// with guards, a flag says whether an arm already ran. A match used as a value assigns the
    /// chosen arm's value to a temporary.
    fn match_value(&mut self, e: &h::Expr, scrut: &h::Expr, arms: &[h::MatchArm]) -> ir::Expr {
        let sv = self.expr(scrut);
        let val = if is_scalar(&scrut.ty) {
            let name = match sv {
                ir::Expr::Var(n) => n,
                other => {
                    let t = self.fresh("scrut");
                    self.pre.push(ir::Stmt::Let { name: t.clone(), kind: kind_of(&scrut.ty), init: Some(other) });
                    t
                }
            };
            MatchVal::Score(ir::Expr::var(&name))
        } else {
            MatchVal::Data(self.data_var(sv), NbtPath::new())
        };
        let result = (e.ty != Ty::Unit).then(|| self.fresh("m"));
        if let Some(r) = &result {
            self.pre.push(ir::Stmt::Let { name: r.clone(), kind: kind_of(&e.ty), init: Some(self.default_ir(&e.ty)) });
        }
        let saved = std::mem::take(&mut self.pre);
        let guarded = arms.iter().any(|a| a.guard.is_some());
        let done = guarded.then(|| self.fresh("done"));
        // each arm: its condition, and what runs when it is chosen
        let mut parts: Vec<ArmCode> = Vec::new();
        for arm in arms {
            let mut binds = Vec::new();
            let cond = self.pattern_test(&arm.pat, &val, &scrut.ty, &mut binds);
            let mut run: Vec<ir::Stmt> = binds
                .into_iter()
                .map(|(l, read)| {
                    let info = &self.hir.fns[self.cur_fn].locals[l.0];
                    ir::Stmt::Let { name: self.local_names[l.0].clone(), kind: kind_of(&info.ty), init: Some(read) }
                })
                .collect();
            let guard = arm.guard.as_ref().map(|g| self.expr_pre(g));
            run.extend(self.block(&arm.body));
            if let (Some(r), Some(v)) = (&result, &arm.value) {
                let (pre, v) = self.expr_pre(v);
                run.extend(pre);
                run.push(ir::Stmt::Assign { target: ir::VarRef::Local(r.clone()), value: v });
            }
            parts.push((cond, run, guard));
        }
        self.pre = saved;
        match done {
            None => {
                let mut chain: Vec<ir::Stmt> = Vec::new();
                let last = parts.len().saturating_sub(1);
                for (i, (cond, run, _)) in parts.into_iter().enumerate().rev() {
                    // the last arm of an exhaustive match needs no test (the checker proved it)
                    chain = match cond {
                        Some(c) if i != last => vec![ir::Stmt::If { cond: c, then: run, els: chain }],
                        _ => run,
                    };
                }
                self.pre.extend(chain);
            }
            Some(done) => {
                self.pre.push(ir::Stmt::Let { name: done.clone(), kind: ir::Kind::Int, init: Some(ir::Expr::Int(0)) });
                let set_done = ir::Stmt::Assign { target: ir::VarRef::Local(done.clone()), value: ir::Expr::Int(1) };
                for (cond, run, guard) in parts {
                    let not_done = ir::Expr::bin(ir::BinOp::Eq, ir::Expr::var(&done), ir::Expr::Int(0));
                    let cond = match cond {
                        Some(c) => ir::Expr::bin(ir::BinOp::And, not_done, c),
                        None => not_done,
                    };
                    // the bindings come first, then the guard sees them; `run` starts with them
                    let n_binds = run.iter().take_while(|s| matches!(s, ir::Stmt::Let { .. })).count();
                    let (binds, rest) = run.split_at(n_binds);
                    let mut then = binds.to_vec();
                    let chosen: Vec<ir::Stmt> = std::iter::once(set_done.clone()).chain(rest.iter().cloned()).collect();
                    match guard {
                        Some((pre, g)) => {
                            then.extend(pre);
                            then.push(ir::Stmt::If { cond: g, then: chosen, els: Vec::new() });
                        }
                        None => then.extend(chosen),
                    }
                    self.pre.push(ir::Stmt::If { cond, then, els: Vec::new() });
                }
            }
        }
        match result {
            Some(r) => ir::Expr::var(&r),
            None => ir::Expr::Int(0),
        }
    }

    /// The methods of `Option` and `Result` that the standard library declares as intrinsics.
    fn method_intrinsic(&mut self, id: &str, recv: &h::Expr, args: &[h::Expr], span: Span) -> ir::Expr {
        if id == "entity.selector" {
            // the text of the selector that finds this handle: `@e[scores={<ns>_id=N},limit=1]`
            let n = self.expr(recv);
            let objective = self.id_objective();
            return self.concat(vec![ir::Expr::Data(Nbt::str(format!("@e[scores={{{objective}="))), n, ir::Expr::Data(Nbt::str("},limit=1]"))]);
        }
        let Ty::Enum(_, targs) = &recv.ty else {
            self.err(span, "an Option or Result method needs an Option or a Result");
            return ir::Expr::Int(0);
        };
        let targs = targs.clone();
        let v = self.expr(recv);
        let r = self.data_var(v);
        // the variant that carries the value, and its name for messages: Some / Ok
        let (good, what) = if id.starts_with("option.") { (1, "None") } else { (0, "Err") };
        match id {
            "option.is_some" | "result.is_err" => Self::is_variant(&r, 1),
            "option.is_none" | "result.is_ok" => Self::is_variant(&r, 0),
            "option.unwrap" | "result.unwrap" | "option.expect" | "result.expect" => {
                let message = if id.ends_with("expect") { Some(self.expr(&args[0])) } else { None };
                self.unwrap_check(ir::Expr::logical_not(Self::is_variant(&r, good)), what, message, span);
                if targs[0] == Ty::Unit { ir::Expr::Int(0) } else { from_data(Self::payload_field(&r, 0), &targs[0]) }
            }
            "option.unwrap_or" | "result.unwrap_or" => {
                let t = targs[0].clone();
                let default = self.expr(&args[0]);
                let out = self.fresh("or");
                self.pre.push(ir::Stmt::Let { name: out.clone(), kind: kind_of(&t), init: Some(default) });
                let take = ir::Stmt::Assign { target: ir::VarRef::Local(out.clone()), value: from_data(Self::payload_field(&r, 0), &t) };
                self.pre.push(ir::Stmt::If { cond: Self::is_variant(&r, good), then: vec![take], els: Vec::new() });
                ir::Expr::var(&out)
            }
            "result.ok" | "result.err" => {
                // Ok(x).ok() is Some(x), Err(e).err() is Some(e); anything else is None
                let (variant, t) = if id == "result.ok" { (0, targs[0].clone()) } else { (1, targs[1].clone()) };
                let out = self.fresh("opt");
                self.pre.push(ir::Stmt::Let { name: out.clone(), kind: ir::Kind::Data, init: Some(ir::Expr::Data(Nbt::Compound(Compound::new().with("t", 0)))) });
                let mut then = vec![ir::Stmt::Assign { target: ir::VarRef::Local(out.clone()), value: ir::Expr::Data(Nbt::Compound(Compound::new().with("t", 1))) }];
                if t != Ty::Unit {
                    then.push(ir::Stmt::SetField { target: ir::VarRef::Local(out.clone()), path: NbtPath::new().key("f0"), value: Self::payload_field(&r, 0) });
                }
                self.pre.push(ir::Stmt::If { cond: Self::is_variant(&r, variant), then, els: Vec::new() });
                ir::Expr::var(&out)
            }
            other => {
                self.err(span, format!("the method `{other}` is not supported by the backend yet"));
                ir::Expr::Int(0)
            }
        }
    }

    /// A target for a command: a selector literal, or an entity handle, which selects the entity
    /// whose id is the handle's value (a hole filled at run time).
    fn target(&mut self, e: &h::Expr, holes: &mut Holes) -> Option<Selector> {
        match &e.kind {
            h::ExprKind::Selector(s, hs) => Some(self.fill_selector(s, hs, holes)),
            _ if e.ty == Ty::Entity => {
                self.entities = true;
                let hole = self.hole(holes, e);
                Some(Selector::entity_id(Word::new(&self.id_objective()).ok()?, hole))
            }
            _ => {
                self.err(e.span, "expected a selector such as `@a[tag=x]` or an entity");
                None
            }
        }
    }

    /// A selector literal whose run-time options (`tag=$t`) become holes of the command being built.
    fn fill_selector(&mut self, sel: &Selector, exprs: &[h::Expr], holes: &mut Holes) -> Selector {
        let mut sel = sel.clone();
        if !exprs.is_empty() {
            let real: Vec<Hole> = exprs
                .iter()
                .map(|e| {
                    let v = self.expr(e);
                    if e.ty == Ty::Str { self.hole_str(holes, v) } else { self.hole_ir(holes, v) }
                })
                .collect();
            sel.remap_holes(|h| real[h.0 as usize]);
        }
        sel
    }

    fn selector(&mut self, e: &h::Expr, holes: &mut Holes) -> Option<Selector> {
        match &e.kind {
            h::ExprKind::Selector(s, hs) => Some(self.fill_selector(s, hs, holes)),
            _ => {
                self.err(e.span, "expected a selector literal such as `@a[tag=x]`");
                None
            }
        }
    }

    fn holder(&mut self, e: &h::Expr, holes: &mut Holes) -> ScoreHolder {
        match &e.kind {
            h::ExprKind::Selector(s, hs) => ScoreHolder::Selector(self.fill_selector(s, hs, holes)),
            _ if e.ty == Ty::Entity => match self.target(e, holes) {
                Some(sel) => ScoreHolder::Selector(sel),
                None => ScoreHolder::All,
            },
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
            h::CtxClause::As(e) => Clause::As(self.target(e, holes)?),
            h::CtxClause::At(e) => Clause::At(self.target(e, holes)?),
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
            "say" => {
                let mut holes = Holes::default();
                if let Some(msg) = self.str_arg(&args[0], &mut holes, |s| Ok(s.to_owned())) {
                    self.push_cmd(Command::Say(msg), holes, span, out);
                }
            }
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

    /// `execute [positioned x y z] summon <type> run function <ns>:_new_entity`: creates the entity
    /// and has the helper give it an id and mark it as ours.
    fn new_entity_command(&mut self, args: &[h::Expr], span: Span, holes: &mut Holes) -> Option<Command> {
        let _ = span;
        let id = self.str_arg(&args[0], holes, ResourceLocation::parse)?;
        let mut exec = Command::execute();
        if args.len() == 4 {
            exec = exec.positioned(self.pos(&args[1..4], holes)?);
        }
        self.entities = true;
        self.uses_summon = true;
        let helper = ResourceLocation::new(&self.namespace, "_new_entity").ok()?;
        Some(exec.clause(Clause::Summon(id)).run(Command::call(helper)))
    }

    /// A string argument of a command: a literal checked now by `parse`, or a hole filled at run
    /// time (the text is substituted as is, so it must be valid where it lands).
    fn str_arg<T>(&mut self, e: &h::Expr, holes: &mut Holes, parse: impl FnOnce(&str) -> datapack::Result<T>) -> Option<datapack::Arg<T>> {
        match const_str(e) {
            Some(s) => match parse(&s) {
                Ok(v) => Some(datapack::Arg::Lit(v)),
                Err(err) => {
                    self.err(e.span, err.message().to_owned());
                    None
                }
            },
            None => {
                let v = self.expr(e);
                Some(datapack::Arg::Hole(self.hole_str(holes, v)))
            }
        }
    }

    /// `a + b` on strings. Nested joins are flattened, and neighbouring literals merged.
    fn concat(&mut self, parts: Vec<ir::Expr>) -> ir::Expr {
        let mut flat: Vec<ir::Expr> = Vec::new();
        for p in parts {
            let inner = match p {
                ir::Expr::Concat(inner) => inner,
                other => vec![other],
            };
            for q in inner {
                match (flat.last_mut(), &q) {
                    (Some(ir::Expr::Data(Nbt::String(prev))), ir::Expr::Data(Nbt::String(s))) => prev.push_str(s),
                    _ => flat.push(q),
                }
            }
        }
        match flat.as_slice() {
            [lit @ ir::Expr::Data(Nbt::String(_))] => lit.clone(),
            _ => ir::Expr::Concat(flat),
        }
    }

    /// A value spliced into a string: text as it is, ints and bools as numbers, an entity as its id.
    fn text_part(&mut self, e: &h::Expr) -> ir::Expr {
        match &e.ty {
            Ty::List(_) | Ty::Struct(_) => {
                self.err(e.span, "a list or struct cannot be put into a string (print it with `print` instead)");
                ir::Expr::Data(Nbt::String(String::new()))
            }
            _ => self.expr(e),
        }
    }

    /// The command a world-acting builtin runs. Run-time values go into `holes`.
    fn builtin_command(&mut self, name: &str, args: &[h::Expr], span: Span, holes: &mut Holes) -> Option<Command> {
        let _ = span;
        let n = |i: usize| match args.get(i).map(|a| &a.kind) {
            Some(h::ExprKind::Int(n)) => Some(*n),
            _ => None,
        };
        Some(match name {
            "run_command" => Command::Run(self.str_arg(&args[0], holes, |s| Ok(s.to_owned()))?),
            "kill" => Command::Kill(Some(self.target(&args[0], holes)?)),
            "tag.add" | "tag.remove" => {
                let sel = self.target(&args[0], holes)?;
                let tag = self.str_arg(&args[1], holes, Word::new)?;
                // a tag chosen at run time can't be removed again by `_uninstall`
                if name == "tag.add"
                    && let datapack::Arg::Lit(t) = &tag
                {
                    self.added_tags.insert(t.as_str().to_owned());
                }
                let action = if name == "tag.add" { TagAction::Add(tag) } else { TagAction::Remove(tag) };
                Command::Tag { targets: sel, action }
            }
            "effect.give" => {
                let sel = self.target(&args[0], holes)?;
                let id = self.str_arg(&args[1], holes, ResourceLocation::parse)?;
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
            "effect.clear" => Command::Effect(Effect::Clear { targets: Some(self.target(&args[0], holes)?), effect: None }),
            "tp" => {
                let sel = self.target(&args[0], holes)?;
                Command::Teleport { targets: sel, dest: self.pos(&args[1..4], holes)? }
            }
            "setblock" => {
                let pos = self.pos(&args[0..3], holes)?;
                let block = self.str_arg(&args[3], holes, ResourceLocation::parse)?;
                Command::Setblock { pos, block: datapack::BlockState::new(block), mode: None }
            }
            "particle" => {
                let name = self.str_arg(&args[0], holes, ResourceLocation::parse)?;
                let pos = if args.len() == 4 { Some(self.pos(&args[1..4], holes)?) } else { None };
                Command::Particle { name, pos }
            }
            "summon" => {
                let id = self.str_arg(&args[0], holes, ResourceLocation::parse)?;
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
    let or = |(a, b): (bool, bool), (c, d): (bool, bool)| (a || c, b || d);
    match s {
        h::Stmt::Break => (true, false),
        h::Stmt::Continue => (false, true),
        h::Stmt::If { cond, then, els } => or(or(jumps_expr(cond), jumps_block(then)), jumps_block(els)),
        h::Stmt::Block(b) => jumps_block(b),
        h::Stmt::Let { init: e, .. } | h::Stmt::Expr(e) | h::Stmt::Return(Some(e)) => jumps_expr(e),
        h::Stmt::Assign { target, value, .. } => or(jumps_expr(target), jumps_expr(value)),
        _ => (false, false),
    }
}

/// `break`/`continue` inside the arms of a `match` used anywhere in the expression.
fn jumps_expr(e: &h::Expr) -> (bool, bool) {
    let mut acc = (false, false);
    if let h::ExprKind::Match { arms, .. } = &e.kind {
        for a in arms {
            let (x, y) = jumps_block(&a.body);
            acc = (acc.0 || x, acc.1 || y);
            for sub in a.guard.iter().chain(a.value.iter()) {
                let (x, y) = jumps_expr(sub);
                acc = (acc.0 || x, acc.1 || y);
            }
        }
    }
    e.for_each_child(&mut |c| {
        let (x, y) = jumps_expr(c);
        acc = (acc.0 || x, acc.1 || y);
    });
    acc
}

fn jumps_block(b: &h::Block) -> (bool, bool) {
    b.stmts.iter().map(jumps).fold((false, false), |(a, b), (c, d)| (a || c, b || d))
}
