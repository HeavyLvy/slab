//! IR -> data pack. All command text comes from `crate::datapack`; this module only decides
//! *which* typed commands to build. Design notes: docs/lang/ir.md.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

use crate::datapack::{
    self, BuildError, Clause, Command, Condition, Data, DataPack, DataTarget, Execute, FunctionRef, HolderName, Hole,
    IntRange, ModifyOp, ModifySource, Nbt, NbtPath, NumType, Objective, PathNode, ResourceLocation, ScoreHolder, ScoreOp,
    ScoreTest, Scoreboard, Selector, StoreKind, StoreTarget, Text,
};
use crate::ir::{self, BinOp, Entry, Expr, Kind, PrintPart, Program, Stmt, UnOp, VarRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerError {
    pub message: String,
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LowerError {}

impl From<datapack::Error> for LowerError {
    fn from(e: datapack::Error) -> Self {
        LowerError { message: e.to_string() }
    }
}

impl From<BuildError> for LowerError {
    fn from(e: BuildError) -> Self {
        LowerError { message: e.to_string() }
    }
}

type R<T> = Result<T, LowerError>;

macro_rules! err {
    ($($arg:tt)*) => {
        return Err(LowerError { message: format!($($arg)*) })
    };
}

/// A readable/writable place in command storage.
#[derive(Clone, Debug)]
struct DataLoc {
    storage: ResourceLocation,
    path: NbtPath,
}

impl DataLoc {
    fn target(&self) -> DataTarget {
        DataTarget::Storage(self.storage.clone())
    }

    fn extend(&self, extra: &NbtPath) -> R<DataLoc> {
        let mut path = self.path.clone();
        for node in extra.nodes() {
            if matches!(node, PathNode::RootFilter(_)) {
                err!("a root filter is not valid in a field path");
            }
            path = path.push(node.clone());
        }
        Ok(DataLoc { storage: self.storage.clone(), path })
    }
}

/// An evaluated scoreboard-class value. `bool` = the holder is an owned temp to release.
#[derive(Clone, Debug)]
enum IntVal {
    Const(i32),
    Holder(HolderName, bool),
}

#[derive(Clone, Debug)]
enum DataVal {
    Const(Nbt),
    Loc(DataLoc, bool),
}

/// A single `execute if|unless` test.
#[derive(Clone, Debug)]
struct Test {
    negate: bool,
    cond: Condition,
}

enum CondKind {
    True,
    False,
    Tests(Vec<Test>),
}

/// A lowered condition plus the temps that must be released once it has been used.
struct Cond {
    kind: CondKind,
    temps: Vec<HolderName>,
}

impl Cond {
    fn constant(b: bool) -> Cond {
        Cond { kind: if b { CondKind::True } else { CondKind::False }, temps: Vec::new() }
    }
}

enum CallResult {
    None,
    Int,
    Data,
}

struct FnState {
    name: String,
    vars: HashMap<String, Kind>,
    /// Declared locals/params in declaration order (the set saved around recursive calls).
    declared: Vec<(String, Kind)>,
    int_temps: Vec<bool>,
    data_temps: Vec<bool>,
    next_block: u32,
    next_macro: u32,
    /// Names of the Data locals/params, removed from storage whenever the function exits.
    drop: Vec<String>,
}

impl FnState {
    fn new(name: &str) -> Self {
        FnState { name: name.to_owned(), vars: HashMap::new(), declared: Vec::new(), int_temps: Vec::new(), data_temps: Vec::new(), next_block: 0, next_macro: 0, drop: Vec::new() }
    }
}

struct Lower<'a> {
    prog: &'a Program,
    ns: String,
    obj: Objective,
    funcs: HashMap<&'a str, &'a ir::Function>,
    globals: HashMap<&'a str, Kind>,
    /// function -> every function it can reach through calls
    reaches: HashMap<String, HashSet<String>>,
    consts: BTreeSet<i32>,
    pack: DataPack,
    f: FnState,
    out: Vec<Command>,
}

/// Compiles an IR program into a validated-on-write data pack.
pub fn lower(program: &Program) -> R<DataPack> {
    let ns = program.namespace.clone();
    let obj = Objective::new(&ns)?;
    let mut funcs = HashMap::new();
    for f in &program.functions {
        check_ident("function", &f.name)?;
        if funcs.insert(f.name.as_str(), f).is_some() {
            err!("function `{}` is defined twice", f.name);
        }
        if f.entry.is_some() && !f.params.is_empty() {
            err!("entry function `{}` cannot take parameters", f.name);
        }
    }
    let mut globals = HashMap::new();
    for g in &program.globals {
        check_ident("global", &g.name)?;
        if globals.insert(g.name.as_str(), g.kind).is_some() {
            err!("global `{}` is defined twice", g.name);
        }
    }
    let reaches = call_reachability(program, &funcs)?;
    let mut l = Lower {
        prog: program,
        ns,
        obj,
        funcs,
        globals,
        reaches,
        consts: BTreeSet::new(),
        pack: DataPack::new(program.description.clone()),
        f: FnState::new(""),
        out: Vec::new(),
    };
    for f in &program.functions {
        l.lower_function(f)?;
    }
    l.emit_init()?;
    l.emit_uninstall()?;
    Ok(l.pack)
}

fn check_ident(what: &str, name: &str) -> R<()> {
    if name.is_empty() || !name.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_')) {
        err!("invalid {what} name `{name}`: use [a-z0-9_]+");
    }
    Ok(())
}

// ---------------------------------------------------------------- purity / flow

fn has_call(e: &Expr) -> bool {
    match e {
        Expr::Call(..) => true,
        Expr::CommandSuccess { args, .. } => args.iter().any(|(_, e)| has_call(e)),
        Expr::Unary(_, a) | Expr::ToInt(a) | Expr::ToData(a) => has_call(a),
        Expr::Binary(_, a, b) => has_call(a) || has_call(b),
        _ => false,
    }
}

fn collect_calls_expr(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::CommandSuccess { args, .. } => args.iter().for_each(|(_, a)| collect_calls_expr(a, out)),
        Expr::Call(n, args) => {
            out.push(n.clone());
            args.iter().for_each(|a| collect_calls_expr(a, out));
        }
        Expr::Unary(_, a) | Expr::ToInt(a) | Expr::ToData(a) => collect_calls_expr(a, out),
        Expr::Binary(_, a, b) => {
            collect_calls_expr(a, out);
            collect_calls_expr(b, out);
        }
        _ => {}
    }
}

fn collect_calls(stmts: &[Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match s {
            Stmt::Let { init, .. } => {
                if let Some(e) = init {
                    collect_calls_expr(e, out);
                }
            }
            Stmt::Assign { value, .. } | Stmt::SetField { value, .. } | Stmt::Append { value, .. } => collect_calls_expr(value, out),
            Stmt::If { cond, then, els } => {
                collect_calls_expr(cond, out);
                collect_calls(then, out);
                collect_calls(els, out);
            }
            Stmt::While { cond, body } => {
                collect_calls_expr(cond, out);
                collect_calls(body, out);
            }
            Stmt::Return(Some(e)) | Stmt::Expr(e) | Stmt::Print(e) => collect_calls_expr(e, out),
            Stmt::PrintParts(parts) => {
                for p in parts {
                    if let PrintPart::Value(e) = p {
                        collect_calls_expr(e, out);
                    }
                }
            }
            Stmt::SetScore { value, .. } => collect_calls_expr(value, out),
            Stmt::Context { args, body, .. } => {
                args.iter().for_each(|(_, e)| collect_calls_expr(e, out));
                collect_calls(body, out);
            }
            Stmt::MacroCommand { args, .. } | Stmt::ReadAt { args, .. } => args.iter().for_each(|(_, e)| collect_calls_expr(e, out)),
            Stmt::WriteAt { value, args, .. } => {
                collect_calls_expr(value, out);
                args.iter().for_each(|(_, e)| collect_calls_expr(e, out));
            }
            Stmt::Return(None) | Stmt::Command(_) | Stmt::Remove { .. } => {}
        }
    }
}

fn call_reachability(program: &Program, funcs: &HashMap<&str, &ir::Function>) -> R<HashMap<String, HashSet<String>>> {
    let mut direct: HashMap<&str, Vec<String>> = HashMap::new();
    for f in &program.functions {
        let mut calls = Vec::new();
        collect_calls(&f.body, &mut calls);
        for c in &calls {
            if !funcs.contains_key(c.as_str()) {
                err!("function `{}` calls unknown function `{c}`", f.name);
            }
        }
        direct.insert(f.name.as_str(), calls);
    }
    let mut out = HashMap::new();
    for f in &program.functions {
        let mut seen: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = direct[f.name.as_str()].clone();
        while let Some(n) = stack.pop() {
            if seen.insert(n.clone()) {
                stack.extend(direct[n.as_str()].iter().cloned());
            }
        }
        out.insert(f.name.clone(), seen);
    }
    Ok(out)
}

/// Names of the Data variables declared anywhere in `stmts` (the ones to drop at function exit).
fn collect_data_locals(stmts: &[Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match s {
            Stmt::Let { name, kind: Kind::Data, .. } => out.push(name.clone()),
            Stmt::ReadAt { dest, .. } => out.push(dest.clone()),
            Stmt::If { then, els, .. } => {
                collect_data_locals(then, out);
                collect_data_locals(els, out);
            }
            Stmt::While { body, .. } | Stmt::Context { body, .. } => collect_data_locals(body, out),
            _ => {}
        }
    }
}

fn may_return(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        Stmt::Return(_) => true,
        Stmt::If { then, els, .. } => may_return(then) || may_return(els),
        Stmt::While { body, .. } => may_return(body),
        _ => false,
    })
}

/// Floored integer arithmetic with i32 wrapping, matching the scoreboard.
fn fold(op: BinOp, a: i32, b: i32) -> R<i32> {
    let floor_div = |a: i32, b: i32| -> i64 {
        let (a, b) = (a as i64, b as i64);
        let q = a / b;
        if a % b != 0 && ((a < 0) != (b < 0)) { q - 1 } else { q }
    };
    Ok(match op {
        BinOp::Add => a.wrapping_add(b),
        BinOp::Sub => a.wrapping_sub(b),
        BinOp::Mul => a.wrapping_mul(b),
        BinOp::Div | BinOp::Mod if b == 0 => err!("division by zero"),
        BinOp::Div => floor_div(a, b) as i32,
        BinOp::Mod => (a as i64 - floor_div(a, b) * b as i64) as i32,
        _ => unreachable!("fold is only for arithmetic"),
    })
}

fn is_arith(op: BinOp) -> bool {
    matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod)
}

fn is_cmp(op: BinOp) -> bool {
    matches!(op, BinOp::Lt | BinOp::Le | BinOp::Eq | BinOp::Ne | BinOp::Ge | BinOp::Gt)
}

fn cmp_const(op: BinOp, a: i32, b: i32) -> bool {
    match op {
        BinOp::Lt => a < b,
        BinOp::Le => a <= b,
        BinOp::Eq => a == b,
        BinOp::Ne => a != b,
        BinOp::Ge => a >= b,
        BinOp::Gt => a > b,
        _ => unreachable!(),
    }
}

fn flip(op: BinOp) -> BinOp {
    match op {
        BinOp::Lt => BinOp::Gt,
        BinOp::Le => BinOp::Ge,
        BinOp::Gt => BinOp::Lt,
        BinOp::Ge => BinOp::Le,
        other => other,
    }
}

fn to_cmp(op: BinOp) -> datapack::Cmp {
    match op {
        BinOp::Lt => datapack::Cmp::Lt,
        BinOp::Le => datapack::Cmp::Le,
        BinOp::Eq => datapack::Cmp::Eq,
        BinOp::Ge => datapack::Cmp::Ge,
        BinOp::Gt => datapack::Cmp::Gt,
        _ => unreachable!("Ne is handled by negating Eq"),
    }
}

/// `holder <op> c` as a `matches` range, or None when the range isn't expressible (i32 edges).
fn range_for(op: BinOp, c: i32) -> Option<IntRange> {
    match op {
        BinOp::Lt => c.checked_sub(1).map(IntRange::at_most),
        BinOp::Le => Some(IntRange::at_most(c)),
        BinOp::Eq => Some(IntRange::exactly(c)),
        BinOp::Ge => Some(IntRange::at_least(c)),
        BinOp::Gt => c.checked_add(1).map(IntRange::at_least),
        _ => None,
    }
}

fn apply_tests(mut e: Execute, tests: &[Test]) -> Execute {
    for t in tests {
        e = if t.negate { e.unless(t.cond.clone()) } else { e.if_(t.cond.clone()) };
    }
    e
}

fn fake(h: &HolderName) -> ScoreHolder {
    ScoreHolder::fake(h.clone())
}

// ------------------------------------------------------------------- lowering

impl<'a> Lower<'a> {
    // ---- naming

    fn holder(&self, s: String) -> R<HolderName> {
        Ok(HolderName::new(&s)?)
    }

    fn var_holder(&self, name: &str) -> R<HolderName> {
        self.holder(format!("#{}.v_{name}", self.f.name))
    }

    fn global_holder(&self, name: &str) -> R<HolderName> {
        self.holder(format!("#g.{name}"))
    }

    fn param_holder(&self, func: &str, name: &str) -> R<HolderName> {
        self.holder(format!("#{func}.v_{name}"))
    }

    fn ret_holder(&self) -> R<HolderName> {
        self.holder("#ret".to_owned())
    }

    fn storage(&self, name: &str) -> R<ResourceLocation> {
        Ok(ResourceLocation::new(&self.ns, name)?)
    }

    fn var_loc(&self, func: &str, name: &str) -> R<DataLoc> {
        Ok(DataLoc { storage: self.storage("vars")?, path: NbtPath::new().key(func).key(format!("v_{name}")) })
    }

    fn global_loc(&self, name: &str) -> R<DataLoc> {
        Ok(DataLoc { storage: self.storage("globals")?, path: NbtPath::new().key(name) })
    }

    fn ret_loc(&self) -> R<DataLoc> {
        Ok(DataLoc { storage: self.storage("ret")?, path: NbtPath::new().key("v") })
    }

    fn stack_loc(&self, list: &str) -> R<DataLoc> {
        Ok(DataLoc { storage: self.storage("stack")?, path: NbtPath::new().key(list) })
    }

    fn const_holder(&mut self, v: i32) -> R<HolderName> {
        self.consts.insert(v);
        if v < 0 { self.holder(format!("#c_m{}", v.unsigned_abs())) } else { self.holder(format!("#c_{v}")) }
    }

    fn emit(&mut self, c: Command) {
        self.out.push(c);
    }

    // ---- temps

    fn alloc_int(&mut self) -> R<HolderName> {
        let pool = &mut self.f.int_temps;
        let i = match pool.iter().position(|used| !used) {
            Some(i) => i,
            None => {
                pool.push(false);
                pool.len() - 1
            }
        };
        pool[i] = true;
        self.holder(format!("#{}.t_{i}", self.f.name))
    }

    fn temp_index(&self, h: &HolderName) -> Option<usize> {
        h.as_str().strip_prefix(&format!("#{}.t_", self.f.name))?.parse().ok()
    }

    fn free_int(&mut self, h: &HolderName) {
        if let Some(i) = self.temp_index(h) {
            self.f.int_temps[i] = false;
        }
    }

    fn alloc_data(&mut self) -> R<DataLoc> {
        let pool = &mut self.f.data_temps;
        let i = match pool.iter().position(|used| !used) {
            Some(i) => i,
            None => {
                pool.push(false);
                pool.len() - 1
            }
        };
        pool[i] = true;
        Ok(DataLoc { storage: self.storage("vars")?, path: NbtPath::new().key(self.f.name.clone()).key(format!("t_{i}")) })
    }

    fn data_temp_index(&self, l: &DataLoc) -> Option<usize> {
        match l.path.nodes() {
            [PathNode::Key(f), PathNode::Key(t)] if *f == self.f.name => t.strip_prefix("t_")?.parse().ok(),
            _ => None,
        }
    }

    fn free_data(&mut self, l: &DataLoc) {
        if let Some(i) = self.data_temp_index(l) {
            self.f.data_temps[i] = false;
        }
    }

    fn release_int(&mut self, v: &IntVal) {
        if let IntVal::Holder(h, true) = v {
            self.free_int(h);
        }
    }

    /// Returns a Data temporary to the pool and removes its value from storage: a temp's value is
    /// dead once it has been used, and leaving it would leak whatever it held.
    fn release_data(&mut self, v: &DataVal) {
        if let DataVal::Loc(l, true) = v {
            self.free_data(l);
            self.drop_data(l);
        }
    }

    fn drop_data(&mut self, l: &DataLoc) {
        self.emit(Command::Data(Data::Remove { target: l.target(), path: l.path.clone() }));
    }

    /// Cleanup at a function exit: remove every Data local (docs/lang/memory.md).
    fn emit_drop(&mut self) -> R<()> {
        match self.f.drop.len() {
            0 => {}
            1 => {
                let name = self.f.drop[0].clone();
                let loc = self.var_loc(&self.f.name.clone(), &name)?;
                self.drop_data(&loc);
            }
            _ => {
                let id = ResourceLocation::new(&self.ns, &format!("{}/_drop", self.f.name))?;
                self.emit(Command::call(id));
            }
        }
        Ok(())
    }

    // ---- kinds

    fn var_kind(&self, name: &str) -> R<Kind> {
        match self.f.vars.get(name) {
            Some(k) => Ok(*k),
            None => err!("unknown variable `{name}` in function `{}`", self.f.name),
        }
    }

    fn global_kind(&self, name: &str) -> R<Kind> {
        match self.globals.get(name) {
            Some(k) => Ok(*k),
            None => err!("unknown global `{name}`"),
        }
    }

    fn ref_kind(&self, r: &VarRef) -> R<Kind> {
        match r {
            VarRef::Local(n) => self.var_kind(n),
            VarRef::Global(n) => self.global_kind(n),
        }
    }

    fn ref_loc(&self, r: &VarRef) -> R<DataLoc> {
        match r {
            VarRef::Local(n) => self.var_loc(&self.f.name, n),
            VarRef::Global(n) => self.global_loc(n),
        }
    }

    fn ref_holder(&self, r: &VarRef) -> R<HolderName> {
        match r {
            VarRef::Local(n) => self.var_holder(n),
            VarRef::Global(n) => self.global_holder(n),
        }
    }

    fn kind_of(&self, e: &Expr) -> R<Kind> {
        Ok(match e {
            Expr::Int(_) => Kind::Int,
            Expr::Bool(_) => Kind::Bool,
            Expr::Data(_) => Kind::Data,
            Expr::Var(n) => self.var_kind(n)?,
            Expr::Global(n) => self.global_kind(n)?,
            Expr::Unary(UnOp::Neg, a) => {
                self.expect_score(a, "negation")?;
                Kind::Int
            }
            Expr::Unary(UnOp::Not, a) => {
                self.expect_score(a, "`not`")?;
                Kind::Bool
            }
            Expr::Binary(op, a, b) => {
                self.expect_score(a, "a binary operator")?;
                self.expect_score(b, "a binary operator")?;
                if is_arith(*op) { Kind::Int } else { Kind::Bool }
            }
            Expr::Call(n, _) => match self.funcs.get(n.as_str()).and_then(|f| f.ret) {
                Some(k) => k,
                None => err!("function `{n}` has no return value"),
            },
            Expr::ToInt(a) => {
                if self.kind_of(a)? != Kind::Data {
                    err!("ToInt needs a Data value");
                }
                Kind::Int
            }
            Expr::ToData(a) => {
                self.expect_score(a, "ToData")?;
                Kind::Data
            }
            Expr::Field(r, _) => {
                self.expect_data_ref(r)?;
                Kind::Data
            }
            Expr::Exists(r, _) => {
                self.expect_data_ref(r)?;
                Kind::Bool
            }
            Expr::Len(r, _) => {
                self.expect_data_ref(r)?;
                Kind::Int
            }
            Expr::PlayerScore { .. } => Kind::Int,
            Expr::EntityExists(_) | Expr::CommandSuccess { .. } => Kind::Bool,
        })
    }

    fn expect_score(&self, e: &Expr, what: &str) -> R<()> {
        if !self.kind_of(e)?.is_score() {
            err!("{what} needs an Int/Bool operand but got a Data value");
        }
        Ok(())
    }

    fn expect_data_ref(&self, r: &VarRef) -> R<()> {
        if self.ref_kind(r)? != Kind::Data {
            err!("{r:?} is not a Data variable");
        }
        Ok(())
    }

    // ---- scoreboard emit helpers

    fn set_const(&mut self, h: &HolderName, v: i32) {
        self.emit(Command::score_set(fake(h), self.obj.clone(), v));
    }

    fn copy_score(&mut self, dst: &HolderName, src: &HolderName) {
        if dst != src {
            self.emit(Command::score_op(fake(dst), self.obj.clone(), ScoreOp::Set, fake(src), self.obj.clone()));
        }
    }

    fn assign_int(&mut self, dst: &HolderName, v: &IntVal) {
        match v {
            IntVal::Const(c) => self.set_const(dst, *c),
            IntVal::Holder(h, _) => self.copy_score(dst, h),
        }
    }

    fn apply_arith(&mut self, op: BinOp, d: &HolderName, rv: &IntVal) -> R<()> {
        let via_holder = |this: &mut Self, h: &HolderName, o: ScoreOp| {
            this.emit(Command::score_op(fake(d), this.obj.clone(), o, fake(h), this.obj.clone()));
        };
        let sop = match op {
            BinOp::Add => ScoreOp::Add,
            BinOp::Sub => ScoreOp::Sub,
            BinOp::Mul => ScoreOp::Mul,
            BinOp::Div => ScoreOp::Div,
            BinOp::Mod => ScoreOp::Mod,
            _ => unreachable!(),
        };
        match rv {
            IntVal::Holder(h, _) => via_holder(self, h, sop),
            IntVal::Const(c) => match op {
                BinOp::Add | BinOp::Sub if *c == i32::MIN => {
                    let ch = self.const_holder(*c)?;
                    via_holder(self, &ch, sop);
                }
                BinOp::Add | BinOp::Sub => {
                    // add c == remove -c; skip no-ops
                    let delta = if op == BinOp::Add { *c } else { -*c };
                    if delta != 0 {
                        self.emit(Command::score_add(fake(d), self.obj.clone(), delta));
                    }
                }
                _ => {
                    let ch = self.const_holder(*c)?;
                    via_holder(self, &ch, sop);
                }
            },
        }
        Ok(())
    }

    // ---- integer expressions

    /// Evaluates a scoreboard-class expression. `hint` is a holder the caller will store the
    /// result in; it's used as the destination when that is safe.
    fn eval_int(&mut self, e: &Expr, hint: Option<&HolderName>) -> R<IntVal> {
        if !self.kind_of(e)?.is_score() {
            err!("expected an Int/Bool expression");
        }
        match e {
            Expr::Int(v) => Ok(IntVal::Const(*v)),
            Expr::Bool(b) => Ok(IntVal::Const(*b as i32)),
            Expr::Var(n) => Ok(IntVal::Holder(self.var_holder(n)?, false)),
            Expr::Global(n) => Ok(IntVal::Holder(self.global_holder(n)?, false)),
            Expr::Unary(UnOp::Neg, a) => {
                let v = self.eval_int(a, None)?;
                match v {
                    IntVal::Const(c) => Ok(IntVal::Const(c.wrapping_neg())),
                    IntVal::Holder(h, temp) => {
                        let (d, d_temp) = if temp { (h.clone(), true) } else { self.dest(hint)? };
                        self.copy_score(&d, &h);
                        let m1 = self.const_holder(-1)?;
                        self.emit(Command::score_op(fake(&d), self.obj.clone(), ScoreOp::Mul, fake(&m1), self.obj.clone()));
                        Ok(IntVal::Holder(d, d_temp))
                    }
                }
            }
            Expr::Unary(UnOp::Not, _) => self.bool_value(e, hint),
            Expr::Binary(op, a, b) if is_arith(*op) => self.eval_arith(*op, a, b, hint),
            Expr::Binary(..) | Expr::Exists(..) | Expr::EntityExists(_) => self.bool_value(e, hint),
            Expr::CommandSuccess { command, args } => {
                // always a fresh temp: the arguments may read the variable this is assigned to
                let d = self.alloc_int()?;
                // `execute store` writes nothing if the chain ends early, so clear first
                self.set_const(&d, 0);
                if args.is_empty() {
                    let cmd = Command::execute().store(StoreKind::Success, StoreTarget::Score { targets: fake(&d), objective: self.obj.clone() }).run(command.clone());
                    self.emit(cmd);
                } else {
                    self.call_macro(command.clone(), args, Some(&d))?;
                }
                Ok(IntVal::Holder(d, true))
            }
            Expr::PlayerScore { objective, holder } => {
                let (d, temp) = self.dest(hint)?;
                let cmd = Command::execute()
                    .store(StoreKind::Result, StoreTarget::Score { targets: fake(&d), objective: self.obj.clone() })
                    .run(Command::Scoreboard(Scoreboard::PlayersGet { target: holder.clone(), objective: Objective::new(objective)? }));
                self.emit(cmd);
                Ok(IntVal::Holder(d, temp))
            }
            Expr::Call(name, args) => {
                self.lower_call(name, args)?;
                let (d, temp) = self.dest(hint)?;
                let ret = self.ret_holder()?;
                self.copy_score(&d, &ret);
                Ok(IntVal::Holder(d, temp))
            }
            Expr::ToInt(a) => {
                let dv = self.eval_data(a)?;
                self.data_to_int(dv, hint)
            }
            Expr::Len(r, path) => {
                let loc = self.ref_loc(r)?.extend(path)?;
                self.data_to_int(DataVal::Loc(loc, false), hint)
            }
            Expr::Data(_) | Expr::ToData(_) | Expr::Field(..) => unreachable!("rejected by kind_of"),
        }
    }

    fn dest(&mut self, hint: Option<&HolderName>) -> R<(HolderName, bool)> {
        match hint {
            Some(h) => Ok((h.clone(), false)),
            None => Ok((self.alloc_int()?, true)),
        }
    }

    fn data_to_int(&mut self, dv: DataVal, hint: Option<&HolderName>) -> R<IntVal> {
        match dv {
            DataVal::Const(Nbt::Byte(v)) => Ok(IntVal::Const(v as i32)),
            DataVal::Const(Nbt::Short(v)) => Ok(IntVal::Const(v as i32)),
            DataVal::Const(Nbt::Int(v)) => Ok(IntVal::Const(v)),
            DataVal::Const(other) => err!("cannot convert the constant {other} to an Int at compile time"),
            DataVal::Loc(loc, temp) => {
                let (d, d_temp) = self.dest(hint)?;
                let cmd = Command::execute()
                    .store(StoreKind::Result, StoreTarget::Score { targets: fake(&d), objective: self.obj.clone() })
                    .run(Command::Data(Data::Get { target: loc.target(), path: Some(loc.path.clone()), scale: None }));
                self.emit(cmd);
                self.release_data(&DataVal::Loc(loc, temp));
                Ok(IntVal::Holder(d, d_temp))
            }
        }
    }

    /// Copies a non-temp holder into a fresh temp so later side effects can't change it.
    fn protect(&mut self, v: IntVal) -> R<IntVal> {
        match v {
            IntVal::Holder(h, false) => {
                let t = self.alloc_int()?;
                self.copy_score(&t, &h);
                Ok(IntVal::Holder(t, true))
            }
            other => Ok(other),
        }
    }

    fn eval_arith(&mut self, op: BinOp, a: &Expr, b: &Expr, hint: Option<&HolderName>) -> R<IntVal> {
        let mut lv = self.eval_int(a, None)?;
        if has_call(b) {
            lv = self.protect(lv)?;
        }
        let rv = self.eval_int(b, None)?;
        if let (IntVal::Const(x), IntVal::Const(y)) = (&lv, &rv) {
            return Ok(IntVal::Const(fold(op, *x, *y)?));
        }
        if matches!(op, BinOp::Div | BinOp::Mod) && matches!(rv, IntVal::Const(0)) {
            err!("division by zero");
        }
        // Destination: the caller's hint when the right operand doesn't read it, else reuse a
        // temp we own, else allocate.
        let rv_reads = |h: &HolderName| matches!(&rv, IntVal::Holder(r, _) if r == h);
        let (d, d_temp) = match (hint, &lv) {
            (Some(h), _) if !rv_reads(h) => (h.clone(), false),
            (_, IntVal::Holder(t, true)) => (t.clone(), true),
            _ => (self.alloc_int()?, true),
        };
        self.assign_int(&d, &lv);
        if let IntVal::Holder(t, true) = &lv
            && *t != d {
                self.free_int(t);
            }
        self.apply_arith(op, &d, &rv)?;
        self.release_int(&rv);
        Ok(IntVal::Holder(d, d_temp))
    }

    /// A Bool-valued expression materialised as 0/1.
    fn bool_value(&mut self, e: &Expr, hint: Option<&HolderName>) -> R<IntVal> {
        let c = self.cond(e, false)?;
        let result = match &c.kind {
            CondKind::True => IntVal::Const(1),
            CondKind::False => IntVal::Const(0),
            CondKind::Tests(tests) => {
                let (d, d_temp) = self.dest(hint)?;
                self.store_success(&d, tests)?;
                IntVal::Holder(d, d_temp)
            }
        };
        for t in &c.temps {
            self.free_int(t);
        }
        Ok(result)
    }

    // ---- conditions

    /// Materialises "all of `tests` hold" as 0/1 in `dest`.
    ///
    /// `execute store` only writes when the chain reaches its last condition. If an earlier test
    /// fails the chain just ends and the old value stays (verified on 26.3), so with more than one
    /// test the destination is cleared first. A single, final test stores its own 0.
    fn store_success(&mut self, dest: &HolderName, tests: &[Test]) -> R<()> {
        if tests.len() > 1 {
            self.set_const(dest, 0);
        }
        let cmd = apply_tests(Command::execute().store(StoreKind::Success, StoreTarget::Score { targets: fake(dest), objective: self.obj.clone() }), tests).into_test()?;
        self.emit(cmd);
        Ok(())
    }

    fn cond(&mut self, e: &Expr, negate: bool) -> R<Cond> {
        if self.kind_of(e)? == Kind::Data {
            err!("a condition must be Bool, not Data");
        }
        match e {
            Expr::Bool(b) => Ok(Cond::constant(*b != negate)),
            Expr::Unary(UnOp::Not, a) => self.cond(a, !negate),
            Expr::Binary(BinOp::And, a, b) => self.and_or(true, a, b, negate),
            Expr::Binary(BinOp::Or, a, b) => self.and_or(false, a, b, negate),
            Expr::Binary(op, a, b) if is_cmp(*op) => self.compare(*op, a, b, negate),
            Expr::Exists(r, path) => {
                let loc = self.ref_loc(r)?.extend(path)?;
                let test = Test { negate, cond: Condition::Data(loc.target(), loc.path.clone()) };
                Ok(Cond { kind: CondKind::Tests(vec![test]), temps: Vec::new() })
            }
            Expr::EntityExists(sel) => {
                let test = Test { negate, cond: Condition::Entity(sel.clone()) };
                Ok(Cond { kind: CondKind::Tests(vec![test]), temps: Vec::new() })
            }
            other => {
                let v = self.eval_int(other, None)?;
                match v {
                    IntVal::Const(c) => Ok(Cond::constant((c != 0) != negate)),
                    IntVal::Holder(h, temp) => {
                        let test = Test {
                            negate,
                            cond: Condition::Score { target: fake(&h), objective: self.obj.clone(), test: ScoreTest::Matches(IntRange::exactly(1)) },
                        };
                        Ok(Cond { kind: CondKind::Tests(vec![test]), temps: if temp { vec![h] } else { Vec::new() } })
                    }
                }
            }
        }
    }

    /// `a && b` (is_and) or `a || b`, possibly negated.
    fn and_or(&mut self, is_and: bool, a: &Expr, b: &Expr, negate: bool) -> R<Cond> {
        if has_call(a) || has_call(b) {
            err!("calls are not allowed inside && / || operands (no short circuit); hoist them into locals first");
        }
        // De Morgan: !(a && b) == !a || !b and !(a || b) == !a && !b
        let conjunction = is_and != negate;
        if conjunction {
            let ca = self.cond(a, negate)?;
            let cb = self.cond(b, negate)?;
            let mut temps = ca.temps;
            temps.extend(cb.temps);
            let kind = match (ca.kind, cb.kind) {
                (CondKind::False, _) | (_, CondKind::False) => CondKind::False,
                (CondKind::True, k) | (k, CondKind::True) => k,
                (CondKind::Tests(mut x), CondKind::Tests(y)) => {
                    x.extend(y);
                    CondKind::Tests(x)
                }
            };
            return Ok(Cond { kind, temps });
        }
        // Disjunction: materialise into a flag set by each disjunct.
        let flag = self.alloc_int()?;
        self.set_const(&flag, 0);
        let mut always = false;
        for part in [a, b] {
            let c = self.cond(part, negate)?;
            match &c.kind {
                CondKind::True => always = true,
                CondKind::False => {}
                CondKind::Tests(tests) => {
                    let cmd = apply_tests(Command::execute(), tests).run(Command::score_set(fake(&flag), self.obj.clone(), 1));
                    self.emit(cmd);
                }
            }
            for t in &c.temps {
                self.free_int(t);
            }
        }
        if always {
            self.free_int(&flag);
            return Ok(Cond::constant(true));
        }
        let test = Test {
            negate: false,
            cond: Condition::Score { target: fake(&flag), objective: self.obj.clone(), test: ScoreTest::Matches(IntRange::exactly(1)) },
        };
        Ok(Cond { kind: CondKind::Tests(vec![test]), temps: vec![flag] })
    }

    fn compare(&mut self, op: BinOp, a: &Expr, b: &Expr, negate: bool) -> R<Cond> {
        let mut lv = self.eval_int(a, None)?;
        if has_call(b) {
            lv = self.protect(lv)?;
        }
        let rv = self.eval_int(b, None)?;
        // `!=` is `unless ==`
        let (op, negate) = if op == BinOp::Ne { (BinOp::Eq, !negate) } else { (op, negate) };
        let mut temps = Vec::new();
        for v in [&lv, &rv] {
            if let IntVal::Holder(h, true) = v {
                temps.push(h.clone());
            }
        }
        let score = |this: &Self, target: &HolderName, test: ScoreTest| Condition::Score { target: fake(target), objective: this.obj.clone(), test };
        let cond = match (&lv, &rv) {
            (IntVal::Const(x), IntVal::Const(y)) => return Ok(Cond::constant(cmp_const(op, *x, *y) != negate)),
            (IntVal::Holder(h, _), IntVal::Const(c)) | (IntVal::Const(c), IntVal::Holder(h, _)) => {
                let op = if matches!(lv, IntVal::Const(_)) { flip(op) } else { op };
                match range_for(op, *c) {
                    Some(range) => score(self, h, ScoreTest::Matches(range)),
                    None => {
                        // range not expressible at the i32 edge: compare against a constant holder
                        let ch = self.const_holder(*c)?;
                        score(self, h, ScoreTest::Compare(to_cmp(op), fake(&ch), self.obj.clone()))
                    }
                }
            }
            (IntVal::Holder(l, _), IntVal::Holder(r, _)) => score(self, l, ScoreTest::Compare(to_cmp(op), fake(r), self.obj.clone())),
        };
        Ok(Cond { kind: CondKind::Tests(vec![Test { negate, cond }]), temps })
    }

    // ---- data expressions

    fn eval_data(&mut self, e: &Expr) -> R<DataVal> {
        if self.kind_of(e)? != Kind::Data {
            err!("expected a Data expression");
        }
        match e {
            Expr::Data(n) => Ok(DataVal::Const(n.clone())),
            Expr::Var(n) => Ok(DataVal::Loc(self.var_loc(&self.f.name.clone(), n)?, false)),
            Expr::Global(n) => Ok(DataVal::Loc(self.global_loc(n)?, false)),
            Expr::Field(r, path) => Ok(DataVal::Loc(self.ref_loc(r)?.extend(path)?, false)),
            Expr::Call(name, args) => {
                self.lower_call(name, args)?;
                let t = self.alloc_data()?;
                let ret = self.ret_loc()?;
                self.assign_data(&t, &DataVal::Loc(ret.clone(), false));
                self.drop_data(&ret);
                Ok(DataVal::Loc(t, true))
            }
            Expr::ToData(a) => match self.eval_int(a, None)? {
                IntVal::Const(c) => Ok(DataVal::Const(Nbt::Int(c))),
                IntVal::Holder(h, temp) => {
                    let t = self.alloc_data()?;
                    let cmd = Command::execute()
                        .store(StoreKind::Result, StoreTarget::Data { target: t.target(), path: t.path.clone(), ty: NumType::Int, scale: 1.0 })
                        .run(Command::Scoreboard(Scoreboard::PlayersGet { target: fake(&h), objective: self.obj.clone() }));
                    self.emit(cmd);
                    self.release_int(&IntVal::Holder(h, temp));
                    Ok(DataVal::Loc(t, true))
                }
            },
            _ => unreachable!("rejected by kind_of"),
        }
    }

    fn assign_data(&mut self, dst: &DataLoc, v: &DataVal) {
        let source = match v {
            DataVal::Const(n) => ModifySource::Value(n.clone()),
            DataVal::Loc(l, _) => {
                if l.storage == dst.storage && l.path == dst.path {
                    return;
                }
                ModifySource::From { source: l.target(), path: Some(l.path.clone()) }
            }
        };
        self.emit(Command::Data(Data::Modify { target: dst.target(), path: dst.path.clone(), op: ModifyOp::Set, source }));
    }

    // ---- calls and the recursion stack

    fn needs_save(&self, callee: &str) -> bool {
        callee == self.f.name || self.reaches.get(callee).is_some_and(|r| r.contains(&self.f.name))
    }

    fn lower_call(&mut self, name: &str, args: &[Expr]) -> R<CallResult> {
        let callee: &'a ir::Function = match self.funcs.get(name) {
            Some(f) => f,
            None => err!("unknown function `{name}`"),
        };
        if callee.params.len() != args.len() {
            err!("`{name}` takes {} argument(s) but {} were given", callee.params.len(), args.len());
        }
        let self_call = name == self.f.name;

        // 1. evaluate arguments left to right
        enum Arg {
            Int(IntVal),
            Data(DataVal),
        }
        let mut vals = Vec::new();
        for (i, (arg, param)) in args.iter().zip(&callee.params).enumerate() {
            let later_impure = args[i + 1..].iter().any(has_call);
            if param.kind.is_score() {
                if !self.kind_of(arg)?.is_score() {
                    err!("argument {} of `{name}` must be Int/Bool", i + 1);
                }
                let mut v = self.eval_int(arg, None)?;
                // Self calls overwrite the caller's own slots while copying, so every argument
                // must be a stable temp; later impure arguments could change a variable too.
                if later_impure || self_call {
                    v = self.protect(v)?;
                }
                vals.push(Arg::Int(v));
            } else {
                if self.kind_of(arg)? != Kind::Data {
                    err!("argument {} of `{name}` must be Data", i + 1);
                }
                let mut v = self.eval_data(arg)?;
                if (later_impure || self_call)
                    && let DataVal::Loc(l, false) = &v {
                        let t = self.alloc_data()?;
                        self.assign_data(&t, &DataVal::Loc(l.clone(), false));
                        v = DataVal::Loc(t, true);
                    }
                vals.push(Arg::Data(v));
            }
        }

        // 2. argument temps are consumed right after the save, so don't save them. They only leave
        //    the pool here; the Data ones are removed once copied into the parameters (step 4).
        let mut dead_args: Vec<DataLoc> = Vec::new();
        for v in &vals {
            match v {
                Arg::Int(v) => self.release_int(v),
                Arg::Data(DataVal::Loc(l, true)) => {
                    self.free_data(l);
                    dead_args.push(l.clone());
                }
                Arg::Data(_) => {}
            }
        }

        // 3. save our live slots if the callee can re-enter us
        let save = if self.needs_save(name) { self.live_slots()? } else { Vec::new() };
        self.save_slots(&save)?;

        // 4. pass arguments
        for (v, param) in vals.iter().zip(&callee.params) {
            match v {
                Arg::Int(v) => {
                    let ph = self.param_holder(name, &param.name)?;
                    self.assign_int(&ph, v);
                }
                Arg::Data(v) => {
                    let pl = self.var_loc(name, &param.name)?;
                    self.assign_data(&pl, v);
                }
            }
        }

        for l in &dead_args {
            self.drop_data(l);
        }

        // 5. call, then restore
        self.emit(Command::call(self.fn_id(name)?));
        self.restore_slots(&save)?;
        Ok(match callee.ret {
            None => CallResult::None,
            Some(k) if k.is_score() => CallResult::Int,
            Some(_) => CallResult::Data,
        })
    }

    fn fn_id(&self, name: &str) -> R<ResourceLocation> {
        Ok(ResourceLocation::new(&self.ns, name)?)
    }

    /// Everything of the current function that is live: declared variables and allocated temps.
    fn live_slots(&self) -> R<Vec<Slot>> {
        let mut slots = Vec::new();
        for (name, kind) in &self.f.declared {
            slots.push(if kind.is_score() { Slot::Score(self.var_holder(name)?) } else { Slot::Data(self.var_loc(&self.f.name, name)?) });
        }
        for (i, used) in self.f.int_temps.iter().enumerate() {
            if *used {
                slots.push(Slot::Score(self.holder(format!("#{}.t_{i}", self.f.name))?));
            }
        }
        for (i, used) in self.f.data_temps.iter().enumerate() {
            if *used {
                slots.push(Slot::Data(DataLoc { storage: self.storage("vars")?, path: NbtPath::new().key(self.f.name.clone()).key(format!("t_{i}")) }));
            }
        }
        Ok(slots)
    }

    fn save_slots(&mut self, slots: &[Slot]) -> R<()> {
        for s in slots {
            match s {
                Slot::Score(h) => {
                    let list = self.stack_loc("i")?;
                    let last = DataLoc { storage: list.storage.clone(), path: list.path.clone().index(-1) };
                    self.emit(Command::Data(Data::Modify { target: list.target(), path: list.path.clone(), op: ModifyOp::Append, source: ModifySource::Value(Nbt::Int(0)) }));
                    let cmd = Command::execute()
                        .store(StoreKind::Result, StoreTarget::Data { target: last.target(), path: last.path.clone(), ty: NumType::Int, scale: 1.0 })
                        .run(Command::Scoreboard(Scoreboard::PlayersGet { target: fake(h), objective: self.obj.clone() }));
                    self.emit(cmd);
                }
                Slot::Data(loc) => {
                    let list = self.stack_loc("d")?;
                    let last = DataLoc { storage: list.storage.clone(), path: list.path.clone().index(-1) };
                    // placeholder first so the stack stays aligned if the slot is unset
                    self.emit(Command::Data(Data::Modify { target: list.target(), path: list.path.clone(), op: ModifyOp::Append, source: ModifySource::Value(Nbt::Int(0)) }));
                    self.assign_data(&last, &DataVal::Loc(loc.clone(), false));
                }
            }
        }
        Ok(())
    }

    fn restore_slots(&mut self, slots: &[Slot]) -> R<()> {
        for s in slots.iter().rev() {
            match s {
                Slot::Score(h) => {
                    let list = self.stack_loc("i")?;
                    let last = DataLoc { storage: list.storage.clone(), path: list.path.clone().index(-1) };
                    let cmd = Command::execute()
                        .store(StoreKind::Result, StoreTarget::Score { targets: fake(h), objective: self.obj.clone() })
                        .run(Command::Data(Data::Get { target: last.target(), path: Some(last.path.clone()), scale: None }));
                    self.emit(cmd);
                    self.emit(Command::Data(Data::Remove { target: last.target(), path: last.path }));
                }
                Slot::Data(loc) => {
                    let list = self.stack_loc("d")?;
                    let last = DataLoc { storage: list.storage.clone(), path: list.path.clone().index(-1) };
                    self.assign_data(loc, &DataVal::Loc(last.clone(), false));
                    self.emit(Command::Data(Data::Remove { target: last.target(), path: last.path }));
                }
            }
        }
        Ok(())
    }

    // ---- statements

    fn declare(&mut self, name: &str, kind: Kind) -> R<()> {
        check_ident("variable", name)?;
        if self.f.vars.insert(name.to_owned(), kind).is_some() {
            err!("variable `{name}` is declared twice in `{}`", self.f.name);
        }
        self.f.declared.push((name.to_owned(), kind));
        Ok(())
    }

    fn store_to_ref(&mut self, target: &VarRef, value: &Expr) -> R<()> {
        let tk = self.ref_kind(target)?;
        let vk = self.kind_of(value)?;
        if tk.is_score() != vk.is_score() {
            err!("cannot assign a {vk:?} value to {target:?} of kind {tk:?}");
        }
        if tk.is_score() {
            let dst = self.ref_holder(target)?;
            let v = self.eval_int(value, Some(&dst))?;
            self.assign_int(&dst, &v);
            self.release_int(&v);
        } else {
            let dst = self.ref_loc(target)?;
            let v = self.eval_data(value)?;
            self.assign_data(&dst, &v);
            self.release_data(&v);
        }
        Ok(())
    }

    fn lower_stmts(&mut self, stmts: &[Stmt]) -> R<()> {
        for s in stmts {
            let done = matches!(s, Stmt::Return(_));
            self.lower_stmt(s)?;
            if done {
                break; // anything after a return is unreachable
            }
        }
        Ok(())
    }

    fn lower_stmt(&mut self, s: &Stmt) -> R<()> {
        match s {
            Stmt::Let { name, kind, init } => {
                self.declare(name, *kind)?;
                if let Some(e) = init {
                    self.store_to_ref(&VarRef::Local(name.clone()), e)?;
                }
            }
            Stmt::Assign { target, value } => self.store_to_ref(target, value)?,
            Stmt::SetField { target, path, value } => {
                self.expect_data_ref(target)?;
                let dst = self.ref_loc(target)?.extend(path)?;
                let v = self.eval_data(value)?;
                self.assign_data(&dst, &v);
                self.release_data(&v);
            }
            Stmt::Append { target, path, value } => {
                self.expect_data_ref(target)?;
                let dst = self.ref_loc(target)?.extend(path)?;
                let v = self.eval_data(value)?;
                let source = match &v {
                    DataVal::Const(n) => ModifySource::Value(n.clone()),
                    DataVal::Loc(l, _) => ModifySource::From { source: l.target(), path: Some(l.path.clone()) },
                };
                self.emit(Command::Data(Data::Modify { target: dst.target(), path: dst.path, op: ModifyOp::Append, source }));
                self.release_data(&v);
            }
            Stmt::Expr(Expr::Call(name, args)) => {
                if let CallResult::Data = self.lower_call(name, args)? {
                    // the value is ignored, so don't leave it in the shared return slot
                    let ret = self.ret_loc()?;
                    self.drop_data(&ret);
                }
            }
            Stmt::Expr(other) => {
                // evaluate for effect; still type-check and keep any calls inside it
                if self.kind_of(other)?.is_score() {
                    let v = self.eval_int(other, None)?;
                    self.release_int(&v);
                } else {
                    let v = self.eval_data(other)?;
                    self.release_data(&v);
                }
            }
            Stmt::Print(e) => self.lower_print(e)?,
            Stmt::PrintParts(parts) => self.lower_print_parts(parts)?,
            Stmt::SetScore { objective, holder, op, value } => self.lower_set_score(objective, holder, *op, value)?,
            Stmt::Remove { target, path } => {
                self.expect_data_ref(target)?;
                let loc = self.ref_loc(target)?.extend(path)?;
                self.emit(Command::Data(Data::Remove { target: loc.target(), path: loc.path }));
            }
            Stmt::Context { clauses, args, body } => {
                if may_return(body) {
                    err!("a context block cannot return (it may run once per entity)");
                }
                let blk = self.block(|s, _| s.lower_stmts(body))?;
                let mut e = Execute::new();
                e.clauses = clauses.clone();
                let cmd = e.run(Command::call(blk));
                self.run_command(cmd, args)?;
            }
            Stmt::MacroCommand { command, args } => self.run_command(command.clone(), args)?,
            Stmt::ReadAt { dest, source, path, args } => {
                self.expect_data_ref(source)?;
                self.declare(dest, Kind::Data)?;
                let dest_loc = self.var_loc(&self.f.name.clone(), dest)?;
                let src = self.ref_loc(source)?.extend(path)?;
                let cmd = Command::Data(Data::Modify {
                    target: dest_loc.target(),
                    path: dest_loc.path,
                    op: ModifyOp::Set,
                    source: ModifySource::From { source: src.target(), path: Some(src.path) },
                });
                self.run_command(cmd, args)?;
            }
            Stmt::WriteAt { target, path, value, args } => {
                self.expect_data_ref(target)?;
                let dst = self.ref_loc(target)?.extend(path)?;
                let v = self.eval_data(value)?;
                let source = match &v {
                    DataVal::Const(n) => ModifySource::Value(n.clone()),
                    DataVal::Loc(l, _) => ModifySource::From { source: l.target(), path: Some(l.path.clone()) },
                };
                let cmd = Command::Data(Data::Modify { target: dst.target(), path: dst.path, op: ModifyOp::Set, source });
                self.run_command(cmd, args)?;
                self.release_data(&v);
            }
            Stmt::Command(c) => self.emit(c.clone()),
            Stmt::Return(value) => self.lower_return(value.as_ref())?,
            Stmt::If { cond, then, els } => self.lower_if(cond, then, els)?,
            Stmt::While { cond, body } => self.lower_while(cond, body)?,
        }
        Ok(())
    }

    /// Emits `cmd`; if it has holes, through a generated macro function (see [`Self::call_macro`]).
    fn run_command(&mut self, cmd: Command, args: &[(Hole, Expr)]) -> R<()> {
        if args.is_empty() {
            if cmd.is_macro() {
                err!("internal error: a command has macro placeholders but no values: {cmd}");
            }
            self.emit(cmd);
            return Ok(());
        }
        self.call_macro(cmd, args, None)
    }

    /// Runs a command whose arguments are only known at run time.
    ///
    /// Minecraft substitutes `$(key)` placeholders when a function is *called with arguments*, so
    /// the command becomes the only line of a helper function `<fn>/m<N>`, called with
    /// `function ... with storage <ns>:macro`. All argument expressions are evaluated first (they
    /// may call functions that use the scratch storage themselves) and written right before the
    /// call; the keys are removed afterwards so nothing is left behind.
    ///
    /// With `success`, the helper is `return run <cmd>` so that the command's own success flows out
    /// of the function call, and it is stored in that holder.
    fn call_macro(&mut self, cmd: Command, args: &[(Hole, Expr)], success: Option<&HolderName>) -> R<()> {
        enum Val {
            Int(IntVal),
            Data(DataVal),
        }
        let impure = args.iter().any(|(_, e)| has_call(e));
        let mut vals: Vec<(Hole, Val)> = Vec::new();
        for (hole, e) in args {
            if self.kind_of(e)?.is_score() {
                let mut v = self.eval_int(e, None)?;
                if impure {
                    v = self.protect(v)?;
                }
                vals.push((*hole, Val::Int(v)));
            } else {
                let mut v = self.eval_data(e)?;
                if impure && let DataVal::Loc(l, false) = &v {
                    let t = self.alloc_data()?;
                    self.assign_data(&t, &DataVal::Loc(l.clone(), false));
                    v = DataVal::Loc(t, true);
                }
                vals.push((*hole, Val::Data(v)));
            }
        }
        let scratch = self.storage("macro")?;
        let slot = |h: &Hole| DataLoc { storage: scratch.clone(), path: NbtPath::new().key(h.key()) };
        for (hole, v) in &vals {
            let loc = slot(hole);
            match v {
                Val::Int(IntVal::Const(c)) => self.assign_data(&loc, &DataVal::Const(Nbt::Int(*c))),
                Val::Int(IntVal::Holder(h, _)) => {
                    let store = Command::execute()
                        .store(StoreKind::Result, StoreTarget::Data { target: loc.target(), path: loc.path.clone(), ty: NumType::Int, scale: 1.0 })
                        .run(Command::Scoreboard(Scoreboard::PlayersGet { target: fake(h), objective: self.obj.clone() }));
                    self.emit(store);
                }
                Val::Data(d) => self.assign_data(&loc, d),
            }
        }
        let n = self.f.next_macro;
        self.f.next_macro += 1;
        let id = ResourceLocation::new(&self.ns, &format!("{}/m{n}", self.f.name))?;
        let mut helper = datapack::Function::new();
        helper.push(if success.is_some() { Command::Return(datapack::Return::Run(Box::new(cmd))) } else { cmd });
        self.pack.add_function(id.clone(), helper)?;
        let call = Command::Function { function: FunctionRef::Id(id), args: datapack::FnArgs::With(DataTarget::Storage(scratch.clone()), None) };
        match success {
            None => self.emit(call),
            Some(d) => {
                let wrapped = Command::execute().store(StoreKind::Success, StoreTarget::Score { targets: fake(d), objective: self.obj.clone() }).run(call);
                self.emit(wrapped);
            }
        }
        for (hole, v) in &vals {
            self.drop_data(&slot(hole));
            match v {
                Val::Int(i) => self.release_int(i),
                Val::Data(d) => self.release_data(d),
            }
        }
        Ok(())
    }

    fn lower_set_score(&mut self, objective: &str, holder: &ScoreHolder, op: Option<BinOp>, value: &Expr) -> R<()> {
        if !self.kind_of(value)?.is_score() {
            err!("a score can only be set to an Int/Bool value");
        }
        let obj = Objective::new(objective)?;
        let v = self.eval_int(value, None)?;
        let my_obj = self.obj.clone();
        let sop = |op: BinOp| match op {
            BinOp::Add => Ok(ScoreOp::Add),
            BinOp::Sub => Ok(ScoreOp::Sub),
            BinOp::Mul => Ok(ScoreOp::Mul),
            BinOp::Div => Ok(ScoreOp::Div),
            BinOp::Mod => Ok(ScoreOp::Mod),
            other => Err(LowerError { message: format!("`{other:?}` is not a score assignment operator") }),
        };
        match (op, &v) {
            (None, IntVal::Const(c)) => self.emit(Command::Scoreboard(Scoreboard::PlayersSet { targets: holder.clone(), objective: obj, value: *c })),
            (None, IntVal::Holder(h, _)) => self.emit(Command::score_op(holder.clone(), obj, ScoreOp::Set, fake(h), my_obj)),
            (Some(BinOp::Add), IntVal::Const(c)) if *c != i32::MIN => self.emit(Command::score_add(holder.clone(), obj, *c)),
            (Some(BinOp::Sub), IntVal::Const(c)) if *c != i32::MIN => self.emit(Command::score_add(holder.clone(), obj, -*c)),
            (Some(op), v) => {
                let src = match v {
                    IntVal::Const(c) => self.const_holder(*c)?,
                    IntVal::Holder(h, _) => h.clone(),
                };
                self.emit(Command::score_op(holder.clone(), obj, sop(op)?, fake(&src), my_obj));
            }
        }
        self.release_int(&v);
        Ok(())
    }

    fn lower_return(&mut self, value: Option<&Expr>) -> R<()> {
        let fname = self.f.name.clone();
        let ret = self.funcs[fname.as_str()].ret;
        match (ret, value) {
            (None, None) => {}
            (None, Some(_)) => err!("`{fname}` has no return type but returns a value"),
            (Some(_), None) => err!("`{fname}` must return a value"),
            (Some(k), Some(e)) => {
                if k.is_score() != self.kind_of(e)?.is_score() {
                    err!("`{fname}` returns {k:?} but the value is {:?}", self.kind_of(e)?);
                }
                if k.is_score() {
                    let r = self.ret_holder()?;
                    let v = self.eval_int(e, Some(&r))?;
                    self.assign_int(&r, &v);
                    self.release_int(&v);
                } else {
                    let r = self.ret_loc()?;
                    let v = self.eval_data(e)?;
                    self.assign_data(&r, &v);
                    self.release_data(&v);
                }
            }
        }
        self.emit_drop()?;
        // A non-zero result tells enclosing block calls (`execute if function`) that the
        // source-level function has returned.
        self.emit(Command::return_value(1));
        Ok(())
    }

    fn lower_print(&mut self, e: &Expr) -> R<()> {
        let mut cleanup: Option<DataLoc> = None;
        let text = if self.kind_of(e)?.is_score() {
            match self.eval_int(e, None)? {
                IntVal::Const(c) => Text::plain(c.to_string()),
                IntVal::Holder(h, temp) => {
                    let t = Text::score(fake(&h), self.obj.clone());
                    self.release_int(&IntVal::Holder(h, temp));
                    t
                }
            }
        } else {
            match self.eval_data(e)? {
                DataVal::Const(Nbt::String(s)) => Text::plain(s),
                DataVal::Const(n) => {
                    let t = self.alloc_data()?;
                    self.assign_data(&t, &DataVal::Const(n));
                    let text = Text::new(datapack::Content::Nbt { path: t.path.clone(), source: t.target(), interpret: false });
                    self.free_data(&t);
                    cleanup = Some(t);
                    text
                }
                DataVal::Loc(l, temp) => {
                    let t = Text::new(datapack::Content::Nbt { path: l.path.clone(), source: l.target(), interpret: false });
                    if temp {
                        self.free_data(&l);
                        cleanup = Some(l);
                    }
                    t
                }
            }
        };
        self.emit(Command::tellraw(Selector::a(), text));
        if let Some(l) = cleanup {
            self.drop_data(&l);
        }
        Ok(())
    }

    /// `tellraw @a` with a list of components: literal text, a `score` component for numbers and
    /// an `nbt` component for strings, lists and structs. Components read the values when the
    /// message is sent, so every temporary stays alive until the `tellraw` has been emitted.
    fn lower_print_parts(&mut self, parts: &[PrintPart]) -> R<()> {
        let mut text = Text::plain("");
        let mut int_temps: Vec<IntVal> = Vec::new();
        let mut data_temps: Vec<DataLoc> = Vec::new();
        for part in parts {
            let piece = match part {
                PrintPart::Lit(s) => Text::plain(s.clone()),
                PrintPart::Value(e) if self.kind_of(e)?.is_score() => match self.eval_int(e, None)? {
                    IntVal::Const(c) => Text::plain(c.to_string()),
                    IntVal::Holder(h, temp) => {
                        let t = Text::score(fake(&h), self.obj.clone());
                        int_temps.push(IntVal::Holder(h, temp));
                        t
                    }
                },
                PrintPart::Value(e) => match self.eval_data(e)? {
                    DataVal::Const(Nbt::String(s)) => Text::plain(s),
                    DataVal::Const(n) => {
                        let t = self.alloc_data()?;
                        self.assign_data(&t, &DataVal::Const(n));
                        let c = Text::new(datapack::Content::Nbt { path: t.path.clone(), source: t.target(), interpret: false });
                        data_temps.push(t);
                        c
                    }
                    DataVal::Loc(l, temp) => {
                        let c = Text::new(datapack::Content::Nbt { path: l.path.clone(), source: l.target(), interpret: false });
                        if temp {
                            data_temps.push(l);
                        }
                        c
                    }
                },
            };
            text = text.then(piece);
        }
        self.emit(Command::tellraw(Selector::a(), text));
        for v in &int_temps {
            self.release_int(v);
        }
        for l in &data_temps {
            self.free_data(l);
            self.drop_data(l);
        }
        Ok(())
    }

    /// Lowers `f` into a new `.mcfunction` and returns its id.
    fn block(&mut self, f: impl FnOnce(&mut Self, &ResourceLocation) -> R<()>) -> R<ResourceLocation> {
        let id = ResourceLocation::new(&self.ns, &format!("{}/b{}", self.f.name, self.f.next_block))?;
        self.f.next_block += 1;
        let outer = std::mem::take(&mut self.out);
        let result = f(self, &id);
        let body = std::mem::replace(&mut self.out, outer);
        result?;
        let mut func = datapack::Function::new();
        func.extend(body);
        self.pack.add_function(id.clone(), func)?;
        Ok(id)
    }

    /// `execute <tests> run function <blk>`, propagating a source-level `return` from the block.
    fn call_block(&mut self, tests: &[Test], blk: &ResourceLocation, returns: bool) -> R<()> {
        let e = apply_tests(Command::execute(), tests);
        let cmd = if returns {
            e.clause(Clause::If(Condition::Function(FunctionRef::Id(blk.clone())))).run(Command::return_value(1))
        } else {
            e.run(Command::call(blk.clone()))
        };
        self.emit(cmd);
        Ok(())
    }

    fn lower_if(&mut self, cond: &Expr, then: &[Stmt], els: &[Stmt]) -> R<()> {
        let c = self.cond(cond, false)?;
        let tests = match c.kind {
            CondKind::True => {
                for t in &c.temps {
                    self.free_int(t);
                }
                return self.lower_stmts(then);
            }
            CondKind::False => {
                for t in &c.temps {
                    self.free_int(t);
                }
                return self.lower_stmts(els);
            }
            CondKind::Tests(t) => t,
        };
        if els.is_empty() {
            for t in &c.temps {
                self.free_int(t);
            }
            let blk = self.block(|s, _| s.lower_stmts(then))?;
            return self.call_block(&tests, &blk, may_return(then));
        }
        // With an else branch the test is stored first: the `then` block may change what the
        // test reads, and re-evaluating it for the else would run both branches.
        let flag = self.alloc_int()?;
        self.store_success(&flag, &tests)?;
        for t in &c.temps {
            self.free_int(t);
        }
        let then_blk = self.block(|s, _| s.lower_stmts(then))?;
        let else_blk = self.block(|s, _| s.lower_stmts(els))?;
        let obj = self.obj.clone();
        let on = |negate| Test {
            negate,
            cond: Condition::Score { target: fake(&flag), objective: obj.clone(), test: ScoreTest::Matches(IntRange::exactly(1)) },
        };
        self.call_block(&[on(false)], &then_blk, may_return(then))?;
        self.call_block(&[on(true)], &else_blk, may_return(els))?;
        self.free_int(&flag);
        Ok(())
    }

    fn lower_while(&mut self, cond: &Expr, body: &[Stmt]) -> R<()> {
        // A constantly-false loop never runs; check up front without emitting anything.
        let blk = self.block(|s, me| {
            let c = s.cond(cond, false)?;
            match c.kind {
                CondKind::True => {}
                CondKind::False => s.emit(Command::Return(datapack::Return::Value(0))),
                CondKind::Tests(tests) => {
                    let exit_cmd = Command::Return(datapack::Return::Value(0));
                    if let [single] = tests.as_slice() {
                        let inverted = Test { negate: !single.negate, cond: single.cond.clone() };
                        s.emit(apply_tests(Command::execute(), &[inverted]).run(exit_cmd));
                    } else {
                        let flag = s.alloc_int()?;
                        s.store_success(&flag, &tests)?;
                        let unless = Test {
                            negate: true,
                            cond: Condition::Score { target: fake(&flag), objective: s.obj.clone(), test: ScoreTest::Matches(IntRange::exactly(1)) },
                        };
                        s.emit(apply_tests(Command::execute(), &[unless]).run(exit_cmd));
                        s.free_int(&flag);
                    }
                }
            }
            for t in &c.temps {
                s.free_int(t);
            }
            s.lower_stmts(body)?;
            // tail call: the loop is this function re-entering itself
            s.emit(Command::Return(datapack::Return::Run(Box::new(Command::call(me.clone())))));
            Ok(())
        })?;
        self.call_block(&[], &blk, may_return(body))
    }

    // ---- functions and pack assembly

    fn lower_function(&mut self, f: &'a ir::Function) -> R<()> {
        self.f = FnState::new(&f.name);
        self.out = Vec::new();
        for p in &f.params {
            self.declare(&p.name, p.kind)?;
        }
        let mut drop: Vec<String> = f.params.iter().filter(|p| p.kind == Kind::Data).map(|p| p.name.clone()).collect();
        collect_data_locals(&f.body, &mut drop);
        self.f.drop = drop;
        self.lower_stmts(&f.body)?;
        if !matches!(f.body.last(), Some(Stmt::Return(_))) {
            self.emit_drop()?;
        }
        let mut func = datapack::Function::new();
        func.extend(std::mem::take(&mut self.out));
        self.pack.add_function(self.fn_id(&f.name)?, func)?;
        if self.f.drop.len() >= 2 {
            let mut d = datapack::Function::new();
            for name in self.f.drop.clone() {
                let loc = self.var_loc(&f.name, &name)?;
                d.push(Command::Data(Data::Remove { target: loc.target(), path: loc.path }));
            }
            self.pack.add_function(ResourceLocation::new(&self.ns, &format!("{}/_drop", f.name))?, d)?;
        }
        if f.entry == Some(Entry::Tick) {
            self.pack.on_tick(self.fn_id(&f.name)?);
        }
        Ok(())
    }

    fn emit_init(&mut self) -> R<()> {
        self.f = FnState::new("_init");
        self.out = Vec::new();
        self.emit(Command::Scoreboard(Scoreboard::ObjectivesAdd { name: self.obj.clone(), criteria: datapack::Criteria::Dummy, display_name: None }));
        let prog = self.prog;
        for o in &prog.objectives {
            if o.name == self.obj.as_str() {
                err!("the objective `{}` is reserved for the compiler's own variables; rename it", o.name);
            }
            let criteria = match o.criteria.as_str() {
                "dummy" => datapack::Criteria::Dummy,
                "trigger" => datapack::Criteria::Trigger,
                other => datapack::Criteria::Custom(other.to_owned()),
            };
            self.emit(Command::Scoreboard(Scoreboard::ObjectivesAdd { name: Objective::new(&o.name)?, criteria, display_name: None }));
        }
        let consts: Vec<i32> = self.consts.iter().copied().collect();
        for c in consts {
            let h = self.const_holder(c)?;
            self.set_const(&h, c);
        }
        for g in &prog.globals {
            self.emit_global_init(g)?;
        }
        for f in &prog.functions {
            if f.entry == Some(Entry::Load) {
                self.emit(Command::call(self.fn_id(&f.name)?));
            }
        }
        let mut func = datapack::Function::new();
        func.comment("generated by slab: runs on every load; initialises state once");
        func.extend(std::mem::take(&mut self.out));
        let id = self.fn_id("_init")?;
        self.pack.add_function(id.clone(), func)?;
        self.pack.on_load(id);
        Ok(())
    }

    /// `<ns>:_uninstall`: removes everything this pack ever created. Pack data cannot detect that
    /// the pack was removed, so cleaning up at the end means running this function (docs/lang/memory.md).
    fn emit_uninstall(&mut self) -> R<()> {
        let prog = self.prog;
        let mut f = datapack::Function::new();
        f.comment("generated by slab: removes all state this pack created. Run it, then `/datapack disable` the pack.");
        f.extend(prog.uninstall.iter().cloned());
        for o in &prog.objectives {
            f.push(Command::Scoreboard(Scoreboard::ObjectivesRemove(Objective::new(&o.name)?)));
        }
        f.push(Command::Scoreboard(Scoreboard::ObjectivesRemove(self.obj.clone())));
        let remove = |storage: ResourceLocation, key: &str| Command::Data(Data::Remove { target: DataTarget::Storage(storage), path: NbtPath::new().key(key) });
        for func in &prog.functions {
            f.push(remove(self.storage("vars")?, &func.name));
        }
        for g in prog.globals.iter().filter(|g| g.kind == Kind::Data) {
            f.push(remove(self.storage("globals")?, &g.name));
        }
        f.push(remove(self.storage("ret")?, "v"));
        f.push(remove(self.storage("stack")?, "i"));
        f.push(remove(self.storage("stack")?, "d"));
        self.pack.add_function(self.fn_id("_uninstall")?, f)?;
        Ok(())
    }

    fn emit_global_init(&mut self, g: &'a ir::Global) -> R<()> {
        if g.kind.is_score() {
            let v = match &g.init {
                Expr::Int(v) => *v,
                Expr::Bool(b) => *b as i32,
                _ => err!("initial value of global `{}` must be an Int/Bool constant", g.name),
            };
            let h = self.global_holder(&g.name)?;
            let unset = Condition::Score {
                target: fake(&h),
                objective: self.obj.clone(),
                test: ScoreTest::Matches(IntRange::between(i32::MIN, i32::MAX)),
            };
            let cmd = Command::execute().unless(unset).run(Command::score_set(fake(&h), self.obj.clone(), v));
            self.emit(cmd);
        } else {
            let n = match &g.init {
                Expr::Data(n) => n.clone(),
                _ => err!("initial value of global `{}` must be a Data constant", g.name),
            };
            let loc = self.global_loc(&g.name)?;
            let set = Command::Data(Data::Modify { target: loc.target(), path: loc.path.clone(), op: ModifyOp::Set, source: ModifySource::Value(n) });
            let cmd = Command::execute().unless(Condition::Data(loc.target(), loc.path.clone())).run(set);
            self.emit(cmd);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum Slot {
    Score(HolderName),
    Data(DataLoc),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datapack::{rl, word};
    use crate::ir::Function as F;

    fn text(pack: &DataPack, id: &str) -> String {
        pack.function(&rl(id)).unwrap_or_else(|| panic!("no function {id}")).render()
    }

    fn lowered(f: Vec<F>) -> DataPack {
        let mut p = Program::new("t");
        p.functions = f;
        lower(&p).unwrap()
    }

    fn lower_err(f: Vec<F>) -> String {
        let mut p = Program::new("t");
        p.functions = f;
        lower(&p).unwrap_err().message
    }

    fn let_int(name: &str, e: Expr) -> Stmt {
        Stmt::Let { name: name.into(), kind: Kind::Int, init: Some(e) }
    }

    #[test]
    fn constants_fold_and_assign_directly() {
        let pack = lowered(vec![F::new("main").body(vec![let_int("x", Expr::bin(BinOp::Add, Expr::Int(1), Expr::Int(2)))])]);
        assert_eq!(text(&pack, "t:main"), "scoreboard players set #main.v_x t 3\n");
    }

    #[test]
    fn arithmetic_writes_into_the_target() {
        let pack = lowered(vec![F::new("main").body(vec![
            let_int("x", Expr::Int(5)),
            Stmt::Assign { target: VarRef::Local("x".into()), value: Expr::bin(BinOp::Add, Expr::var("x"), Expr::Int(3)) },
        ])]);
        assert_eq!(text(&pack, "t:main"), "scoreboard players set #main.v_x t 5\nscoreboard players add #main.v_x t 3\n");
    }

    #[test]
    fn non_recursive_calls_do_not_touch_the_stack() {
        let pack = lowered(vec![
            F::new("helper").param("n", Kind::Int).returns(Kind::Int).body(vec![Stmt::Return(Some(Expr::var("n")))]),
            F::new("main").body(vec![let_int("r", Expr::call("helper", vec![Expr::Int(4)]))]),
        ]);
        let main = text(&pack, "t:main");
        assert!(!main.contains("stack"), "{main}");
        assert!(main.contains("function t:helper"), "{main}");
    }

    #[test]
    fn recursive_calls_save_and_restore() {
        let pack = lowered(vec![F::new("f").param("n", Kind::Int).returns(Kind::Int).body(vec![
            Stmt::Return(Some(Expr::call("f", vec![Expr::var("n")]))),
        ])]);
        let f = text(&pack, "t:f");
        let save = f.find("append value 0").expect("saves");
        let call = f.find("function t:f").expect("calls");
        let restore = f.find("data get storage t:stack i[-1]").expect("restores");
        assert!(save < call && call < restore, "{f}");
    }

    #[test]
    fn if_else_stores_the_test_first() {
        let pack = lowered(vec![F::new("main").body(vec![
            let_int("x", Expr::Int(1)),
            Stmt::If { cond: Expr::bin(BinOp::Lt, Expr::var("x"), Expr::Int(3)), then: vec![Stmt::Print(Expr::Int(1))], els: vec![Stmt::Print(Expr::Int(2))] },
        ])]);
        let main = text(&pack, "t:main");
        assert!(main.contains("execute store success score #main.t_0 t if score #main.v_x t matches ..2"), "{main}");
        assert!(main.contains("execute if score #main.t_0 t matches 1 run function t:main/b0"), "{main}");
        assert!(main.contains("execute unless score #main.t_0 t matches 1 run function t:main/b1"), "{main}");
    }

    #[test]
    fn multi_test_flags_are_cleared_first() {
        // `execute store success` writes nothing when an earlier test fails, so a stale 1 would stick
        let pack = lowered(vec![F::new("main").body(vec![
            let_int("x", Expr::Int(1)),
            Stmt::Let { name: "b".into(), kind: Kind::Bool, init: Some(Expr::bin(BinOp::And, Expr::bin(BinOp::Gt, Expr::var("x"), Expr::Int(0)), Expr::bin(BinOp::Lt, Expr::var("x"), Expr::Int(5)))) },
        ])]);
        let main = text(&pack, "t:main");
        let clear = main.find("scoreboard players set #main.v_b t 0").expect(&main);
        let store = main.find("execute store success score #main.v_b t if score").expect(&main);
        assert!(clear < store, "{main}");
    }

    #[test]
    fn constant_conditions_drop_dead_branches() {
        let pack = lowered(vec![F::new("main").body(vec![Stmt::If {
            cond: Expr::Bool(false),
            then: vec![Stmt::Print(Expr::Int(1))],
            els: vec![Stmt::Print(Expr::Int(2))],
        }])]);
        assert_eq!(text(&pack, "t:main"), "tellraw @a \"2\"\n");
    }

    #[test]
    fn while_loops_are_self_tail_calls_even_with_nested_blocks() {
        let pack = lowered(vec![F::new("main").body(vec![
            let_int("i", Expr::Int(0)),
            Stmt::While {
                cond: Expr::bin(BinOp::Lt, Expr::var("i"), Expr::Int(3)),
                body: vec![Stmt::If { cond: Expr::bin(BinOp::Eq, Expr::var("i"), Expr::Int(1)), then: vec![Stmt::Print(Expr::Int(1))], els: vec![] }],
            },
        ])]);
        let lp = text(&pack, "t:main/b0");
        assert!(lp.ends_with("return run function t:main/b0\n"), "{lp}");
        assert!(lp.starts_with("execute unless score #main.v_i t matches ..2 run return 0\n"), "{lp}");
    }

    #[test]
    fn errors_are_reported() {
        assert!(lower_err(vec![F::new("main").body(vec![Stmt::Expr(Expr::call("nope", vec![]))])]).contains("unknown function"));
        assert!(lower_err(vec![F::new("main").body(vec![let_int("x", Expr::Int(1)), let_int("x", Expr::Int(2))])]).contains("declared twice"));
        assert!(lower_err(vec![F::new("main").body(vec![let_int("x", Expr::bin(BinOp::Div, Expr::Int(1), Expr::Int(0)))])]).contains("division by zero"));
        assert!(lower_err(vec![F::new("main").body(vec![Stmt::Assign { target: VarRef::Local("zzz".into()), value: Expr::Int(1) }])]).contains("unknown variable"));
        assert!(lower_err(vec![F::new("Bad-Name")]).contains("invalid function name"));
        assert!(lower_err(vec![F::new("e").param("p", Kind::Int).entry(Entry::Tick)]).contains("cannot take parameters"));
        assert!(lower_err(vec![
            F::new("h").param("a", Kind::Int),
            F::new("main").body(vec![Stmt::Expr(Expr::call("h", vec![]))]),
        ])
        .contains("takes 1 argument"));
        // Data into an Int variable
        assert!(lower_err(vec![F::new("main").body(vec![let_int("x", Expr::Data(Nbt::str("s")))])]).contains("cannot assign"));
        // calls inside && / ||
        let call_in_and = Expr::bin(BinOp::And, Expr::Bool(true), Expr::bin(BinOp::Eq, Expr::call("h", vec![]), Expr::Int(1)));
        assert!(lower_err(vec![
            F::new("h").returns(Kind::Int).body(vec![Stmt::Return(Some(Expr::Int(1)))]),
            F::new("main").body(vec![Stmt::If { cond: call_in_and, then: vec![], els: vec![] }]),
        ])
        .contains("no short circuit"));
        // returning a value from a function without a return type
        assert!(lower_err(vec![F::new("main").body(vec![Stmt::Return(Some(Expr::Int(1)))])]).contains("no return type"));
    }

    #[test]
    fn init_sets_up_objective_constants_globals_and_load_entries() {
        let mut p = Program::new("t").global("count", Kind::Int, Expr::Int(7)).global("s", Kind::Data, Expr::Data(Nbt::str("x")));
        p.functions = vec![
            F::new("main").body(vec![let_int("x", Expr::Int(2)), Stmt::Assign { target: VarRef::Local("x".into()), value: Expr::bin(BinOp::Mul, Expr::var("x"), Expr::Int(3)) }]),
            F::new("boot").entry(Entry::Load),
            F::new("tick").entry(Entry::Tick),
        ];
        let pack = lower(&p).unwrap();
        let init = text(&pack, "t:_init");
        assert!(init.contains("scoreboard objectives add t dummy"));
        assert!(init.contains("scoreboard players set #c_3 t 3"), "{init}");
        assert!(init.contains("execute unless score #g.count t matches -2147483648..2147483647 run scoreboard players set #g.count t 7"), "{init}");
        assert!(init.contains("execute unless data storage t:globals s run data modify storage t:globals s set value \"x\""), "{init}");
        assert!(init.contains("function t:boot"));
        let files = pack.files().unwrap();
        assert!(files["data/minecraft/tags/function/load.json"].starts_with(b"{"));
        assert!(String::from_utf8_lossy(&files["data/minecraft/tags/function/tick.json"]).contains("t:tick"));
    }

    fn data_let(name: &str, init: Expr) -> Stmt {
        Stmt::Let { name: name.into(), kind: Kind::Data, init: Some(init) }
    }

    #[test]
    fn a_data_local_is_removed_on_every_exit() {
        // exits: the early return inside the `if` block and falling off the end
        let pack = lowered(vec![F::new("f").param("flag", Kind::Int).body(vec![
            data_let("s", Expr::Data(Nbt::str("x"))),
            Stmt::If { cond: Expr::bin(BinOp::Eq, Expr::var("flag"), Expr::Int(1)), then: vec![Stmt::Return(None)], els: vec![] },
            Stmt::Print(Expr::var("s")),
        ])]);
        let remove = "data remove storage t:vars f.v_s\n";
        let main = text(&pack, "t:f");
        assert!(main.ends_with(remove), "falls off the end: {main}");
        assert_eq!(text(&pack, "t:f/b0"), format!("{remove}return 1\n"), "early return");
    }

    #[test]
    fn several_data_locals_share_one_drop_function() {
        let pack = lowered(vec![F::new("f").body(vec![data_let("a", Expr::Data(Nbt::str("x"))), data_let("b", Expr::Data(Nbt::List(vec![])))])]);
        assert!(text(&pack, "t:f").ends_with("function t:f/_drop\n"));
        assert_eq!(text(&pack, "t:f/_drop"), "data remove storage t:vars f.v_a\ndata remove storage t:vars f.v_b\n");
    }

    #[test]
    fn the_return_value_is_copied_before_its_source_is_dropped() {
        let pack = lowered(vec![F::new("echo").param("s", Kind::Data).returns(Kind::Data).body(vec![Stmt::Return(Some(Expr::var("s")))])]);
        assert_eq!(text(&pack, "t:echo"), "data modify storage t:ret v set from storage t:vars echo.v_s\ndata remove storage t:vars echo.v_s\nreturn 1\n");
    }

    #[test]
    fn a_data_result_is_taken_out_of_the_return_slot() {
        let pack = lowered(vec![
            F::new("make").returns(Kind::Data).body(vec![Stmt::Return(Some(Expr::Data(Nbt::str("v"))))]),
            F::new("main").body(vec![data_let("x", Expr::call("make", vec![])), Stmt::Expr(Expr::call("make", vec![]))]),
        ]);
        let main = text(&pack, "t:main");
        // the first result is copied into x and the slot cleared; the ignored second one is cleared too
        assert_eq!(main.matches("data remove storage t:ret v").count(), 2, "{main}");
        assert!(main.contains("data modify storage t:vars main.v_x set from storage t:vars main.t_0"), "{main}");
    }

    #[test]
    fn scoreboard_slots_are_not_reset_while_running() {
        let pack = lowered(vec![F::new("f").body(vec![let_int("x", Expr::Int(1))])]);
        assert!(!text(&pack, "t:f").contains("reset"));
    }

    #[test]
    fn uninstall_removes_everything_the_program_created() {
        let mut p = Program::new("t").global("g", Kind::Data, Expr::Data(Nbt::str("x"))).global("n", Kind::Int, Expr::Int(0));
        p.objectives = vec![ir::ObjectiveDecl { name: "pts".into(), criteria: "dummy".into() }];
        p.uninstall = vec![Command::Kill(Some(Selector::e().tag(word("t.owned"))))];
        p.functions = vec![F::new("main")];
        let pack = lower(&p).unwrap();
        let u = text(&pack, "t:_uninstall");
        let want = [
            "kill @e[tag=t.owned]",
            "scoreboard objectives remove pts",
            "scoreboard objectives remove t",
            "data remove storage t:vars main",
            "data remove storage t:globals g",
            "data remove storage t:ret v",
            "data remove storage t:stack i",
            "data remove storage t:stack d",
        ];
        let positions: Vec<usize> = want.iter().map(|w| u.find(w).unwrap_or_else(|| panic!("missing `{w}` in\n{u}"))).collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "entities first, then scoreboard, then storage:\n{u}");
        assert!(!u.contains("globals n"), "scoreboard globals live on the objective, not in storage");
    }

    #[test]
    fn floor_folding_matches_the_scoreboard() {
        assert_eq!(fold(BinOp::Div, -7, 2).unwrap(), -4);
        assert_eq!(fold(BinOp::Div, 7, -2).unwrap(), -4);
        assert_eq!(fold(BinOp::Mod, -7, 2).unwrap(), 1);
        assert_eq!(fold(BinOp::Mod, 3, -4).unwrap(), -1);
        assert_eq!(fold(BinOp::Div, i32::MIN, -1).unwrap(), i32::MIN);
        assert_eq!(fold(BinOp::Add, i32::MAX, 1).unwrap(), i32::MIN);
        assert_eq!(fold(BinOp::Mul, 100000, 100000).unwrap(), 1410065408);
    }
}
