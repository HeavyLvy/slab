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
}

impl BRet {
    pub fn ty(self) -> Ty {
        match self {
            BRet::Unit => Ty::Unit,
            BRet::Bool => Ty::Bool,
            BRet::Entity => Ty::Entity,
        }
    }
}

pub struct Builtin {
    pub name: &'static str,
    pub params: &'static [BParam],
    /// Trailing parameters beyond this count are optional.
    pub min_args: usize,
    pub ret: BRet,
    /// One-line human description, shown on hover by the language server.
    pub docs: &'static str,
}

impl BParam {
    /// The generic parameter name used when displaying a signature
    /// (`tp(selector, coord, coord, coord)`).
    pub fn label(self) -> &'static str {
        match self {
            Printable => "value",
            ConstStr => "const str",
            Str => "str",
            ConstInt => "const int",
            Int => "int",
            Selector => "selector",
            Coord => "coord",
        }
    }
}

use BParam::*;

pub const BUILTINS: &[Builtin] = &[
    Builtin { name: "print", params: &[Printable], min_args: 1, ret: BRet::Unit, docs: "Show a message to every player via `tellraw` with text components; accepts any number of values." },
    Builtin { name: "say", params: &[Str], min_args: 1, ret: BRet::Unit, docs: "Show a raw chat message as if typed by the server." },
    Builtin { name: "kill", params: &[Selector], min_args: 1, ret: BRet::Bool, docs: "Kill the entities matching the selector; true when anything was killed." },
    Builtin { name: "exists", params: &[Selector], min_args: 1, ret: BRet::Bool, docs: "Whether the selector matches at least one entity right now." },
    Builtin { name: "tag.add", params: &[Selector, Str], min_args: 2, ret: BRet::Bool, docs: "Add a tag to the matching entities; true when anything was tagged." },
    Builtin { name: "tag.remove", params: &[Selector, Str], min_args: 2, ret: BRet::Bool, docs: "Remove a tag from the matching entities; true when anything was untagged." },
    Builtin { name: "tag.has", params: &[Selector, ConstStr], min_args: 2, ret: BRet::Bool, docs: "Whether every matching entity carries the tag." },
    Builtin { name: "effect.give", params: &[Selector, Str, Int, Int], min_args: 2, ret: BRet::Bool, docs: "Give an effect to the matching entities: duration and amplifier, both in ticks." },
    Builtin { name: "effect.clear", params: &[Selector], min_args: 1, ret: BRet::Bool, docs: "Clear (all or the named, if given) effects from the matching entities." },
    Builtin { name: "tp", params: &[Selector, Coord, Coord, Coord], min_args: 4, ret: BRet::Bool, docs: "Teleport the matching entities to the coordinates; true when it happened." },
    Builtin { name: "setblock", params: &[Coord, Coord, Coord, Str], min_args: 4, ret: BRet::Bool, docs: "Place a block at the coordinates; true when the block was set." },
    Builtin { name: "summon", params: &[Str, Coord, Coord, Coord], min_args: 1, ret: BRet::Entity, docs: "Summon an entity at the coordinates and yield a handle to it." },
];

/// Names that act as a namespace for builtins (`effect.give`, `tag.add`).
pub const MODULES: &[&str] = &["effect", "tag"];

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

pub fn is_module(name: &str) -> bool {
    MODULES.contains(&name)
}
