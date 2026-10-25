//! What the checker knows about intrinsics: functions of the embedded standard library
//! (`src/stdlib/*.slab`, docs/lang/stdlib.md) that have no body because the compiler
//! implements them. The signature comes from the declaration in the `.slab` file, built into
//! an [`Intrinsic`]; the code generator matches on the intrinsic's `id`.
//!
//! Intrinsics that act on the world return `bool`: whether the command succeeded (it targeted
//! something, changed something, ...). Ignoring the result costs nothing; using it adds one
//! `execute store success`.

use super::ty::Ty;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BParam {
    /// Any storable value.
    Printable,
    /// A string literal or string constant, known at compile time.
    ConstStr,
    /// Any string expression; a computed one is passed to the command at run time (a macro).
    Str,
    /// An integer literal or constant, known at compile time.
    ConstInt,
    /// Any integer expression; a variable is passed to the command at run time (a macro).
    Int,
    Selector,
    /// An integer expression or a relative coordinate (`~`, `^2`).
    Coord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BRet {
    Unit,
    Bool,
    /// A handle to the entity the builtin created (`summon`).
    Entity,
    /// `Result<int, ()>`: the integer result of a command, or an error when it failed.
    CommandResult,
}

impl BRet {
    pub fn ty(self) -> Ty {
        match self {
            BRet::Unit => Ty::Unit,
            BRet::Bool => Ty::Bool,
            BRet::Entity => Ty::Entity,
            // needs the `Result` enum of the standard library: see `Checker::intrinsic_ret`
            BRet::CommandResult => Ty::Error,
        }
    }
}

/// The intrinsics the code generator implements; `#[intrinsic("id")]` must name one of these.
pub const IDS: &[&str] = &[
    "print",
    "say",
    "kill",
    "exists",
    "tag.add",
    "tag.remove",
    "tag.has",
    "effect.give",
    "effect.clear",
    "tp",
    "setblock",
    "summon",
    "run_command",
];

/// The intrinsic *methods* (`impl<T> Option<T> { #[intrinsic("option.unwrap")] fn unwrap(self) -> T; }`)
/// the code generator implements. Their signatures use ordinary types, so they are not
/// [`Intrinsic`]s.
pub const METHOD_IDS: &[&str] = &[
    "option.is_some",
    "option.is_none",
    "option.unwrap",
    "option.unwrap_or",
    "option.expect",
    "result.is_ok",
    "result.is_err",
    "result.unwrap",
    "result.unwrap_or",
    "result.expect",
    "result.ok",
    "result.err",
];

pub fn method_id(id: &str) -> Option<&'static str> {
    METHOD_IDS.iter().find(|i| **i == id).copied()
}

/// The static spelling of an intrinsic id, if the compiler knows it.
pub fn intrinsic_id(id: &str) -> Option<&'static str> {
    IDS.iter().find(|i| **i == id).copied()
}

/// A function declared in the standard library without a body.
#[derive(Clone, Debug)]
pub struct Intrinsic {
    /// What the HIR and code generation call it: `summon`, `tag.add`.
    pub id: &'static str,
    /// The name inside its module: `add`.
    pub name: String,
    /// The module it is declared in: `game::tag`.
    pub module: String,
    pub params: Vec<BParam>,
    /// Trailing parameters beyond this count are optional.
    pub min_args: usize,
    pub ret: BRet,
}

impl Intrinsic {
    /// The full path as written in source: `game::tag::add`.
    pub fn path(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }
}

impl BParam {
    /// The parameter kind as shown in signatures (`game::tp(target, coord, coord, coord)`).
    pub fn label(self) -> &'static str {
        match self {
            BParam::Printable => "value",
            BParam::ConstStr => "const str",
            BParam::Str => "str",
            BParam::ConstInt => "const int",
            BParam::Int => "int",
            BParam::Selector => "target",
            BParam::Coord => "coord",
        }
    }

    /// The kind a declared parameter type stands for: the pseudo types `Target` (a selector or
    /// an `EntityId`), `Coord` and `any`, plus `int` and `str`, either `const` or not.
    pub fn from_type(name: &str, constant: bool) -> Option<BParam> {
        Some(match (name, constant) {
            ("Target", false) => BParam::Selector,
            ("Coord", false) => BParam::Coord,
            ("any", false) => BParam::Printable,
            ("str", false) => BParam::Str,
            ("str", true) => BParam::ConstStr,
            ("int", false) => BParam::Int,
            ("int", true) => BParam::ConstInt,
            _ => return None,
        })
    }
}
