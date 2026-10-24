//! Typed command arguments: coordinates, ranges, selectors, score holders
//! (docs/mc/02-commands-core.md section 2).

use std::fmt::{self, Write};

use super::error::{Check, Result, bail};
use super::names::{HolderName, Hole, IdOrTag, Objective, ResourceLocation, Word};
use super::nbt::{Compound, NbtPath, write_quoted};

// ---------------------------------------------------------------- coordinates

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Coord {
    /// `5`, `-3.5`
    Abs(f64),
    /// `~` (offset 0) or `~2`
    Rel(f64),
    /// `^` (offset 0) or `^2`: local coordinates, rotation-relative
    Local(f64),
    /// A number supplied at run time (a macro argument), substituted as an absolute coordinate.
    Hole(Hole),
}

impl fmt::Display for Coord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (prefix, v) = match self {
            Coord::Abs(v) => ("", *v),
            Coord::Rel(v) => ("~", *v),
            Coord::Local(v) => ("^", *v),
            Coord::Hole(h) => return write!(f, "{h}"),
        };
        f.write_str(prefix)?;
        // `~`/`^` alone mean offset 0.
        if v == 0.0 && !prefix.is_empty() { Ok(()) } else { write!(f, "{v}") }
    }
}

impl Coord {
    /// The literal value, or None for a run-time hole (which can't be checked statically).
    fn value(&self) -> Option<f64> {
        match self {
            Coord::Abs(v) | Coord::Rel(v) | Coord::Local(v) => Some(*v),
            Coord::Hole(_) => None,
        }
    }
}

/// A 3D position, used for both `vec3` and `block_pos` arguments. Block positions
/// additionally need integer components: use [`Pos::check_block`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub x: Coord,
    pub y: Coord,
    pub z: Coord,
}

impl Pos {
    pub fn new(x: Coord, y: Coord, z: Coord) -> Self {
        Pos { x, y, z }
    }

    /// `~ ~ ~`
    pub fn here() -> Self {
        Pos::rel(0.0, 0.0, 0.0)
    }

    pub fn abs(x: f64, y: f64, z: f64) -> Self {
        Pos::new(Coord::Abs(x), Coord::Abs(y), Coord::Abs(z))
    }

    pub fn rel(x: f64, y: f64, z: f64) -> Self {
        Pos::new(Coord::Rel(x), Coord::Rel(y), Coord::Rel(z))
    }

    pub fn local(left: f64, up: f64, forward: f64) -> Self {
        Pos::new(Coord::Local(left), Coord::Local(up), Coord::Local(forward))
    }

    /// Absolute block position from integers.
    pub fn block(x: i32, y: i32, z: i32) -> Self {
        Pos::abs(x as f64, y as f64, z as f64)
    }

    fn comps(&self) -> [Coord; 3] {
        [self.x, self.y, self.z]
    }

    /// Additionally requires integer components, as `block_pos` arguments do.
    pub fn check_block(&self) -> Result<()> {
        self.check()?;
        for c in self.comps() {
            if c.value().is_some_and(|v| v.fract() != 0.0) {
                bail!("block position components must be integers, got {c}");
            }
        }
        Ok(())
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.x, self.y, self.z)
    }
}

impl Check for Pos {
    fn check(&self) -> Result<()> {
        let cs = self.comps();
        if cs.iter().any(|c| c.value().is_some_and(|v| !v.is_finite())) {
            bail!("coordinates must be finite");
        }
        // a hole is an absolute number, so it counts as a world coordinate
        let local = cs.iter().filter(|c| matches!(c, Coord::Local(_))).count();
        if local != 0 && local != 3 {
            bail!("cannot mix world (absolute/~) and local (^) coordinates in `{self}`");
        }
        Ok(())
    }
}

/// `<yaw> <pitch>`; local coordinates are not valid here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    pub yaw: Coord,
    pub pitch: Coord,
}

impl Rotation {
    pub fn abs(yaw: f64, pitch: f64) -> Self {
        Rotation { yaw: Coord::Abs(yaw), pitch: Coord::Abs(pitch) }
    }

    pub fn rel(yaw: f64, pitch: f64) -> Self {
        Rotation { yaw: Coord::Rel(yaw), pitch: Coord::Rel(pitch) }
    }
}

impl fmt::Display for Rotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.yaw, self.pitch)
    }
}

impl Check for Rotation {
    fn check(&self) -> Result<()> {
        for c in [self.yaw, self.pitch] {
            if matches!(c, Coord::Local(_)) {
                bail!("local coordinates are not valid in a rotation");
            }
            if c.value().is_some_and(|v| !v.is_finite()) {
                bail!("rotation must be finite");
            }
        }
        Ok(())
    }
}

/// A non-empty subset of the axes for `execute align`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Axes {
    pub x: bool,
    pub y: bool,
    pub z: bool,
}

impl Axes {
    pub const XYZ: Axes = Axes { x: true, y: true, z: true };
    pub const XZ: Axes = Axes { x: true, y: false, z: true };
}

impl fmt::Display for Axes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.x {
            f.write_char('x')?;
        }
        if self.y {
            f.write_char('y')?;
        }
        if self.z {
            f.write_char('z')?;
        }
        Ok(())
    }
}

impl Check for Axes {
    fn check(&self) -> Result<()> {
        if !(self.x || self.y || self.z) {
            bail!("align needs at least one axis");
        }
        Ok(())
    }
}

macro_rules! keyword_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
pub(crate) use keyword_enum;

keyword_enum!(Anchor { Eyes => "eyes", Feet => "feet" });
keyword_enum!(Heightmap {
    WorldSurface => "world_surface",
    MotionBlocking => "motion_blocking",
    MotionBlockingNoLeaves => "motion_blocking_no_leaves",
    OceanFloor => "ocean_floor",
});
keyword_enum!(
    /// `execute on <relation>`
    Relation {
        Attacker => "attacker",
        Controller => "controller",
        Leasher => "leasher",
        Origin => "origin",
        Owner => "owner",
        Passengers => "passengers",
        Target => "target",
        Vehicle => "vehicle",
    }
);
keyword_enum!(Gamemode {
    Survival => "survival",
    Creative => "creative",
    Adventure => "adventure",
    Spectator => "spectator",
});
keyword_enum!(NamedColor {
    Black => "black",
    DarkBlue => "dark_blue",
    DarkGreen => "dark_green",
    DarkAqua => "dark_aqua",
    DarkRed => "dark_red",
    DarkPurple => "dark_purple",
    Gold => "gold",
    Gray => "gray",
    DarkGray => "dark_gray",
    Blue => "blue",
    Green => "green",
    Aqua => "aqua",
    Red => "red",
    LightPurple => "light_purple",
    Yellow => "yellow",
    White => "white",
});

// --------------------------------------------------------------------- ranges

/// Inclusive integer range: `5`, `..5`, `5..`, `3..7`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntRange {
    pub min: Option<i32>,
    pub max: Option<i32>,
}

impl IntRange {
    pub fn exactly(n: i32) -> Self {
        IntRange { min: Some(n), max: Some(n) }
    }

    pub fn between(min: i32, max: i32) -> Self {
        IntRange { min: Some(min), max: Some(max) }
    }

    pub fn at_least(min: i32) -> Self {
        IntRange { min: Some(min), max: None }
    }

    pub fn at_most(max: i32) -> Self {
        IntRange { min: None, max: Some(max) }
    }
}

impl fmt::Display for IntRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.min, self.max) {
            (Some(a), Some(b)) if a == b => write!(f, "{a}"),
            (Some(a), Some(b)) => write!(f, "{a}..{b}"),
            (Some(a), None) => write!(f, "{a}.."),
            (None, Some(b)) => write!(f, "..{b}"),
            (None, None) => f.write_str(".."),
        }
    }
}

impl Check for IntRange {
    fn check(&self) -> Result<()> {
        match (self.min, self.max) {
            (None, None) => bail!("range needs at least one bound"),
            (Some(a), Some(b)) if a > b => bail!("range minimum {a} is bigger than maximum {b}"),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatRange {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl FloatRange {
    pub fn between(min: f64, max: f64) -> Self {
        FloatRange { min: Some(min), max: Some(max) }
    }

    pub fn at_most(max: f64) -> Self {
        FloatRange { min: None, max: Some(max) }
    }

    pub fn at_least(min: f64) -> Self {
        FloatRange { min: Some(min), max: None }
    }
}

impl fmt::Display for FloatRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.min, self.max) {
            (Some(a), Some(b)) if a == b => write!(f, "{a}"),
            (Some(a), Some(b)) => write!(f, "{a}..{b}"),
            (Some(a), None) => write!(f, "{a}.."),
            (None, Some(b)) => write!(f, "..{b}"),
            (None, None) => f.write_str(".."),
        }
    }
}

impl Check for FloatRange {
    fn check(&self) -> Result<()> {
        for v in [self.min, self.max].into_iter().flatten() {
            if !v.is_finite() {
                bail!("range bounds must be finite");
            }
        }
        match (self.min, self.max) {
            (None, None) => bail!("range needs at least one bound"),
            (Some(a), Some(b)) if a > b => bail!("range minimum {a} is bigger than maximum {b}"),
            _ => Ok(()),
        }
    }
}

// ------------------------------------------------------------------ selectors

keyword_enum!(SelectorVar { P => "@p", R => "@r", A => "@a", E => "@e", S => "@s", N => "@n" });
keyword_enum!(Sort { Nearest => "nearest", Furthest => "furthest", Random => "random", Arbitrary => "arbitrary" });

#[derive(Clone, Debug, PartialEq)]
pub enum SelectorArg {
    X(f64),
    Y(f64),
    Z(f64),
    Dx(f64),
    Dy(f64),
    Dz(f64),
    Distance(FloatRange),
    XRotation(FloatRange),
    YRotation(FloatRange),
    /// `scores={obj=range,...}`
    Scores(Vec<(Objective, IntRange)>),
    /// `scores={obj=$(hN)}`: the one entity whose id score is the number supplied at run time
    /// (an entity handle). Matches at most one entity.
    EntityId { objective: Objective, hole: Hole },
    Tag { name: Word, negated: bool },
    Team { name: Word, negated: bool },
    /// Plain-text entity name (not JSON).
    Name { name: String, negated: bool },
    Type { id: IdOrTag, negated: bool },
    Predicate { id: ResourceLocation, negated: bool },
    Nbt { nbt: Compound, negated: bool },
    Level(IntRange),
    Gamemode { mode: Gamemode, negated: bool },
    Limit(u32),
    Sort(Sort),
}

impl SelectorArg {
    pub fn tag(name: Word) -> Self {
        SelectorArg::Tag { name, negated: false }
    }

    pub fn not_tag(name: Word) -> Self {
        SelectorArg::Tag { name, negated: true }
    }

    pub fn entity_type(id: ResourceLocation) -> Self {
        SelectorArg::Type { id: IdOrTag::Id(id), negated: false }
    }

    pub fn predicate(id: ResourceLocation) -> Self {
        SelectorArg::Predicate { id, negated: false }
    }

    pub fn score(objective: Objective, range: IntRange) -> Self {
        SelectorArg::Scores(vec![(objective, range)])
    }

    /// The key as written, used for the "applied twice" check.
    fn key(&self) -> &'static str {
        match self {
            SelectorArg::X(_) => "x",
            SelectorArg::Y(_) => "y",
            SelectorArg::Z(_) => "z",
            SelectorArg::Dx(_) => "dx",
            SelectorArg::Dy(_) => "dy",
            SelectorArg::Dz(_) => "dz",
            SelectorArg::Distance(_) => "distance",
            SelectorArg::XRotation(_) => "x_rotation",
            SelectorArg::YRotation(_) => "y_rotation",
            SelectorArg::Scores(_) | SelectorArg::EntityId { .. } => "scores",
            SelectorArg::Tag { .. } => "tag",
            SelectorArg::Team { .. } => "team",
            SelectorArg::Name { .. } => "name",
            SelectorArg::Type { .. } => "type",
            SelectorArg::Predicate { .. } => "predicate",
            SelectorArg::Nbt { .. } => "nbt",
            SelectorArg::Level(_) => "level",
            SelectorArg::Gamemode { .. } => "gamemode",
            SelectorArg::Limit(_) => "limit",
            SelectorArg::Sort(_) => "sort",
        }
    }

    fn negated(&self) -> bool {
        matches!(
            self,
            SelectorArg::Tag { negated: true, .. }
                | SelectorArg::Team { negated: true, .. }
                | SelectorArg::Name { negated: true, .. }
                | SelectorArg::Type { negated: true, .. }
                | SelectorArg::Predicate { negated: true, .. }
                | SelectorArg::Nbt { negated: true, .. }
                | SelectorArg::Gamemode { negated: true, .. }
        )
    }

    /// Whether the same key may appear more than once (docs/mc/02-commands-core.md 2.3):
    /// tag/predicate/nbt always; name/team/type/gamemode only in their negated form.
    fn repeatable(&self) -> bool {
        match self {
            SelectorArg::Tag { .. } | SelectorArg::Predicate { .. } | SelectorArg::Nbt { .. } => true,
            SelectorArg::Team { .. }
            | SelectorArg::Name { .. }
            | SelectorArg::Type { .. }
            | SelectorArg::Gamemode { .. } => self.negated(),
            _ => false,
        }
    }
}

fn bang(negated: bool) -> &'static str {
    if negated { "!" } else { "" }
}

impl fmt::Display for SelectorArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = self.key();
        match self {
            SelectorArg::X(v) | SelectorArg::Y(v) | SelectorArg::Z(v) | SelectorArg::Dx(v) | SelectorArg::Dy(v) | SelectorArg::Dz(v) => {
                write!(f, "{key}={v}")
            }
            SelectorArg::Distance(r) | SelectorArg::XRotation(r) | SelectorArg::YRotation(r) => write!(f, "{key}={r}"),
            SelectorArg::Scores(entries) => {
                f.write_str("scores={")?;
                for (i, (obj, range)) in entries.iter().enumerate() {
                    if i > 0 {
                        f.write_char(',')?;
                    }
                    write!(f, "{obj}={range}")?;
                }
                f.write_char('}')
            }
            SelectorArg::EntityId { objective, hole } => write!(f, "scores={{{objective}={hole}}}"),
            SelectorArg::Tag { name, negated } => write!(f, "tag={}{name}", bang(*negated)),
            SelectorArg::Team { name, negated } => write!(f, "team={}{name}", bang(*negated)),
            SelectorArg::Name { name, negated } => {
                write!(f, "name={}", bang(*negated))?;
                write_quoted(f, name)
            }
            SelectorArg::Type { id, negated } => write!(f, "type={}{id}", bang(*negated)),
            SelectorArg::Predicate { id, negated } => write!(f, "predicate={}{id}", bang(*negated)),
            SelectorArg::Nbt { nbt, negated } => write!(f, "nbt={}{nbt}", bang(*negated)),
            SelectorArg::Level(r) => write!(f, "level={r}"),
            SelectorArg::Gamemode { mode, negated } => write!(f, "gamemode={}{mode}", bang(*negated)),
            SelectorArg::Limit(n) => write!(f, "limit={n}"),
            SelectorArg::Sort(s) => write!(f, "sort={s}"),
        }
    }
}

impl Check for SelectorArg {
    fn check(&self) -> Result<()> {
        match self {
            SelectorArg::X(v) | SelectorArg::Y(v) | SelectorArg::Z(v) | SelectorArg::Dx(v) | SelectorArg::Dy(v) | SelectorArg::Dz(v)
                if !v.is_finite() =>
            {
                bail!("selector coordinate must be finite")
            }
            SelectorArg::Distance(r) => {
                r.check()?;
                if r.min.is_some_and(|v| v < 0.0) || r.max.is_some_and(|v| v < 0.0) {
                    bail!("selector distance must not be negative");
                }
                Ok(())
            }
            SelectorArg::XRotation(r) | SelectorArg::YRotation(r) => r.check(),
            SelectorArg::Scores(entries) => {
                if entries.is_empty() {
                    bail!("scores={{}} needs at least one objective");
                }
                entries.iter().try_for_each(|(_, r)| r.check())
            }
            SelectorArg::Nbt { nbt, .. } => nbt.check(),
            SelectorArg::Level(r) => r.check(),
            SelectorArg::Limit(0) => bail!("selector limit must be at least 1"),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    pub var: SelectorVar,
    pub args: Vec<SelectorArg>,
}

impl Selector {
    pub fn new(var: SelectorVar) -> Self {
        Selector { var, args: Vec::new() }
    }

    pub fn s() -> Self {
        Self::new(SelectorVar::S)
    }
    pub fn a() -> Self {
        Self::new(SelectorVar::A)
    }
    pub fn e() -> Self {
        Self::new(SelectorVar::E)
    }
    pub fn p() -> Self {
        Self::new(SelectorVar::P)
    }
    pub fn r() -> Self {
        Self::new(SelectorVar::R)
    }
    pub fn n() -> Self {
        Self::new(SelectorVar::N)
    }

    pub fn arg(mut self, arg: SelectorArg) -> Self {
        self.args.push(arg);
        self
    }

    pub fn tag(self, name: Word) -> Self {
        self.arg(SelectorArg::tag(name))
    }

    pub fn not_tag(self, name: Word) -> Self {
        self.arg(SelectorArg::not_tag(name))
    }

    pub fn entity_type(self, id: ResourceLocation) -> Self {
        self.arg(SelectorArg::entity_type(id))
    }

    pub fn limit(self, n: u32) -> Self {
        self.arg(SelectorArg::Limit(n))
    }

    /// The entity whose `objective` score equals the run-time value of `hole`.
    ///
    /// `limit=1` is part of it: the id is unique so it changes nothing, it lets the game stop
    /// searching at the first hit, and Minecraft refuses a selector that "allows more than one"
    /// entity in single-target arguments such as `scoreboard players get`.
    pub fn entity_id(objective: Objective, hole: Hole) -> Self {
        Selector::e().arg(SelectorArg::EntityId { objective, hole }).limit(1)
    }

    pub fn score(self, objective: Objective, range: IntRange) -> Self {
        self.arg(SelectorArg::score(objective, range))
    }

    /// True when the selector can only ever match one entity, which several arguments
    /// require ("Only one entity is allowed" is a parse error otherwise).
    pub fn is_single(&self) -> bool {
        match self.var {
            SelectorVar::S | SelectorVar::P | SelectorVar::R | SelectorVar::N => {
                // `@n[limit=2]` is allowed and then matches several.
                !self.args.iter().any(|a| matches!(a, SelectorArg::Limit(n) if *n > 1))
            }
            SelectorVar::A | SelectorVar::E => self.args.iter().any(|a| matches!(a, SelectorArg::Limit(1) | SelectorArg::EntityId { .. })),
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.var)?;
        if !self.args.is_empty() {
            f.write_char('[')?;
            for (i, arg) in self.args.iter().enumerate() {
                if i > 0 {
                    f.write_char(',')?;
                }
                write!(f, "{arg}")?;
            }
            f.write_char(']')?;
        }
        Ok(())
    }
}

impl Check for Selector {
    fn check(&self) -> Result<()> {
        self.args.check()?;
        for (i, a) in self.args.iter().enumerate() {
            if !a.repeatable() && self.args[..i].iter().any(|b| b.key() == a.key()) {
                bail!("selector option '{}' can only be used once in `{self}`", a.key());
            }
        }
        // `type=` can't be combined with itself positively, but a positive and a negated one
        // is fine; same for gamemode/team/name. Mixed handling is left to the game.
        Ok(())
    }
}

// --------------------------------------------------------------- score holders

#[derive(Clone, Debug, PartialEq)]
pub enum ScoreHolder {
    Selector(Selector),
    /// A fake player such as `#counter` or a literal player name.
    Name(HolderName),
    /// `*`: every tracked holder. Only valid where several targets are accepted.
    All,
}

impl ScoreHolder {
    pub fn fake(name: HolderName) -> Self {
        ScoreHolder::Name(name)
    }

    pub fn is_single(&self) -> bool {
        match self {
            ScoreHolder::Selector(s) => s.is_single(),
            ScoreHolder::Name(_) => true,
            ScoreHolder::All => false,
        }
    }

    /// Checks usage where exactly one holder is required (`get`, `if score`, ...).
    pub(crate) fn check_single(&self) -> Result<()> {
        self.check()?;
        if !self.is_single() {
            bail!("`{self}` can match several holders but a single one is required here");
        }
        Ok(())
    }
}

impl From<Selector> for ScoreHolder {
    fn from(s: Selector) -> Self {
        ScoreHolder::Selector(s)
    }
}

impl fmt::Display for ScoreHolder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScoreHolder::Selector(s) => write!(f, "{s}"),
            ScoreHolder::Name(n) => write!(f, "{n}"),
            ScoreHolder::All => f.write_char('*'),
        }
    }
}

impl Check for ScoreHolder {
    fn check(&self) -> Result<()> {
        match self {
            ScoreHolder::Selector(s) => s.check(),
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------- data targets

/// Where `data` commands read and write NBT.
#[derive(Clone, Debug, PartialEq)]
pub enum DataTarget {
    Block(Pos),
    Entity(Selector),
    Storage(ResourceLocation),
}

impl fmt::Display for DataTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataTarget::Block(p) => write!(f, "block {p}"),
            DataTarget::Entity(s) => write!(f, "entity {s}"),
            DataTarget::Storage(id) => write!(f, "storage {id}"),
        }
    }
}

impl Check for DataTarget {
    fn check(&self) -> Result<()> {
        match self {
            DataTarget::Block(p) => p.check_block(),
            DataTarget::Entity(s) => {
                s.check()?;
                if !s.is_single() {
                    bail!("data target `{s}` can match several entities but exactly one is required");
                }
                Ok(())
            }
            DataTarget::Storage(_) => Ok(()),
        }
    }
}

/// `[<path>]` suffix helper: renders ` <path>` when present.
pub(crate) struct OptPath<'a>(pub &'a Option<NbtPath>);

impl fmt::Display for OptPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(p) => write!(f, " {p}"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datapack::names::{holder, rl, word};

    #[test]
    fn coordinates() {
        assert_eq!(Pos::here().to_string(), "~ ~ ~");
        assert_eq!(Pos::rel(0.0, 1.0, -2.5).to_string(), "~ ~1 ~-2.5");
        assert_eq!(Pos::abs(1.0, 64.0, -3.5).to_string(), "1 64 -3.5");
        assert_eq!(Pos::local(0.0, 0.0, 2.0).to_string(), "^ ^ ^2");
        assert!(Pos::new(Coord::Rel(0.0), Coord::Local(1.0), Coord::Rel(0.0)).check().is_err());
        assert!(Pos::abs(0.5, 0.0, 0.0).check_block().is_err());
        assert!(Pos::block(1, 2, 3).check_block().is_ok());
    }

    #[test]
    fn coordinate_holes() {
        let p = Pos::new(Coord::Hole(Hole(0)), Coord::Rel(1.0), Coord::Hole(Hole(1)));
        assert_eq!(p.to_string(), "$(h0) ~1 $(h1)");
        assert!(p.check().is_ok() && p.check_block().is_ok());
        // a hole is a world coordinate, so it can't be combined with `^`
        assert!(Pos::new(Coord::Hole(Hole(0)), Coord::Local(1.0), Coord::Local(0.0)).check().is_err());
    }

    #[test]
    fn ranges() {
        assert_eq!(IntRange::exactly(5).to_string(), "5");
        assert_eq!(IntRange::at_most(5).to_string(), "..5");
        assert_eq!(IntRange::at_least(5).to_string(), "5..");
        assert_eq!(IntRange::between(3, 7).to_string(), "3..7");
        assert!(IntRange::between(7, 3).check().is_err());
        assert_eq!(FloatRange::at_most(2.5).to_string(), "..2.5");
    }

    #[test]
    fn selectors() {
        assert_eq!(Selector::s().to_string(), "@s");
        let s = Selector::e()
            .tag(word("a"))
            .not_tag(word("b"))
            .entity_type(rl("pig"))
            .score(word("pts"), IntRange::at_least(3))
            .arg(SelectorArg::Name { name: "Bob \"B\"".into(), negated: false })
            .limit(1);
        assert_eq!(
            s.to_string(),
            r#"@e[tag=a,tag=!b,type=minecraft:pig,scores={pts=3..},name="Bob \"B\"",limit=1]"#
        );
        assert!(s.is_single());
        assert!(!Selector::a().is_single());
        assert!(Selector::p().is_single());
        assert!(!Selector::n().limit(2).is_single());
        assert!(Selector::a().limit(0).check().is_err());
        assert!(Selector::a().limit(1).limit(2).check().is_err());
        assert!(Selector::e().tag(word("a")).tag(word("b")).check().is_ok());
    }

    #[test]
    fn entity_id_selectors() {
        let s = Selector::entity_id(word("ns_id"), Hole(0)).tag(word("mine"));
        assert_eq!(s.to_string(), "@e[scores={ns_id=$(h0)},limit=1,tag=mine]");
        assert!(s.is_single(), "an id matches at most one entity");
        assert!(s.check().is_ok());
        // it uses the `scores` option, which may appear only once
        assert!(Selector::entity_id(word("ns_id"), Hole(0)).score(word("pts"), IntRange::at_least(1)).check().is_err());
    }

    #[test]
    fn holders() {
        assert_eq!(ScoreHolder::fake(holder("#tmp")).to_string(), "#tmp");
        assert!(ScoreHolder::All.check_single().is_err());
        assert!(ScoreHolder::from(Selector::a()).check_single().is_err());
        assert!(ScoreHolder::from(Selector::s()).check_single().is_ok());
    }
}
