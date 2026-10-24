use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructId(pub usize);

/// Static types of slab values.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// 32-bit signed integer (scoreboard).
    Int,
    Bool,
    Str,
    /// The type of statements and of functions that return nothing.
    Unit,
    List(Box<Ty>),
    Struct(StructId),
    /// A reference to one entity (a handle). Stored as an int id under the hood, so it can live in
    /// variables, lists and structs and be passed and returned; the entity may have died since.
    Entity,
    /// An entity selector such as `@a[tag=x]`. Only usable where the language expects one
    /// (builtin arguments, context blocks, objective indexing); it cannot be stored.
    Selector,
    /// A relative coordinate such as `~` or `^2`. Not storable either.
    Coord,
    /// A type that already produced an error; compatible with everything so one mistake is
    /// reported once.
    Error,
}

impl Ty {
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Whether a value of this type can live in a variable, field or list.
    pub fn is_storable(&self) -> bool {
        match self {
            Ty::Int | Ty::Bool | Ty::Str | Ty::Struct(_) | Ty::Entity => true,
            Ty::List(t) => t.is_storable(),
            Ty::Unit | Ty::Selector | Ty::Coord => false,
            Ty::Error => true,
        }
    }

    /// Equality that treats `Error` as matching anything.
    pub fn compatible(&self, other: &Ty) -> bool {
        match (self, other) {
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::List(a), Ty::List(b)) => a.compatible(b),
            (a, b) => a == b,
        }
    }

    pub fn display<'a>(&'a self, structs: &'a [String]) -> TyDisplay<'a> {
        TyDisplay { ty: self, structs }
    }
}

/// `Ty` with struct names resolved, for diagnostics.
pub struct TyDisplay<'a> {
    ty: &'a Ty,
    structs: &'a [String],
}

impl fmt::Display for TyDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ty {
            Ty::Int => f.write_str("int"),
            Ty::Bool => f.write_str("bool"),
            Ty::Str => f.write_str("str"),
            Ty::Unit => f.write_str("()"),
            Ty::List(t) => write!(f, "list<{}>", t.display(self.structs)),
            Ty::Struct(id) => f.write_str(self.structs.get(id.0).map_or("?", String::as_str)),
            Ty::Entity => f.write_str("entity"),
            Ty::Selector => f.write_str("selector"),
            Ty::Coord => f.write_str("coordinate"),
            Ty::Error => f.write_str("{error}"),
        }
    }
}
