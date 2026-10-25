//! The typed, name-resolved tree produced by the checker. Every expression carries its type,
//! every name is an id, constants are inlined and selectors are already typed
//! `datapack::Selector`s. Lowering to the IR can rely on all of that having been checked.

use super::ty::{EnumId, StructId, Ty};
use crate::datapack::Selector;
use crate::diag::Span;
use crate::syntax::ast::{AssignOp, BinOp, UnOp};
use crate::syntax::token::RelKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlobalId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FuncId(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectiveId(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub struct Hir {
    /// Module names, index 0 is the root (`main`); nested modules look like `maps::arena`.
    pub modules: Vec<String>,
    /// `files[m]` is the source-file index (for diagnostics) of module `m`.
    pub files: Vec<usize>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub globals: Vec<GlobalDef>,
    pub objectives: Vec<ObjectiveDef>,
    pub fns: Vec<FnDef>,
}

/// An enum declaration. A value is stored as `{t: <variant index>, f0: .., f1: ..}`.
#[derive(Clone, Debug, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub module: usize,
    /// The names of the type parameters (`T`, `E`).
    pub generics: Vec<String>,
    pub variants: Vec<VariantDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantDef {
    pub name: String,
    /// Payload types, which may mention the enum's `Ty::Param`s.
    pub fields: Vec<Ty>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub module: usize,
    pub fields: Vec<(String, Ty)>,
}

/// A compile-time value (constants, global initialisers).
#[derive(Clone, Debug, PartialEq)]
pub enum ConstVal {
    Int(i32),
    Bool(bool),
    Str(String),
    List(Vec<ConstVal>),
    /// Field values in declaration order.
    Struct(StructId, Vec<ConstVal>),
    /// `()`, the payload of `Ok(())`.
    Unit,
    /// A variant of an enum (its index in declaration order) with its payload values.
    Enum(EnumId, usize, Vec<ConstVal>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlobalDef {
    pub name: String,
    pub module: usize,
    pub ty: Ty,
    pub init: ConstVal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveDef {
    pub name: String,
    pub module: usize,
    pub criteria: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Load,
    Tick,
    /// Every N ticks (N >= 1).
    Every(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalInfo {
    pub name: String,
    pub ty: Ty,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FnDef {
    pub name: String,
    pub module: usize,
    pub params: Vec<LocalId>,
    pub ret: Ty,
    pub locals: Vec<LocalInfo>,
    pub body: Block,
    pub entry: Option<EntryKind>,
    pub span: Span,
    /// Declared in the embedded standard library: only generated when something calls it.
    pub std: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Let { local: LocalId, init: Expr },
    /// `target` is an lvalue: `Local`, `Global`, `Field`, `Index` or `Score`.
    Assign { target: Expr, op: AssignOp, value: Expr },
    If { cond: Expr, then: Block, els: Block },
    While { cond: Expr, body: Block },
    Loop { body: Block },
    ForRange { var: LocalId, start: Expr, end: Expr, inclusive: bool, body: Block },
    ForList { var: LocalId, list: Expr, body: Block },
    Return(Option<Expr>),
    Break,
    Continue,
    Context { clauses: Vec<CtxClause>, body: Block },
    Block(Block),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum CtxClause {
    As(Expr),
    At(Expr),
    In(String),
    Positioned([Expr; 3]),
    Rotated([Expr; 2]),
    Facing([Expr; 3]),
    /// Axes in x, y, z order, e.g. `"xz"`.
    Align(String),
    /// `eyes` or `feet`.
    Anchored(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum FormatPart {
    Lit(String),
    Expr(Expr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MethodKind {
    ListPush,
    ListLen,
    StrLen,
    /// A method of the standard library the compiler implements (`option.unwrap`,
    /// `result.is_ok`, ...); the code generator matches on the id.
    Intrinsic(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(i32),
    Bool(bool),
    Str(String),
    /// An f-string; its type is `text`.
    Format(Vec<FormatPart>),
    List(Vec<Expr>),
    /// Field values in declaration order.
    Struct { id: StructId, fields: Vec<Expr> },
    /// `Some(x)`, `None`, `Option::Some(x)`: variant number `index` of the enum with its payload.
    Variant { enum_id: EnumId, index: usize, args: Vec<Expr> },
    /// `()`.
    Unit,
    Local(LocalId),
    Global(GlobalId),
    /// A selector literal. The expressions fill the run-time parts of its options (`$name`,
    /// `${expression}`): `Hole(k)` of the selector is `holes[k]`; empty for a plain selector.
    Selector(Selector, Vec<Expr>),
    /// The entity handle of a selector literal that matches (at most) one entity.
    ToEntity(Box<Expr>),
    Rel { kind: RelKind, offset: Option<String> },
    Unary { op: UnOp, expr: Box<Expr> },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Call { func: FuncId, args: Vec<Expr> },
    /// A builtin function by its full name (`"print"`, `"effect.give"`). Arguments that must be
    /// compile-time constants are already literals.
    Builtin { name: &'static str, args: Vec<Expr> },
    Method { kind: MethodKind, recv: Box<Expr>, args: Vec<Expr> },
    Field { base: Box<Expr>, index: usize, name: String },
    Index { base: Box<Expr>, index: Box<Expr> },
    /// `objective[holder]`: a scoreboard score (an lvalue).
    Score { objective: ObjectiveId, holder: Box<Expr> },
    /// Produced after an error; never reaches lowering because errors stop the pipeline.
    Error,
}

impl Expr {
    fn calls(&self, out: &mut Vec<FuncId>) {
        match &self.kind {
            ExprKind::Call { func, args } => {
                out.push(*func);
                args.iter().for_each(|a| a.calls(out));
            }
            ExprKind::Format(parts) => {
                for p in parts {
                    if let FormatPart::Expr(e) = p {
                        e.calls(out);
                    }
                }
            }
            ExprKind::List(es) | ExprKind::Struct { fields: es, .. } | ExprKind::Variant { args: es, .. } | ExprKind::Selector(_, es) | ExprKind::Builtin { args: es, .. } => es.iter().for_each(|e| e.calls(out)),
            ExprKind::ToEntity(e) | ExprKind::Unary { expr: e, .. } | ExprKind::Field { base: e, .. } | ExprKind::Score { holder: e, .. } => e.calls(out),
            ExprKind::Binary { lhs, rhs, .. } => {
                lhs.calls(out);
                rhs.calls(out);
            }
            ExprKind::Method { recv, args, .. } => {
                recv.calls(out);
                args.iter().for_each(|a| a.calls(out));
            }
            ExprKind::Index { base, index } => {
                base.calls(out);
                index.calls(out);
            }
            ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Unit | ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::Rel { .. } | ExprKind::Error => {}
        }
    }

    pub fn error(span: Span) -> Expr {
        Expr { kind: ExprKind::Error, ty: Ty::Error, span }
    }
}

impl Block {
    /// Every function called (directly) in the block.
    pub fn calls(&self, out: &mut Vec<FuncId>) {
        for s in &self.stmts {
            s.calls(out);
        }
    }

    /// Whether control can never reach the end of the block normally (it always returns,
    /// breaks, continues or loops forever).
    pub fn diverges(&self) -> bool {
        self.stmts.iter().any(Stmt::diverges)
    }

    /// Whether a `break` that targets the loop enclosing this block occurs in it (nested loops
    /// have their own).
    pub fn breaks(&self) -> bool {
        self.stmts.iter().any(Stmt::breaks)
    }
}

impl Stmt {
    fn calls(&self, out: &mut Vec<FuncId>) {
        match self {
            Stmt::Let { init, .. } => init.calls(out),
            Stmt::Assign { target, value, .. } => {
                target.calls(out);
                value.calls(out);
            }
            Stmt::If { cond, then, els } => {
                cond.calls(out);
                then.calls(out);
                els.calls(out);
            }
            Stmt::While { cond, body } => {
                cond.calls(out);
                body.calls(out);
            }
            Stmt::Loop { body } => body.calls(out),
            Stmt::ForRange { start, end, body, .. } => {
                start.calls(out);
                end.calls(out);
                body.calls(out);
            }
            Stmt::ForList { list, body, .. } => {
                list.calls(out);
                body.calls(out);
            }
            Stmt::Return(e) => {
                if let Some(e) = e {
                    e.calls(out);
                }
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Context { clauses, body } => {
                for c in clauses {
                    match c {
                        CtxClause::As(e) | CtxClause::At(e) => e.calls(out),
                        CtxClause::Positioned(es) | CtxClause::Facing(es) => es.iter().for_each(|e| e.calls(out)),
                        CtxClause::Rotated(es) => es.iter().for_each(|e| e.calls(out)),
                        CtxClause::In(_) | CtxClause::Align(_) | CtxClause::Anchored(_) => {}
                    }
                }
                body.calls(out);
            }
            Stmt::Block(b) => b.calls(out),
            Stmt::Expr(e) => e.calls(out),
        }
    }

    pub fn diverges(&self) -> bool {
        match self {
            Stmt::Return(_) | Stmt::Break | Stmt::Continue => true,
            Stmt::If { then, els, .. } => then.diverges() && els.diverges(),
            Stmt::Block(b) => b.diverges(),
            // `loop` only ends through `break`
            Stmt::Loop { body } => !body.breaks(),
            _ => false,
        }
    }

    fn breaks(&self) -> bool {
        match self {
            Stmt::Break => true,
            Stmt::If { then, els, .. } => then.breaks() || els.breaks(),
            Stmt::Block(b) => b.breaks(),
            Stmt::Context { body, .. } => body.breaks(),
            _ => false,
        }
    }
}
