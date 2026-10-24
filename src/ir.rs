//! The mid-level IR between the language frontend and the data pack generator.
//!
//! Structured (blocks, `if`, `while`) rather than a flat CFG, because mcfunction control flow
//! is function calls anyway. Runtime model (see docs/lang/ir.md):
//!
//! * `Int` and `Bool` values live on the scoreboard (i32, wrapping, floored division).
//! * `Data` values (strings, lists, compounds, any NBT) live in command storage.
//!
//! The IR is untyped beyond that split; the frontend does real type checking and the lowering
//! only verifies the scoreboard/storage class of every operation.

use crate::datapack::{Clause, Command, Hole, Nbt, NbtPath, ScoreHolder, Selector};

/// Where a value lives at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Int,
    /// Stored like `Int`, always 0 or 1.
    Bool,
    /// Anything NBT, stored in command storage.
    Data,
}

impl Kind {
    pub fn is_score(self) -> bool {
        !matches!(self, Kind::Data)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    /// Floored, like the scoreboard's `/=`. Division by zero is a compile error for constants
    /// and leaves the left operand unchanged at runtime (the command fails).
    Div,
    /// Floored modulo: the result has the divisor's sign.
    Mod,
    Lt,
    Le,
    Eq,
    Ne,
    Ge,
    Gt,
    /// Both operands are always evaluated (no short circuit), so they must not contain calls.
    And,
    /// Same restriction as `And`.
    Or,
}

/// A variable reference: a local/parameter of the current function, or a global.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VarRef {
    Local(String),
    Global(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i32),
    Bool(bool),
    /// An NBT literal.
    Data(Nbt),
    Var(String),
    Global(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    /// Data -> Int: numeric tags floor to an int, lists/strings/compounds give their length.
    ToInt(Box<Expr>),
    /// Int -> Data (an int tag).
    ToData(Box<Expr>),
    /// `var.path` of a Data variable.
    Field(VarRef, NbtPath),
    /// Bool: whether `var.path` exists.
    Exists(VarRef, NbtPath),
    /// Int: length of the list/string at `var.path`.
    Len(VarRef, NbtPath),
    /// Int: a score on a user objective (`scoreboard players get`).
    PlayerScore { objective: String, holder: ScoreHolder },
    /// Bool: whether any entity matches the selector (`execute if entity`).
    EntityExists(Selector),
}

impl Expr {
    pub fn var(name: &str) -> Expr {
        Expr::Var(name.to_owned())
    }

    pub fn global(name: &str) -> Expr {
        Expr::Global(name.to_owned())
    }

    pub fn call(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Call(name.to_owned(), args)
    }

    pub fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::Binary(op, Box::new(l), Box::new(r))
    }

    pub fn logical_not(e: Expr) -> Expr {
        Expr::Unary(UnOp::Not, Box::new(e))
    }

    pub fn negate(e: Expr) -> Expr {
        Expr::Unary(UnOp::Neg, Box::new(e))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// Declares a local with an optional initial value. Names are unique within a function;
    /// the frontend renames shadowed variables.
    Let { name: String, kind: Kind, init: Option<Expr> },
    Assign { target: VarRef, value: Expr },
    /// `target.path = value` on a Data variable.
    SetField { target: VarRef, path: NbtPath, value: Expr },
    /// Appends a Data value to the list at `target.path`.
    Append { target: VarRef, path: NbtPath, value: Expr },
    If { cond: Expr, then: Vec<Stmt>, els: Vec<Stmt> },
    /// Loops are tail-recursive functions, so one tick can run at most about
    /// `max_command_sequence_length` (default 65536) commands in total.
    While { cond: Expr, body: Vec<Stmt> },
    Return(Option<Expr>),
    /// Evaluate for effect (calls).
    Expr(Expr),
    /// Show a value to all players with `tellraw`.
    Print(Expr),
    /// Show text assembled from parts (an f-string) as one `tellraw` message.
    PrintParts(Vec<PrintPart>),
    /// Writes a score on a user objective; `op` of `None` assigns, otherwise `score op= value`
    /// (only Add, Sub, Mul, Div and Mod).
    SetScore { objective: String, holder: ScoreHolder, op: Option<BinOp>, value: Expr },
    /// Removes the element or field at `target.path` (`data remove`).
    Remove { target: VarRef, path: NbtPath },
    /// Runs `body` under `/execute` modifiers. The body is its own function, so it may run once
    /// per matching entity. It must not `Return`. `args` supply the [`Hole`]s used in `clauses`.
    Context { clauses: Vec<Clause>, args: Vec<(Hole, Expr)>, body: Vec<Stmt> },
    /// A typed command containing [`Hole`]s (rendered as `$(hN)`): the lowering stores the values
    /// of `args` in the macro scratch storage and runs the command through a generated macro
    /// function. Used wherever a command argument is only known at run time.
    MacroCommand { command: Command, args: Vec<(Hole, Expr)> },
    /// Declares `dest` (a Data local) and sets it to the value at `source.path`, where `path`
    /// may contain index holes (`xs[$(h0)]`) filled from `args`.
    ReadAt { dest: String, source: VarRef, path: NbtPath, args: Vec<(Hole, Expr)> },
    /// `target.path = value` (a Data value) where `path` may contain index holes.
    WriteAt { target: VarRef, path: NbtPath, value: Expr, args: Vec<(Hole, Expr)> },
    /// A typed command from the datapack module, for builtins the IR doesn't model.
    Command(Command),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrintPart {
    Lit(String),
    /// Any value: ints and bools are shown as numbers, Data as its NBT text.
    Value(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Runs on every load/`/reload`, after globals are initialised. No parameters.
    Load,
    /// Runs every game tick. No parameters.
    Tick,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// `[a-z0-9_]+`; becomes the function `<namespace>:<name>`.
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Kind>,
    pub body: Vec<Stmt>,
    pub entry: Option<Entry>,
}

impl Function {
    pub fn new(name: &str) -> Self {
        Function { name: name.to_owned(), params: Vec::new(), ret: None, body: Vec::new(), entry: None }
    }

    pub fn param(mut self, name: &str, kind: Kind) -> Self {
        self.params.push(Param { name: name.to_owned(), kind });
        self
    }

    pub fn returns(mut self, kind: Kind) -> Self {
        self.ret = Some(kind);
        self
    }

    pub fn entry(mut self, entry: Entry) -> Self {
        self.entry = Some(entry);
        self
    }

    pub fn body(mut self, body: Vec<Stmt>) -> Self {
        self.body = body;
        self
    }
}

/// A global survives ticks and `/reload`. `init` must be a constant; it's applied once, the
/// first time the pack loads in a world.
#[derive(Clone, Debug, PartialEq)]
pub struct Global {
    pub name: String,
    pub kind: Kind,
    pub init: Expr,
}

/// A scoreboard objective the program declares (created on load).
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveDecl {
    pub name: String,
    /// `dummy`, `trigger`, `deathCount`, ...
    pub criteria: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// Pack namespace; also the scoreboard objective and the prefix of the storage ids.
    pub namespace: String,
    pub description: String,
    pub objectives: Vec<ObjectiveDecl>,
    /// Extra commands for the generated `_uninstall` function (kill entities the program
    /// summoned, remove tags it added, ...), run before the compiler removes its own state.
    pub uninstall: Vec<Command>,
    pub globals: Vec<Global>,
    pub functions: Vec<Function>,
}

impl Program {
    pub fn new(namespace: &str) -> Self {
        Program { namespace: namespace.to_owned(), description: format!("{namespace} (compiled by slab)"), objectives: Vec::new(), uninstall: Vec::new(), globals: Vec::new(), functions: Vec::new() }
    }

    pub fn global(mut self, name: &str, kind: Kind, init: Expr) -> Self {
        self.globals.push(Global { name: name.to_owned(), kind, init });
        self
    }

    pub fn function(mut self, f: Function) -> Self {
        self.functions.push(f);
        self
    }
}
