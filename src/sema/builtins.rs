//! Signatures of the builtin functions. Builtins that act on the world return `bool`: whether the
//! command succeeded (it targeted something, changed something, ...). Ignoring the result costs
//! nothing; using it adds one `execute store success`.
//! Whether the backend can lower each one yet is a
//! separate question (see `docs/lang/semantics.md`); this table is what the checker enforces.

use super::ty::Ty;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BParam {
    /// Any storable value.
    Printable,
    /// A string literal or string constant, known at compile time.
    ConstStr,
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
}

impl BRet {
    pub fn ty(self) -> Ty {
        match self {
            BRet::Unit => Ty::Unit,
            BRet::Bool => Ty::Bool,
        }
    }
}

pub struct Builtin {
    pub name: &'static str,
    pub params: &'static [BParam],
    /// Trailing parameters beyond this count are optional.
    pub min_args: usize,
    pub ret: BRet,
}

use BParam::*;

pub const BUILTINS: &[Builtin] = &[
    Builtin { name: "print", params: &[Printable], min_args: 1, ret: BRet::Unit },
    Builtin { name: "say", params: &[ConstStr], min_args: 1, ret: BRet::Unit },
    Builtin { name: "kill", params: &[Selector], min_args: 1, ret: BRet::Bool },
    Builtin { name: "exists", params: &[Selector], min_args: 1, ret: BRet::Bool },
    Builtin { name: "tag.add", params: &[Selector, ConstStr], min_args: 2, ret: BRet::Bool },
    Builtin { name: "tag.remove", params: &[Selector, ConstStr], min_args: 2, ret: BRet::Bool },
    Builtin { name: "tag.has", params: &[Selector, ConstStr], min_args: 2, ret: BRet::Bool },
    Builtin { name: "effect.give", params: &[Selector, ConstStr, Int, Int], min_args: 2, ret: BRet::Bool },
    Builtin { name: "effect.clear", params: &[Selector], min_args: 1, ret: BRet::Bool },
    Builtin { name: "tp", params: &[Selector, Coord, Coord, Coord], min_args: 4, ret: BRet::Bool },
    Builtin { name: "setblock", params: &[Coord, Coord, Coord, ConstStr], min_args: 4, ret: BRet::Bool },
    Builtin { name: "summon", params: &[ConstStr, Coord, Coord, Coord], min_args: 1, ret: BRet::Bool },
];

/// Names that act as a namespace for builtins (`effect.give`, `tag.add`).
pub const MODULES: &[&str] = &["effect", "tag"];

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

pub fn is_module(name: &str) -> bool {
    MODULES.contains(&name)
}
