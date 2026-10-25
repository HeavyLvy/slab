//! The syntax tree produced by the parser. Purely syntactic: no names are resolved and no
//! types are checked yet. Every node carries its source span for diagnostics.

use super::token::RelKind;
use crate::diag::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub attrs: Vec<Attr>,
    /// Declared with `pub`: visible to modules that import this one.
    pub public: bool,
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attr {
    pub name: Ident,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    Fn(FnDecl),
    Global(VarDecl),
    Const(VarDecl),
    Struct(StructDecl),
    /// `objective kills = "playerKillCount";`
    Objective { name: Ident, criteria: Expr },
    Import(ImportDecl),
}

/// `import a::b;`, `import a::b as c;`, `import a::b::{x, y};`, `import a::b::*;`
#[derive(Clone, Debug, PartialEq)]
pub struct ImportDecl {
    /// The module path: `a::b` is the file `a/b.slab` under the project root (`std` and `game`
    /// are built in).
    pub path: Vec<Ident>,
    pub alias: Option<Ident>,
    /// `Some` for the braced form, which imports these items instead of the module name.
    pub items: Option<Vec<Ident>>,
    /// The `*` form: every public item of the module, by its own name.
    pub glob: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FnDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: Type,
}

/// `global`/`const` declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct VarDecl {
    pub name: Ident,
    pub ty: Type,
    pub init: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructDecl {
    pub name: Ident,
    pub fields: Vec<Param>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    /// `utils::Point` has `module == Some(utils)`. A longer path is one identifier joined with
    /// `::` (`game::tag::Foo` has the module `game::tag`).
    pub module: Option<Ident>,
    pub name: Ident,
    /// `list<int>` has `args == [int]`.
    pub args: Vec<Type>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Let { name: Ident, mutable: bool, ty: Option<Type>, init: Expr },
    /// `target` is a name, a field access or an index expression.
    Assign { target: Expr, op: AssignOp, value: Expr },
    If { cond: Expr, then: Block, els: Option<Box<Else>> },
    While { cond: Expr, body: Block },
    Loop { body: Block },
    For { var: Ident, iter: Expr, body: Block },
    Return(Option<Expr>),
    Break,
    Continue,
    /// `as @a at @s { ... }`: a chain of `/execute` modifiers around a block.
    Context { clauses: Vec<CtxClause>, body: Block },
    Block(Block),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Else {
    Block(Block),
    If(Box<Stmt>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CtxClause {
    pub kind: CtxKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CtxKind {
    As(Expr),
    At(Expr),
    In(Expr),
    Positioned([Expr; 3]),
    Rotated([Expr; 2]),
    Facing([Expr; 3]),
    /// `align xz`: the axes as written.
    Align(Ident),
    /// `anchored eyes`
    Anchored(Ident),
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
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// A selector argument as written: `key=value`, with the value kept as raw text because it has
/// its own sub-grammar (`1..5`, `!tag`, `{kills=1..}`) that the type checker interprets.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectorArg {
    pub key: String,
    pub value: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectorLit {
    /// One of `p r a e s n`.
    pub var: char,
    pub args: Vec<SelectorArg>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldInit {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FormatPart {
    Lit(String),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(String),
    Bool(bool),
    Str(String),
    /// `f"x = {x}"`: text with embedded expressions.
    Format(Vec<FormatPart>),
    Ident(String),
    List(Vec<Expr>),
    /// `{ a: 1 }`
    Compound(Vec<FieldInit>),
    /// `Point { x: 1, y: 2 }`
    StructLit { module: Option<Ident>, name: Ident, fields: Vec<FieldInit> },
    /// `module::name`; the module of `a::b::name` is the identifier `a::b`.
    Path { module: Ident, name: Ident },
    Selector(SelectorLit),
    /// `~`, `~1`, `^-2`
    Rel { kind: RelKind, offset: Option<String> },
    Unary { op: UnOp, expr: Box<Expr> },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    /// `a..b`, `a..=b`, `..b`, `a..`
    Range { start: Option<Box<Expr>>, end: Option<Box<Expr>>, inclusive: bool },
    /// Method calls are `Call` on a `Field`: `xs.push(1)`.
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Field { base: Box<Expr>, name: Ident },
    Index { base: Box<Expr>, index: Box<Expr> },
}
