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
    /// The built-in module it lives in: `std`, `game`, `game::tag`, `game::effect`.
    pub module: &'static str,
    /// The name inside the module: `summon`.
    pub name: &'static str,
    /// What the HIR and code generation call it: `summon`, `tag.add`. Unique across all builtins.
    pub id: &'static str,
    pub params: &'static [BParam],
    /// Trailing parameters beyond this count are optional.
    pub min_args: usize,
    pub ret: BRet,
    /// One-line human description, shown on hover by the language server.
    pub docs: &'static str,
}

impl Builtin {
    /// The full path as written in source: `game::tag::add`.
    pub fn path(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }
}

impl BParam {
    /// The generic parameter name used when displaying a signature
    /// (`game::tp(selector, coord, coord, coord)`).
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

macro_rules! b {
    ($module:literal, $name:literal, $id:literal, $params:expr, $min:expr, $ret:expr, $docs:literal) => {
        Builtin { module: $module, name: $name, id: $id, params: $params, min_args: $min, ret: $ret, docs: $docs }
    };
}

pub const BUILTINS: &[Builtin] = &[
    b!("std", "print", "print", &[Printable], 1, BRet::Unit, "Show a message to every player via `tellraw` with text components; accepts any number of values."),
    b!("game", "say", "say", &[Str], 1, BRet::Unit, "Show a raw chat message as if typed by the server."),
    b!("game", "kill", "kill", &[Selector], 1, BRet::Bool, "Kill the entities matching the selector; true when anything was killed."),
    b!("game", "exists", "exists", &[Selector], 1, BRet::Bool, "Whether the selector matches at least one entity right now."),
    b!("game::tag", "add", "tag.add", &[Selector, Str], 2, BRet::Bool, "Add a tag to the matching entities; true when anything was tagged."),
    b!("game::tag", "remove", "tag.remove", &[Selector, Str], 2, BRet::Bool, "Remove a tag from the matching entities; true when anything was untagged."),
    b!("game::tag", "has", "tag.has", &[Selector, ConstStr], 2, BRet::Bool, "Whether every matching entity carries the tag."),
    b!("game::effect", "give", "effect.give", &[Selector, Str, Int, Int], 2, BRet::Bool, "Give an effect to the matching entities: duration and amplifier, both in ticks."),
    b!("game::effect", "clear", "effect.clear", &[Selector], 1, BRet::Bool, "Clear (all or the named, if given) effects from the matching entities."),
    b!("game", "tp", "tp", &[Selector, Coord, Coord, Coord], 4, BRet::Bool, "Teleport the matching entities to the coordinates; true when it happened."),
    b!("game", "setblock", "setblock", &[Coord, Coord, Coord, Str], 4, BRet::Bool, "Place a block at the coordinates; true when the block was set."),
    b!("game", "summon", "summon", &[Str, Coord, Coord, Coord], 1, BRet::Entity, "Summon an entity at the coordinates and yield a handle to it."),
];

/// The built-in modules. A module path is one of these, or a user module from the project.
pub const MODULES: &[&str] = &["std", "game", "game::tag", "game::effect"];

/// Types that live in a built-in module.
pub const TYPES: &[(&str, &str, &str)] = &[("game", "Entity", "A reference to one entity; it may have died since. Create one with `game::summon`.")];

/// The type a built-in module exports under this name.
pub fn builtin_type(module: &str, name: &str) -> Option<Ty> {
    match (module, name) {
        ("game", "Entity") => Some(Ty::Entity),
        _ => None,
    }
}

/// A builtin by its id (what the HIR carries).
pub fn lookup(id: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.id == id)
}

/// Index into [`BUILTINS`] of `module::name`.
pub fn member(module: &str, name: &str) -> Option<usize> {
    BUILTINS.iter().position(|b| b.module == module && b.name == name)
}

pub fn is_module(path: &str) -> bool {
    MODULES.contains(&path)
}

/// Whether `import <path>;` names something built in: a module, or an item of one
/// (`import std::print;`).
pub fn is_import_path(path: &str) -> bool {
    is_module(path) || path.rsplit_once("::").is_some_and(|(parent, last)| is_module(parent) && module_items(parent).iter().any(|(n, _)| *n == last))
}

/// Everything a built-in module exports directly: its functions, its types and (for `game`) its
/// submodules.
pub fn module_items(module: &str) -> Vec<(&'static str, ItemKind)> {
    let mut out = Vec::new();
    for (i, b) in BUILTINS.iter().enumerate() {
        if b.module == module {
            out.push((b.name, ItemKind::Fn(i)));
        }
    }
    for (m, name, _) in TYPES {
        if *m == module {
            out.push((name, ItemKind::Type));
        }
    }
    for sub in MODULES {
        if let Some(rest) = sub.strip_prefix(module).and_then(|r| r.strip_prefix("::"))
            && !rest.contains("::")
        {
            out.push((rest, ItemKind::Module));
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// Index into [`BUILTINS`].
    Fn(usize),
    Type,
    Module,
}
