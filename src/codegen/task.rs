//! `task fn`: a function whose loops run a slice per tick.
//!
//! A task body is turned into a state machine in ONE function (`<task>__step__run`), so its
//! locals stay in the same static slots from one tick to the next:
//!
//! ```text
//! budget = <task-budget>
//! while true {
//!     if budget <= 0 { schedule the step again next tick; suspend }
//!     if state == 0 { ..block 0..; budget -= its cost }
//!     if state == 1 { ..block 1.. }
//!     ...
//! }
//! ```
//!
//! The code between loops, and the loops that contain other loops, become blocks that set
//! `state` to say where to go next (a control flow graph). A loop with no loop inside it is one
//! block of its own that runs as an ordinary loop, with a budget check at the top of every trip:
//! when the budget is spent, the task schedules itself for the next tick and returns without
//! dropping its locals, and the next tick enters the same block again. `start name(args)`
//! stores the arguments in globals, sets `state` to 0 and schedules the first slice.
//!
//! What a trip or a block costs is measured, not guessed: the first code generation pass
//! numbers the loops and blocks (`ir::LoopInfo::slice`, `ir::Stmt::BlockInfo`), `compile.rs`
//! measures the lowered functions (`cost.rs`), and a second pass writes the numbers in.

use super::*;
use crate::datapack::{FunctionRef, Schedule};

/// The state machine of the task being generated.
pub(super) struct TaskCtx {
    /// The globals holding where the task is and whether it runs.
    pub state: String,
    pub running: String,
    /// The IR local counting the commands left in this tick.
    budget: String,
    /// The step function the task schedules (`<task>__step`).
    step: ResourceLocation,
    blocks: Vec<Vec<ir::Stmt>>,
    cur: usize,
    /// No code may follow in the current block (it ended with a jump).
    dead: bool,
    /// Targets of `continue` and `break` of the enclosing loops that are part of the state graph.
    loops: Vec<(usize, usize)>,
}

fn is_loop(s: &h::Stmt) -> bool {
    matches!(s, h::Stmt::While { .. } | h::Stmt::Loop { .. } | h::Stmt::ForRange { .. } | h::Stmt::ForList { .. })
}

fn loop_body(s: &h::Stmt) -> Option<&h::Block> {
    match s {
        h::Stmt::While { body, .. } | h::Stmt::Loop { body } | h::Stmt::ForRange { body, .. } | h::Stmt::ForList { body, .. } => Some(body),
        _ => None,
    }
}

fn match_arms(s: &h::Stmt) -> Option<&Vec<h::MatchArm>> {
    match s {
        h::Stmt::Expr(h::Expr { kind: h::ExprKind::Match { arms, .. }, .. }) => Some(arms),
        _ => None,
    }
}

/// Whether the statement is or contains a loop that the task could stop in the middle of. A
/// `Context` body is a function that may run once per entity: loops in it just run.
fn has_loop(s: &h::Stmt) -> bool {
    let block = |b: &h::Block| b.stmts.iter().any(has_loop);
    if is_loop(s) {
        return true;
    }
    match s {
        h::Stmt::If { then, els, .. } => block(then) || block(els),
        h::Stmt::Block(b) => block(b),
        _ => match_arms(s).is_some_and(|arms| arms.iter().any(|a| block(&a.body))),
    }
}

/// Whether a `break` or `continue` that belongs to the enclosing loop is in the statement (not
/// counting ones inside loops nested in it).
fn has_free_jump(s: &h::Stmt) -> bool {
    let block = |b: &h::Block| b.stmts.iter().any(has_free_jump);
    match s {
        h::Stmt::Break | h::Stmt::Continue => true,
        h::Stmt::If { then, els, .. } => block(then) || block(els),
        h::Stmt::Block(b) => block(b),
        _ => match_arms(s).is_some_and(|arms| arms.iter().any(|a| block(&a.body))),
    }
}

impl Gen<'_> {
    fn t(&mut self) -> &mut TaskCtx {
        self.task.as_mut().expect("generating a task")
    }

    fn tpush(&mut self, st: ir::Stmt) {
        let t = self.t();
        if !t.dead {
            let cur = t.cur;
            t.blocks[cur].push(st);
        }
    }

    fn tnew(&mut self) -> usize {
        let t = self.t();
        t.blocks.push(Vec::new());
        t.blocks.len() - 1
    }

    /// Continue in block `to` (a block that something jumps to).
    fn tswitch(&mut self, to: usize) {
        let t = self.t();
        t.cur = to;
        t.dead = false;
    }

    fn tset(&self, name: &str, v: i32) -> ir::Stmt {
        ir::Stmt::Assign { target: ir::VarRef::Global(name.to_owned()), value: ir::Expr::Int(v) }
    }

    /// End the current block by going to block `to`.
    fn tgoto(&mut self, to: usize) {
        let state = self.t().state.clone();
        let st = self.tset(&state, to as i32);
        self.tpush(st);
        self.t().dead = true;
    }

    /// The statements that stop the task for this tick: ask for the next slice, leave the
    /// function with the locals kept.
    fn tsuspend(&self) -> Vec<ir::Stmt> {
        let step = self.task.as_ref().expect("generating a task").step.clone();
        vec![ir::Stmt::Command(Command::Schedule(Schedule::Function { function: FunctionRef::Id(step), ticks: 1, mode: None })), ir::Stmt::Suspend]
    }

    fn tbudget_spent(&self) -> ir::Expr {
        ir::Expr::bin(ir::BinOp::Le, ir::Expr::var(&self.task.as_ref().expect("generating a task").budget), ir::Expr::Int(0))
    }

    fn slice(&mut self) -> u32 {
        self.next_slice += 1;
        self.next_slice
    }

    fn slice_cost(&self, slice: u32) -> i32 {
        self.slice_costs.get(&slice).map_or(1, |c| (*c).clamp(1, i32::MAX as u64) as i32)
    }

    fn tstmts(&mut self, stmts: &[h::Stmt]) {
        for s in stmts {
            if self.task.as_ref().is_some_and(|t| t.dead) {
                break;
            }
            self.tstmt(s);
        }
    }

    fn tstmt(&mut self, s: &h::Stmt) {
        let in_state_loop = !self.task.as_ref().expect("task").loops.is_empty();
        match s {
            h::Stmt::Break | h::Stmt::Continue if in_state_loop => {
                let (cont, brk) = *self.t().loops.last().expect("a loop");
                self.tgoto(if matches!(s, h::Stmt::Break) { brk } else { cont });
            }
            h::Stmt::Block(b) if b.stmts.iter().any(|x| has_loop(x) || (in_state_loop && has_free_jump(x))) => self.tstmts(&b.stmts),
            h::Stmt::If { cond, then, els } if has_loop(s) || (in_state_loop && has_free_jump(s)) => {
                let (pre, c) = self.expr_pre(cond);
                for p in pre {
                    self.tpush(p);
                }
                let (then_b, else_b, join) = (self.tnew(), self.tnew(), self.tnew());
                let state = self.t().state.clone();
                let branch = ir::Stmt::If { cond: c, then: vec![self.tset(&state, then_b as i32)], els: vec![self.tset(&state, else_b as i32)] };
                self.tpush(branch);
                self.t().dead = true;
                self.tswitch(then_b);
                self.tstmts(&then.stmts);
                self.tgoto(join);
                self.tswitch(else_b);
                self.tstmts(&els.stmts);
                self.tgoto(join);
                self.tswitch(join);
            }
            _ if match_arms(s).is_some() && (has_loop(s) || (in_state_loop && has_free_jump(s))) => {
                let span = match s {
                    h::Stmt::Expr(e) => e.span,
                    _ => Span::new(0, 0),
                };
                self.err(span, "a loop (or `break`/`continue`) inside a `match` of a task cannot be sliced yet; use `if`/`else`, or put the loop in a function");
                self.t().dead = true;
            }
            _ if is_loop(s) => {
                let simple = !loop_body(s).is_some_and(|b| b.stmts.iter().any(has_loop));
                if simple { self.sliced_loop(s) } else { self.state_loop(s) }
            }
            h::Stmt::Return(_) => {
                self.simple(s);
                self.t().dead = true;
            }
            _ => self.simple(s),
        }
    }

    /// A statement that runs inside the current block, as it would in an ordinary function.
    fn simple(&mut self, s: &h::Stmt) {
        let mut out = Vec::new();
        self.stmt(s, &mut out);
        for st in out {
            self.tpush(st);
        }
    }

    /// A loop with no loop inside: the ordinary loop code, in a block of its own, with a budget
    /// check at the top of each trip. Its set-up (the counter of a `for`, the copy of a list) stays
    /// in the block before, because a resumed task must not run it again.
    fn sliced_loop(&mut self, s: &h::Stmt) {
        let mut code = Vec::new();
        self.stmt(s, &mut code);
        let Some(at) = code.iter().rposition(|x| matches!(x, ir::Stmt::While { .. })) else {
            for st in code {
                self.tpush(st);
            }
            return;
        };
        let start = if at > 0 && matches!(code[at - 1], ir::Stmt::LoopInfo(_)) { at - 1 } else { at };
        let mut loop_part = code.split_off(start);
        for st in code {
            self.tpush(st);
        }
        let slice = self.slice();
        let cost = self.slice_cost(slice);
        let budget = self.task.as_ref().expect("task").budget.clone();
        let spent = self.tbudget_spent();
        let suspend = self.tsuspend();
        // the marker in front of the `While` says which loop it is
        let info = match loop_part.first() {
            Some(ir::Stmt::LoopInfo(i)) => ir::LoopInfo { slice: Some(slice), ..i.clone() },
            _ => ir::LoopInfo { trips: None, origin: None, slice: Some(slice) },
        };
        if matches!(loop_part.first(), Some(ir::Stmt::LoopInfo(_))) {
            loop_part.remove(0);
        }
        if let Some(ir::Stmt::While { body, .. }) = loop_part.first_mut() {
            body.insert(0, ir::Stmt::If { cond: spent, then: suspend, els: Vec::new() });
            body.push(ir::Stmt::Assign { target: ir::VarRef::Local(budget.clone()), value: ir::Expr::bin(ir::BinOp::Sub, ir::Expr::var(&budget), ir::Expr::Int(cost)) });
        }
        let body_block = self.tnew();
        let after = self.tnew();
        self.tgoto(body_block);
        self.tswitch(body_block);
        self.tpush(ir::Stmt::LoopInfo(info));
        for st in loop_part {
            self.tpush(st);
        }
        self.tgoto(after);
        self.tswitch(after);
    }

    /// A loop with loops inside it: each trip goes through the state graph, so the loops inside
    /// can stop in the middle of it. `break` and `continue` are jumps.
    fn state_loop(&mut self, s: &h::Stmt) {
        let (header, body_b, exit) = (self.tnew(), self.tnew(), self.tnew());
        let state = self.t().state.clone();
        match s {
            h::Stmt::While { cond, body } => {
                self.tgoto(header);
                self.tswitch(header);
                let (pre, c) = self.expr_pre(cond);
                for p in pre {
                    self.tpush(p);
                }
                let go = ir::Stmt::If { cond: c, then: vec![self.tset(&state, body_b as i32)], els: vec![self.tset(&state, exit as i32)] };
                self.tpush(go);
                self.t().dead = true;
                self.tswitch(body_b);
                self.t().loops.push((header, exit));
                self.tstmts(&body.stmts);
                self.t().loops.pop();
                self.tgoto(header);
            }
            h::Stmt::Loop { body } => {
                self.tgoto(body_b);
                self.tswitch(body_b);
                self.t().loops.push((body_b, exit));
                self.tstmts(&body.stmts);
                self.t().loops.pop();
                self.tgoto(body_b);
            }
            h::Stmt::ForRange { var, start, end, inclusive, body } => {
                let s_ir = self.expr(start);
                let e_ir = self.expr(end);
                let mut init = Vec::new();
                self.flush(&mut init);
                for p in init {
                    self.tpush(p);
                }
                let var_name = self.local_names[var.0].clone();
                let end_name = self.fresh("end");
                self.tpush(ir::Stmt::Let { name: var_name.clone(), kind: ir::Kind::Int, init: Some(s_ir) });
                self.tpush(ir::Stmt::Let { name: end_name.clone(), kind: ir::Kind::Int, init: Some(e_ir) });
                let step = self.tnew();
                self.tgoto(header);
                self.tswitch(header);
                let cmp = ir::Expr::bin(if *inclusive { ir::BinOp::Le } else { ir::BinOp::Lt }, ir::Expr::var(&var_name), ir::Expr::var(&end_name));
                let go = ir::Stmt::If { cond: cmp, then: vec![self.tset(&state, body_b as i32)], els: vec![self.tset(&state, exit as i32)] };
                self.tpush(go);
                self.t().dead = true;
                self.tswitch(body_b);
                self.t().loops.push((step, exit));
                self.tstmts(&body.stmts);
                self.t().loops.pop();
                self.tgoto(step);
                self.tswitch(step);
                self.tpush(ir::Stmt::Assign { target: ir::VarRef::Local(var_name.clone()), value: ir::Expr::bin(ir::BinOp::Add, ir::Expr::var(&var_name), ir::Expr::Int(1)) });
                self.tgoto(header);
            }
            h::Stmt::ForList { var, list, body } => {
                let l = self.expr(list);
                let mut init = Vec::new();
                self.flush(&mut init);
                for p in init {
                    self.tpush(p);
                }
                let iter = self.fresh("iter");
                self.tpush(ir::Stmt::Let { name: iter.clone(), kind: ir::Kind::Data, init: Some(l) });
                let elem_ty = self.hir.fns[self.cur_fn].locals[var.0].ty.clone();
                self.tgoto(header);
                self.tswitch(header);
                let has_items = ir::Expr::bin(ir::BinOp::Gt, ir::Expr::Len(ir::VarRef::Local(iter.clone()), NbtPath::new()), ir::Expr::Int(0));
                let go = ir::Stmt::If { cond: has_items, then: vec![self.tset(&state, body_b as i32)], els: vec![self.tset(&state, exit as i32)] };
                self.tpush(go);
                self.t().dead = true;
                self.tswitch(body_b);
                let first = NbtPath::new().index(0);
                self.tpush(ir::Stmt::Let {
                    name: self.local_names[var.0].clone(),
                    kind: kind_of(&elem_ty),
                    init: Some(from_data(ir::Expr::Field(ir::VarRef::Local(iter.clone()), first.clone()), &elem_ty)),
                });
                self.tpush(ir::Stmt::Remove { target: ir::VarRef::Local(iter), path: first });
                self.t().loops.push((header, exit));
                self.tstmts(&body.stmts);
                self.t().loops.pop();
                self.tgoto(header);
            }
            _ => unreachable!("a loop"),
        }
        self.tswitch(exit);
    }

    /// `task fn name(params) { body }`: the function that starts it (`name`), the step the game
    /// runs each tick (`name__step`, guarded like an entry point), and the globals between them.
    pub(super) fn task_function(&mut self, idx: usize, out: &mut Vec<ir::Function>) {
        let f = &self.hir.fns[idx];
        self.cur_fn = idx;
        self.used = HashSet::new();
        self.loops.clear();
        self.pre.clear();
        self.local_names = f.locals.iter().map(|l| unique(&mut self.used, sanitize(&l.name))).collect();
        let name = self.fn_names[idx].clone();
        let state = unique(&mut self.used_globals, format!("{name}__state"));
        let running = unique(&mut self.used_globals, format!("{name}__running"));
        for g in [&state, &running] {
            self.extra_globals.push(ir::Global { name: g.clone(), kind: ir::Kind::Int, init: ir::Expr::Int(0) });
        }
        let step_name = unique(&mut self.used_fns, format!("{name}__step"));
        let step = ResourceLocation::new(&self.namespace, &step_name).expect("a valid id");
        let budget = self.fresh("budget");
        // the arguments wait in globals until the first slice copies them into locals
        let mut params = Vec::new();
        let mut start_body = Vec::new();
        let mut first_block = Vec::new();
        for p in &f.params {
            let ty = &f.locals[p.0].ty;
            let local = self.local_names[p.0].clone();
            let global = unique(&mut self.used_globals, format!("{name}__arg_{local}"));
            self.extra_globals.push(ir::Global { name: global.clone(), kind: kind_of(ty), init: self.default_ir(ty) });
            params.push(ir::Param { name: local.clone(), kind: kind_of(ty) });
            start_body.push(ir::Stmt::Assign { target: ir::VarRef::Global(global.clone()), value: ir::Expr::var(&local) });
            first_block.push(ir::Stmt::Let { name: local, kind: kind_of(ty), init: Some(ir::Expr::Global(global)) });
        }
        start_body.push(ir::Stmt::Assign { target: ir::VarRef::Global(state.clone()), value: ir::Expr::Int(0) });
        start_body.push(ir::Stmt::Assign { target: ir::VarRef::Global(running.clone()), value: ir::Expr::Int(1) });
        start_body.push(ir::Stmt::Command(Command::Schedule(Schedule::Function { function: FunctionRef::Id(step.clone()), ticks: 1, mode: None })));
        out.push(ir::Function { name: name.clone(), params, ret: None, body: start_body, entry: None });

        self.task = Some(TaskCtx { state: state.clone(), running: running.clone(), budget: budget.clone(), step, blocks: vec![first_block], cur: 0, dead: false, loops: Vec::new() });
        self.tstmts(&f.body.stmts);
        // the end of the body: the task is done
        if !self.task.as_ref().expect("task").dead {
            let off = self.tset(&running, 0);
            self.tpush(off);
            self.tpush(ir::Stmt::Return(None));
        }
        let ctx = self.task.take().expect("task");

        // the dispatcher: every block, in order, guarded by its state number
        let mut trip = vec![ir::Stmt::If { cond: ir::Expr::bin(ir::BinOp::Le, ir::Expr::var(&budget), ir::Expr::Int(0)), then: Vec::new(), els: Vec::new() }];
        let suspend = vec![
            ir::Stmt::Command(Command::Schedule(Schedule::Function { function: FunctionRef::Id(ctx.step.clone()), ticks: 1, mode: None })),
            ir::Stmt::Suspend,
        ];
        if let Some(ir::Stmt::If { then, .. }) = trip.first_mut() {
            *then = suspend;
        }
        for (k, mut block) in ctx.blocks.into_iter().enumerate() {
            let slice = self.slice();
            let cost = self.slice_cost(slice);
            if !block.iter().any(|s| matches!(s, ir::Stmt::Return(_))) {
                block.push(ir::Stmt::Assign { target: ir::VarRef::Local(budget.clone()), value: ir::Expr::bin(ir::BinOp::Sub, ir::Expr::var(&budget), ir::Expr::Int(cost)) });
            }
            trip.push(ir::Stmt::BlockInfo(slice));
            trip.push(ir::Stmt::If { cond: ir::Expr::bin(ir::BinOp::Eq, ir::Expr::Global(state.clone()), ir::Expr::Int(k as i32)), then: block, els: Vec::new() });
        }
        let body = vec![
            ir::Stmt::If { cond: ir::Expr::bin(ir::BinOp::Eq, ir::Expr::Global(running.clone()), ir::Expr::Int(0)), then: vec![ir::Stmt::Return(None)], els: Vec::new() },
            ir::Stmt::Let { name: budget, kind: ir::Kind::Int, init: Some(ir::Expr::Int(self.task_budget)) },
            ir::Stmt::While { cond: ir::Expr::Bool(true), body: trip },
        ];
        self.guarded(&f.name, step_name, Vec::new(), body, None, out);
    }
}
