//! The typed command AST. This is the only place command strings are produced: build
//! [`Command`] values, then `Display` them (or let [`Function`](super::Function) do it).
//!
//! Syntax follows docs/mc/02-commands-core.md (generated from the 26.3 server's command tree).

use std::fmt::{self, Write};

use super::args::*;
use super::error::{Check, Result, bail};
use super::names::{Hole, IdOrTag, Objective, ResourceLocation, Word};
use super::nbt::{Compound, NbtPath};
use super::text::Text;

// ---------------------------------------------------------------- shared bits

/// A function id or a `#tag` of functions.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FunctionRef {
    Id(ResourceLocation),
    Tag(ResourceLocation),
}

impl fmt::Display for FunctionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionRef::Id(id) => write!(f, "{id}"),
            FunctionRef::Tag(id) => write!(f, "#{id}"),
        }
    }
}

impl From<ResourceLocation> for FunctionRef {
    fn from(id: ResourceLocation) -> Self {
        FunctionRef::Id(id)
    }
}

keyword_enum!(NumType {
    Byte => "byte",
    Short => "short",
    Int => "int",
    Long => "long",
    Float => "float",
    Double => "double",
});
keyword_enum!(Cmp { Lt => "<", Le => "<=", Eq => "=", Ge => ">=", Gt => ">" });
keyword_enum!(ScoreOp {
    Set => "=",
    Add => "+=",
    Sub => "-=",
    Mul => "*=",
    Div => "/=",
    Mod => "%=",
    Min => "<",
    Max => ">",
    Swap => "><",
});

/// `stone[facing=north]{...}` for placing (`setblock`).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockState {
    pub id: Arg<ResourceLocation>,
    pub props: Vec<(Word, Word)>,
    pub nbt: Option<Compound>,
}

impl BlockState {
    pub fn new(id: impl Into<Arg<ResourceLocation>>) -> Self {
        BlockState { id: id.into(), props: Vec::new(), nbt: None }
    }

    pub fn prop(mut self, key: Word, value: Word) -> Self {
        self.props.push((key, value));
        self
    }

    pub fn nbt(mut self, nbt: Compound) -> Self {
        self.nbt = Some(nbt);
        self
    }
}

/// `#tag[state=value]{nbt}` for testing (`execute if block`, ...).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockPredicate {
    pub id: IdOrTag,
    pub props: Vec<(Word, Word)>,
    pub nbt: Option<Compound>,
}

impl BlockPredicate {
    pub fn new(id: IdOrTag) -> Self {
        BlockPredicate { id, props: Vec::new(), nbt: None }
    }

    pub fn block(id: ResourceLocation) -> Self {
        Self::new(IdOrTag::Id(id))
    }

    pub fn prop(mut self, key: Word, value: Word) -> Self {
        self.props.push((key, value));
        self
    }
}

fn write_block(
    f: &mut fmt::Formatter<'_>,
    id: &dyn fmt::Display,
    props: &[(Word, Word)],
    nbt: &Option<Compound>,
) -> fmt::Result {
    write!(f, "{id}")?;
    if !props.is_empty() {
        f.write_char('[')?;
        for (i, (k, v)) in props.iter().enumerate() {
            if i > 0 {
                f.write_char(',')?;
            }
            write!(f, "{k}={v}")?;
        }
        f.write_char(']')?;
    }
    if let Some(n) = nbt {
        write!(f, "{n}")?;
    }
    Ok(())
}

impl fmt::Display for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_block(f, &self.id, &self.props, &self.nbt)
    }
}

impl fmt::Display for BlockPredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_block(f, &self.id, &self.props, &self.nbt)
    }
}

impl Check for BlockState {
    fn check(&self) -> Result<()> {
        self.nbt.check()
    }
}

impl Check for BlockPredicate {
    fn check(&self) -> Result<()> {
        self.nbt.check()
    }
}

/// A `context_int_provider` / `context_float_provider` argument (26.3): a registered
/// provider id, or an inline compound. A bare number is *not* accepted by the command.
#[derive(Clone, Debug, PartialEq)]
pub enum Provider {
    Id(ResourceLocation),
    Inline(Compound),
}

impl Provider {
    /// `{type:"minecraft:constant",value:n}`
    pub fn constant(value: impl Into<super::nbt::Nbt>) -> Self {
        Provider::Inline(Compound::new().with("type", "minecraft:constant").with("value", value))
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provider::Id(id) => write!(f, "{id}"),
            Provider::Inline(c) => write!(f, "{c}"),
        }
    }
}

impl Check for Provider {
    fn check(&self) -> Result<()> {
        match self {
            Provider::Inline(c) => {
                if c.get("type").is_none() {
                    bail!("an inline number provider needs a `type`");
                }
                c.check()
            }
            Provider::Id(_) => Ok(()),
        }
    }
}

// -------------------------------------------------------------------- execute

keyword_enum!(StoreKind { Result => "result", Success => "success" });
keyword_enum!(BossbarField { Value => "value", Max => "max" });
keyword_enum!(BlocksMode { All => "all", Masked => "masked" });

#[derive(Clone, Debug, PartialEq)]
pub enum StoreTarget {
    Score { targets: ScoreHolder, objective: Objective },
    /// The value is multiplied by `scale`, then cast to `ty`.
    Data { target: DataTarget, path: NbtPath, ty: NumType, scale: f64 },
    Bossbar { id: ResourceLocation, field: BossbarField },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScoreTest {
    Compare(Cmp, ScoreHolder, Objective),
    Matches(IntRange),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    Biome(Pos, IdOrTag),
    Block(Pos, BlockPredicate),
    Blocks { start: Pos, end: Pos, dest: Pos, mode: BlocksMode },
    Data(DataTarget, NbtPath),
    Dimension(ResourceLocation),
    Entity(Selector),
    /// Runs the function as a test; it cannot be the last sub-command.
    Function(FunctionRef),
    Loaded(Pos),
    Predicate(ResourceLocation),
    Score { target: ScoreHolder, objective: Objective, test: ScoreTest },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Clause {
    As(Selector),
    At(Selector),
    Positioned(Pos),
    PositionedAs(Selector),
    PositionedOver(Heightmap),
    Rotated(Rotation),
    RotatedAs(Selector),
    Facing(Pos),
    FacingEntity(Selector, Anchor),
    Align(Axes),
    Anchored(Anchor),
    In(ResourceLocation),
    On(Relation),
    Summon(Arg<ResourceLocation>),
    Store(StoreKind, StoreTarget),
    If(Condition),
    Unless(Condition),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Execute {
    pub clauses: Vec<Clause>,
    pub run: Option<Box<Command>>,
}

impl Execute {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clause(mut self, c: Clause) -> Self {
        self.clauses.push(c);
        self
    }

    pub fn as_(self, s: Selector) -> Self {
        self.clause(Clause::As(s))
    }

    pub fn at(self, s: Selector) -> Self {
        self.clause(Clause::At(s))
    }

    pub fn positioned(self, p: Pos) -> Self {
        self.clause(Clause::Positioned(p))
    }

    pub fn if_(self, c: Condition) -> Self {
        self.clause(Clause::If(c))
    }

    pub fn unless(self, c: Condition) -> Self {
        self.clause(Clause::Unless(c))
    }

    pub fn store(self, kind: StoreKind, target: StoreTarget) -> Self {
        self.clause(Clause::Store(kind, target))
    }

    /// `if score <holder> <obj> matches <range>`
    pub fn if_score_matches(self, target: ScoreHolder, objective: Objective, range: IntRange) -> Self {
        self.if_(Condition::Score { target, objective, test: ScoreTest::Matches(range) })
    }

    /// Finishes the chain with `run <cmd>`. A nested `execute` is flattened (`run execute x`
    /// is identical to continuing the chain) and an empty chain is just the command.
    pub fn run(mut self, cmd: Command) -> Command {
        match cmd {
            Command::Execute(inner) => {
                self.clauses.extend(inner.clauses);
                self.run = inner.run;
            }
            other => self.run = Some(Box::new(other)),
        }
        if self.clauses.is_empty() {
            return *self.run.expect("run was just set or taken from the inner execute");
        }
        Command::Execute(self)
    }

    /// Finishes a chain that ends on a condition (e.g. `execute if entity @e[tag=x]`), whose
    /// success/result is the test outcome. Fails if the chain doesn't end on a condition.
    pub fn into_test(self) -> Result<Command> {
        let cmd = Command::Execute(self);
        cmd.check()?;
        Ok(cmd)
    }
}

impl Check for Condition {
    fn check(&self) -> Result<()> {
        match self {
            Condition::Biome(p, _) | Condition::Block(p, _) | Condition::Loaded(p) => p.check_block()?,
            Condition::Blocks { start, end, dest, .. } => {
                start.check_block()?;
                end.check_block()?;
                dest.check_block()?;
            }
            Condition::Data(t, path) => {
                t.check()?;
                path.check()?;
                if path.is_empty() {
                    bail!("`if data` needs a non-empty path");
                }
            }
            Condition::Entity(s) => s.check()?,
            Condition::Score { target, test, .. } => {
                target.check_single()?;
                match test {
                    ScoreTest::Compare(_, other, _) => other.check_single()?,
                    ScoreTest::Matches(r) => r.check()?,
                }
            }
            Condition::Dimension(_) | Condition::Function(_) | Condition::Predicate(_) => {}
        }
        if let Condition::Block(_, b) = self {
            b.check()?;
        }
        Ok(())
    }
}

impl Check for Clause {
    fn check(&self) -> Result<()> {
        match self {
            Clause::As(s) | Clause::At(s) | Clause::PositionedAs(s) | Clause::RotatedAs(s) | Clause::FacingEntity(s, _) => s.check(),
            Clause::Positioned(p) | Clause::Facing(p) => p.check(),
            Clause::Rotated(r) => r.check(),
            Clause::Align(a) => a.check(),
            Clause::Store(_, StoreTarget::Score { targets, .. }) => targets.check(),
            Clause::Store(_, StoreTarget::Data { target, path, scale, .. }) => {
                target.check()?;
                path.check()?;
                if path.is_empty() {
                    bail!("`store ... {target}` needs a non-empty path");
                }
                if !scale.is_finite() {
                    bail!("store scale must be finite");
                }
                Ok(())
            }
            Clause::If(c) | Clause::Unless(c) => c.check(),
            Clause::PositionedOver(_)
            | Clause::Anchored(_)
            | Clause::In(_)
            | Clause::On(_)
            | Clause::Summon(_)
            | Clause::Store(_, StoreTarget::Bossbar { .. }) => Ok(()),
        }
    }
}

impl Check for Execute {
    fn check(&self) -> Result<()> {
        self.clauses.check()?;
        match (&self.run, self.clauses.last()) {
            (Some(run), _) => run.check(),
            (None, Some(Clause::If(c) | Clause::Unless(c))) => {
                if matches!(c, Condition::Function(_)) {
                    bail!("`execute if function` cannot end the chain; it needs a following sub-command or `run`");
                }
                Ok(())
            }
            (None, _) => bail!("an execute chain must end with `run <command>` or a condition"),
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Condition::Biome(p, b) => write!(f, "biome {p} {b}"),
            Condition::Block(p, b) => write!(f, "block {p} {b}"),
            Condition::Blocks { start, end, dest, mode } => write!(f, "blocks {start} {end} {dest} {mode}"),
            Condition::Data(t, path) => write!(f, "data {t} {path}"),
            Condition::Dimension(d) => write!(f, "dimension {d}"),
            Condition::Entity(s) => write!(f, "entity {s}"),
            Condition::Function(r) => write!(f, "function {r}"),
            Condition::Loaded(p) => write!(f, "loaded {p}"),
            Condition::Predicate(id) => write!(f, "predicate {id}"),
            Condition::Score { target, objective, test } => {
                write!(f, "score {target} {objective} ")?;
                match test {
                    ScoreTest::Compare(op, other, other_obj) => write!(f, "{op} {other} {other_obj}"),
                    ScoreTest::Matches(r) => write!(f, "matches {r}"),
                }
            }
        }
    }
}

impl fmt::Display for Clause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Clause::As(s) => write!(f, "as {s}"),
            Clause::At(s) => write!(f, "at {s}"),
            Clause::Positioned(p) => write!(f, "positioned {p}"),
            Clause::PositionedAs(s) => write!(f, "positioned as {s}"),
            Clause::PositionedOver(h) => write!(f, "positioned over {h}"),
            Clause::Rotated(r) => write!(f, "rotated {r}"),
            Clause::RotatedAs(s) => write!(f, "rotated as {s}"),
            Clause::Facing(p) => write!(f, "facing {p}"),
            Clause::FacingEntity(s, a) => write!(f, "facing entity {s} {a}"),
            Clause::Align(a) => write!(f, "align {a}"),
            Clause::Anchored(a) => write!(f, "anchored {a}"),
            Clause::In(d) => write!(f, "in {d}"),
            Clause::On(r) => write!(f, "on {r}"),
            Clause::Summon(e) => write!(f, "summon {e}"),
            Clause::Store(kind, target) => {
                write!(f, "store {kind} ")?;
                match target {
                    StoreTarget::Score { targets, objective } => write!(f, "score {targets} {objective}"),
                    StoreTarget::Data { target, path, ty, scale } => write!(f, "{target} {path} {ty} {scale}"),
                    StoreTarget::Bossbar { id, field } => write!(f, "bossbar {id} {field}"),
                }
            }
            Clause::If(c) => write!(f, "if {c}"),
            Clause::Unless(c) => write!(f, "unless {c}"),
        }
    }
}

impl fmt::Display for Execute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("execute")?;
        for c in &self.clauses {
            write!(f, " {c}")?;
        }
        if let Some(run) = &self.run {
            write!(f, " run {run}")?;
        }
        Ok(())
    }
}

// ----------------------------------------------------------------- scoreboard

keyword_enum!(RenderType { Hearts => "hearts", Integer => "integer" });

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Criteria {
    Dummy,
    Trigger,
    /// Any other criterion such as `deathCount` or `minecraft.custom:minecraft.jump`.
    Custom(String),
}

impl fmt::Display for Criteria {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Criteria::Dummy => f.write_str("dummy"),
            Criteria::Trigger => f.write_str("trigger"),
            Criteria::Custom(s) => f.write_str(s),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DisplaySlot {
    List,
    Sidebar,
    BelowName,
    SidebarTeam(NamedColor),
}

impl fmt::Display for DisplaySlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplaySlot::List => f.write_str("list"),
            DisplaySlot::Sidebar => f.write_str("sidebar"),
            DisplaySlot::BelowName => f.write_str("below_name"),
            DisplaySlot::SidebarTeam(c) => write!(f, "sidebar.team.{c}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ObjectiveModify {
    DisplayName(Text),
    RenderType(RenderType),
    DisplayAutoUpdate(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Scoreboard {
    ObjectivesAdd { name: Objective, criteria: Criteria, display_name: Option<Text> },
    ObjectivesRemove(Objective),
    ObjectivesList,
    ObjectivesModify { objective: Objective, change: ObjectiveModify },
    /// `objective: None` clears the slot.
    ObjectivesSetDisplay { slot: DisplaySlot, objective: Option<Objective> },
    PlayersSet { targets: ScoreHolder, objective: Objective, value: i32 },
    /// `value` must be >= 0 (use `PlayersRemove` to subtract).
    PlayersAdd { targets: ScoreHolder, objective: Objective, value: i32 },
    /// `value` must be >= 0.
    PlayersRemove { targets: ScoreHolder, objective: Objective, value: i32 },
    PlayersReset { targets: ScoreHolder, objective: Option<Objective> },
    PlayersGet { target: ScoreHolder, objective: Objective },
    PlayersEnable { targets: ScoreHolder, objective: Objective },
    PlayersList(Option<ScoreHolder>),
    PlayersOperation { targets: ScoreHolder, target_objective: Objective, op: ScoreOp, source: ScoreHolder, source_objective: Objective },
}

impl fmt::Display for Scoreboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("scoreboard ")?;
        match self {
            Scoreboard::ObjectivesAdd { name, criteria, display_name } => {
                write!(f, "objectives add {name} {criteria}")?;
                if let Some(d) = display_name {
                    write!(f, " {d}")?;
                }
                Ok(())
            }
            Scoreboard::ObjectivesRemove(o) => write!(f, "objectives remove {o}"),
            Scoreboard::ObjectivesList => f.write_str("objectives list"),
            Scoreboard::ObjectivesModify { objective, change } => {
                write!(f, "objectives modify {objective} ")?;
                match change {
                    ObjectiveModify::DisplayName(t) => write!(f, "displayname {t}"),
                    ObjectiveModify::RenderType(r) => write!(f, "rendertype {r}"),
                    ObjectiveModify::DisplayAutoUpdate(b) => write!(f, "displayautoupdate {b}"),
                }
            }
            Scoreboard::ObjectivesSetDisplay { slot, objective } => {
                write!(f, "objectives setdisplay {slot}")?;
                if let Some(o) = objective {
                    write!(f, " {o}")?;
                }
                Ok(())
            }
            Scoreboard::PlayersSet { targets, objective, value } => write!(f, "players set {targets} {objective} {value}"),
            Scoreboard::PlayersAdd { targets, objective, value } => write!(f, "players add {targets} {objective} {value}"),
            Scoreboard::PlayersRemove { targets, objective, value } => write!(f, "players remove {targets} {objective} {value}"),
            Scoreboard::PlayersReset { targets, objective } => {
                write!(f, "players reset {targets}")?;
                if let Some(o) = objective {
                    write!(f, " {o}")?;
                }
                Ok(())
            }
            Scoreboard::PlayersGet { target, objective } => write!(f, "players get {target} {objective}"),
            Scoreboard::PlayersEnable { targets, objective } => write!(f, "players enable {targets} {objective}"),
            Scoreboard::PlayersList(t) => {
                f.write_str("players list")?;
                if let Some(t) = t {
                    write!(f, " {t}")?;
                }
                Ok(())
            }
            Scoreboard::PlayersOperation { targets, target_objective, op, source, source_objective } => {
                write!(f, "players operation {targets} {target_objective} {op} {source} {source_objective}")
            }
        }
    }
}

impl Check for Scoreboard {
    fn check(&self) -> Result<()> {
        match self {
            Scoreboard::ObjectivesAdd { criteria, display_name, .. } => {
                if let Criteria::Custom(c) = criteria
                    && (c.is_empty() || !c.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-'))) {
                        bail!("invalid objective criterion {c:?}");
                    }
                display_name.check()
            }
            Scoreboard::ObjectivesModify { change: ObjectiveModify::DisplayName(t), .. } => t.check(),
            Scoreboard::PlayersSet { targets, .. } | Scoreboard::PlayersEnable { targets, .. } | Scoreboard::PlayersReset { targets, .. } => {
                targets.check()
            }
            Scoreboard::PlayersAdd { targets, value, .. } | Scoreboard::PlayersRemove { targets, value, .. } => {
                if *value < 0 {
                    bail!("`scoreboard players add/remove` takes a non-negative amount, got {value}");
                }
                targets.check()
            }
            Scoreboard::PlayersGet { target, .. } => target.check_single(),
            Scoreboard::PlayersList(Some(t)) => t.check_single(),
            Scoreboard::PlayersOperation { targets, source, .. } => {
                targets.check()?;
                source.check()
            }
            _ => Ok(()),
        }
    }
}

// ------------------------------------------------------------------------ data

#[derive(Clone, Debug, PartialEq)]
pub enum ComputeContext {
    Default,
    Block(Pos),
    Entity(Selector),
}

impl fmt::Display for ComputeContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComputeContext::Default => f.write_str("default"),
            ComputeContext::Block(p) => write!(f, "block {p}"),
            ComputeContext::Entity(s) => write!(f, "entity {s}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModifyOp {
    Append,
    Prepend,
    Set,
    Merge,
    Insert(i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplatePart {
    Lit(String),
    Hole(Hole),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModifySource {
    From { source: DataTarget, path: Option<NbtPath> },
    /// Substring of a string (or number) tag; `end` requires `start`, `start` requires `path`.
    String { source: DataTarget, path: Option<NbtPath>, start: Option<i32>, end: Option<i32> },
    Value(super::nbt::Nbt),
    /// `value "text $(h0) text"`: a string built at run time from literal text and holes (string
    /// concatenation). Hole values are substituted as-is inside the quotes, so a value that
    /// contains `"` or `\` breaks the command.
    Template(Vec<TemplatePart>),
    /// 26.3: evaluate a number provider into an int tag.
    ComputeInt { ctx: ComputeContext, provider: Provider },
    /// 26.3: evaluate a number provider into a float tag.
    ComputeFloat { ctx: ComputeContext, provider: Provider },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Get { target: DataTarget, path: Option<NbtPath>, scale: Option<f64> },
    Merge { target: DataTarget, nbt: Compound },
    Remove { target: DataTarget, path: NbtPath },
    Modify { target: DataTarget, path: NbtPath, op: ModifyOp, source: ModifySource },
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("data ")?;
        match self {
            Data::Get { target, path, scale } => {
                write!(f, "get {target}{}", OptPath(path))?;
                if let Some(s) = scale {
                    write!(f, " {s}")?;
                }
                Ok(())
            }
            Data::Merge { target, nbt } => write!(f, "merge {target} {nbt}"),
            Data::Remove { target, path } => write!(f, "remove {target} {path}"),
            Data::Modify { target, path, op, source } => {
                write!(f, "modify {target} {path} ")?;
                match op {
                    ModifyOp::Append => f.write_str("append ")?,
                    ModifyOp::Prepend => f.write_str("prepend ")?,
                    ModifyOp::Set => f.write_str("set ")?,
                    ModifyOp::Merge => f.write_str("merge ")?,
                    ModifyOp::Insert(i) => write!(f, "insert {i} ")?,
                }
                match source {
                    ModifySource::From { source, path } => write!(f, "from {source}{}", OptPath(path)),
                    ModifySource::String { source, path, start, end } => {
                        write!(f, "string {source}{}", OptPath(path))?;
                        if let Some(s) = start {
                            write!(f, " {s}")?;
                            if let Some(e) = end {
                                write!(f, " {e}")?;
                            }
                        }
                        Ok(())
                    }
                    ModifySource::Value(v) => write!(f, "value {v}"),
                    ModifySource::Template(parts) => {
                        f.write_str("value \"")?;
                        for p in parts {
                            match p {
                                TemplatePart::Lit(s) => super::nbt::write_quoted_body(f, s)?,
                                TemplatePart::Hole(h) => write!(f, "{h}")?,
                            }
                        }
                        f.write_char('"')
                    }
                    ModifySource::ComputeInt { ctx, provider } => write!(f, "compute {ctx} integer {provider}"),
                    ModifySource::ComputeFloat { ctx, provider } => write!(f, "compute {ctx} float {provider}"),
                }
            }
        }
    }
}

impl Check for Data {
    fn check(&self) -> Result<()> {
        match self {
            Data::Get { target, path, scale } => {
                target.check()?;
                path.check()?;
                if scale.is_some() && path.is_none() {
                    bail!("`data get` with a scale needs a path");
                }
                if scale.is_some_and(|s| !s.is_finite()) {
                    bail!("`data get` scale must be finite");
                }
                Ok(())
            }
            Data::Merge { target, nbt } => {
                target.check()?;
                nbt.check()
            }
            Data::Remove { target, path } => {
                target.check()?;
                path.check()?;
                if path.is_empty() {
                    bail!("`data remove` needs a non-empty path");
                }
                Ok(())
            }
            Data::Modify { target, path, op, source } => {
                target.check()?;
                path.check()?;
                if path.is_empty() {
                    bail!("`data modify` needs a non-empty target path");
                }
                match source {
                    ModifySource::From { source, path } => {
                        source.check()?;
                        path.check()?;
                    }
                    ModifySource::String { source, path, start, end } => {
                        source.check()?;
                        path.check()?;
                        if start.is_some() && path.is_none() {
                            bail!("`data modify ... string` with a range needs a source path");
                        }
                        if end.is_some() && start.is_none() {
                            bail!("`data modify ... string` end index needs a start index");
                        }
                    }
                    ModifySource::Value(v) => {
                        v.check()?;
                        if matches!(op, ModifyOp::Merge) && v.as_compound().is_none() {
                            bail!("`data modify ... merge value` needs a compound");
                        }
                    }
                    ModifySource::Template(_) => {
                        if !matches!(op, ModifyOp::Set) {
                            bail!("a string template is only supported with `data modify ... set`");
                        }
                    }
                    ModifySource::ComputeInt { ctx, provider } | ModifySource::ComputeFloat { ctx, provider } => {
                        match ctx {
                            ComputeContext::Block(p) => p.check_block()?,
                            ComputeContext::Entity(s) => {
                                s.check()?;
                                if !s.is_single() {
                                    bail!("compute context `{s}` must be a single entity");
                                }
                            }
                            ComputeContext::Default => {}
                        }
                        provider.check()?;
                    }
                }
                Ok(())
            }
        }
    }
}

// ----------------------------------------------------------- function and co.

/// Arguments of a `function` call.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum FnArgs {
    /// No arguments; fails at runtime if the function contains macro lines.
    #[default]
    None,
    /// `function ns:f {key:value}`
    Inline(Compound),
    /// `function ns:f with storage ns:args [path]` (the path must select one compound)
    With(DataTarget, Option<NbtPath>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Return {
    Value(i32),
    Fail,
    Run(Box<Command>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleMode {
    Append,
    Replace,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Schedule {
    /// `ticks` must be at least 1.
    Function { function: FunctionRef, ticks: u32, mode: Option<ScheduleMode> },
    /// `schedule clear <id>` takes a plain resource location; `#tag` is a parse error
    /// (verified on 26.3), so a scheduled tag can't be cleared by name.
    Clear(ResourceLocation),
}

#[derive(Clone, Debug, PartialEq)]
pub enum TagAction {
    Add(Arg<Word>),
    Remove(Arg<Word>),
    List,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetblockMode {
    Destroy,
    Keep,
    Replace,
    Strict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuleValue {
    Bool(bool),
    Int(i32),
}

// --------------------------------------------------------------------- effect

/// A command argument that is either known when the program is compiled or filled in at run
/// time (a macro hole, rendered `$(hN)`). Run-time text is substituted as-is, so the value must
/// be valid where it lands (see docs/lang/codegen.md, "String values").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg<T> {
    Lit(T),
    Hole(Hole),
}

impl<T: fmt::Display> fmt::Display for Arg<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arg::Lit(t) => write!(f, "{t}"),
            Arg::Hole(h) => write!(f, "{h}"),
        }
    }
}

impl From<ResourceLocation> for Arg<ResourceLocation> {
    fn from(v: ResourceLocation) -> Self {
        Arg::Lit(v)
    }
}

impl From<Word> for Arg<Word> {
    fn from(v: Word) -> Self {
        Arg::Lit(v)
    }
}

impl From<String> for Arg<String> {
    fn from(v: String) -> Self {
        Arg::Lit(v)
    }
}

impl From<&str> for Arg<String> {
    fn from(v: &str) -> Self {
        Arg::Lit(v.to_owned())
    }
}

/// A non-negative number argument: a literal, or a run-time hole (macro argument).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Num {
    Lit(u32),
    Hole(Hole),
}

impl fmt::Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Num::Lit(n) => write!(f, "{n}"),
            Num::Hole(h) => write!(f, "{h}"),
        }
    }
}

impl From<u32> for Num {
    fn from(n: u32) -> Num {
        Num::Lit(n)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `amplifier` needs `seconds`, and `hide_particles` needs `amplifier`, because the
    /// command's arguments are positional. A hole's range can only be checked at run time.
    Give { targets: Selector, effect: Arg<ResourceLocation>, seconds: Option<Num>, amplifier: Option<Num>, hide_particles: bool },
    /// `effect clear [<targets> [<effect>]]`; an effect needs targets.
    Clear { targets: Option<Selector>, effect: Option<Arg<ResourceLocation>> },
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Effect::Give { targets, effect, seconds, amplifier, hide_particles } => {
                write!(f, "effect give {targets} {effect}")?;
                if let Some(s) = seconds {
                    write!(f, " {s}")?;
                    if let Some(a) = amplifier {
                        write!(f, " {a}")?;
                        if *hide_particles {
                            f.write_str(" true")?;
                        }
                    }
                }
                Ok(())
            }
            Effect::Clear { targets, effect } => {
                f.write_str("effect clear")?;
                if let Some(t) = targets {
                    write!(f, " {t}")?;
                    if let Some(e) = effect {
                        write!(f, " {e}")?;
                    }
                }
                Ok(())
            }
        }
    }
}

impl Check for Effect {
    fn check(&self) -> Result<()> {
        match self {
            Effect::Give { targets, seconds, amplifier, hide_particles, .. } => {
                targets.check()?;
                if let Some(Num::Lit(s)) = seconds
                    && !(1..=1_000_000).contains(s)
                {
                    bail!("effect duration must be 1..=1000000 seconds");
                }
                if let Some(Num::Lit(a)) = amplifier
                    && *a > 255
                {
                    bail!("effect amplifier must be 0..=255");
                }
                if amplifier.is_some() && seconds.is_none() {
                    bail!("an effect amplifier needs a duration");
                }
                if *hide_particles && amplifier.is_none() {
                    bail!("hiding particles needs an amplifier");
                }
                Ok(())
            }
            Effect::Clear { targets, effect } => {
                if effect.is_some() && targets.is_none() {
                    bail!("`effect clear <effect>` needs targets");
                }
                targets.check()
            }
        }
    }
}

// -------------------------------------------------------------------- macros

#[derive(Clone, Debug, PartialEq)]
pub enum MacroPiece {
    Text(String),
    /// `$(name)`, substituted each time the function runs.
    Var(String),
}

/// A command whose text is only known at runtime (`$`-line in the .mcfunction). This is the
/// one form where the caller assembles text, from pieces of typed values' `Display` output
/// and `$(var)` placeholders. Prefer typed commands whenever the value is static.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MacroCommand {
    pub pieces: Vec<MacroPiece>,
}

impl MacroCommand {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(mut self, t: impl fmt::Display) -> Self {
        self.pieces.push(MacroPiece::Text(t.to_string()));
        self
    }

    pub fn var(mut self, name: impl Into<String>) -> Self {
        self.pieces.push(MacroPiece::Var(name.into()));
        self
    }

    pub fn vars(&self) -> impl Iterator<Item = &str> {
        self.pieces.iter().filter_map(|p| match p {
            MacroPiece::Var(v) => Some(v.as_str()),
            MacroPiece::Text(_) => None,
        })
    }
}

impl fmt::Display for MacroCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for p in &self.pieces {
            match p {
                MacroPiece::Text(t) => f.write_str(t)?,
                MacroPiece::Var(v) => write!(f, "$({v})")?,
            }
        }
        Ok(())
    }
}

impl Check for MacroCommand {
    fn check(&self) -> Result<()> {
        if self.pieces.is_empty() {
            bail!("empty macro command");
        }
        for p in &self.pieces {
            match p {
                MacroPiece::Text(t) => {
                    if t.contains('\n') || t.contains('\r') {
                        bail!("a command cannot contain a line break");
                    }
                    if t.contains("$(") {
                        bail!("literal `$(` in macro text; use a Var piece for substitutions");
                    }
                }
                MacroPiece::Var(v) => {
                    if v.is_empty() || !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        bail!("invalid macro variable name {v:?}: use [A-Za-z0-9_]");
                    }
                }
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------- Command

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)] // AST nodes are built once per program; boxing would only add noise
pub enum Command {
    Execute(Execute),
    Scoreboard(Scoreboard),
    Data(Data),
    Function { function: FunctionRef, args: FnArgs },
    Return(Return),
    Schedule(Schedule),
    Tag { targets: Selector, action: TagAction },
    Say(Arg<String>),
    Tellraw { targets: Selector, message: Text },
    Kill(Option<Selector>),
    Summon { entity: Arg<ResourceLocation>, pos: Option<Pos>, nbt: Option<Compound> },
    Setblock { pos: Pos, block: BlockState, mode: Option<SetblockMode> },
    /// `gamerule <rule> [<value>]`; no value queries.
    Gamerule { rule: ResourceLocation, value: Option<GameRuleValue> },
    Effect(Effect),
    /// `teleport <targets> <x y z>`
    Teleport { targets: Selector, dest: Pos },
    Macro(MacroCommand),
    /// Escape hatch for commands the AST doesn't model yet. Only checked for line safety.
    /// Add a typed variant instead of leaning on this.
    Raw(String),
}

impl Command {
    pub fn execute() -> Execute {
        Execute::new()
    }

    pub fn call(function: ResourceLocation) -> Command {
        Command::Function { function: FunctionRef::Id(function), args: FnArgs::None }
    }

    pub fn say(msg: impl Into<String>) -> Command {
        Command::Say(Arg::Lit(msg.into()))
    }

    pub fn tellraw(targets: Selector, message: impl Into<Text>) -> Command {
        Command::Tellraw { targets, message: message.into() }
    }

    pub fn score_set(targets: impl Into<ScoreHolder>, objective: Objective, value: i32) -> Command {
        Command::Scoreboard(Scoreboard::PlayersSet { targets: targets.into(), objective, value })
    }

    /// Adds a signed amount, choosing `add` or `remove` as needed. `i32::MIN` isn't
    /// negatable, so it is split in two steps by the caller.
    pub fn score_add(targets: impl Into<ScoreHolder>, objective: Objective, delta: i32) -> Command {
        let targets = targets.into();
        if delta >= 0 {
            Command::Scoreboard(Scoreboard::PlayersAdd { targets, objective, value: delta })
        } else {
            // i32::MIN can't be negated; wrapping_neg gives a negative value that `check` rejects.
            Command::Scoreboard(Scoreboard::PlayersRemove { targets, objective, value: delta.wrapping_neg() })
        }
    }

    pub fn score_op(
        targets: impl Into<ScoreHolder>,
        target_objective: Objective,
        op: ScoreOp,
        source: impl Into<ScoreHolder>,
        source_objective: Objective,
    ) -> Command {
        Command::Scoreboard(Scoreboard::PlayersOperation {
            targets: targets.into(),
            target_objective,
            op,
            source: source.into(),
            source_objective,
        })
    }

    pub fn return_value(v: i32) -> Command {
        Command::Return(Return::Value(v))
    }

    /// True when the rendered line needs a leading `$` because it contains macro placeholders
    /// (`$(name)`): a [`MacroCommand`] or a command with [`Hole`]s anywhere in it. String literals
    /// escape `$`, so a placeholder can only come from a hole or a macro variable.
    pub fn is_macro(&self) -> bool {
        self.to_string().contains("$(")
    }

    /// The `.mcfunction` line: the command, `$`-prefixed when it is a macro line.
    pub fn render_line(&self) -> String {
        if self.is_macro() { format!("${self}") } else { self.to_string() }
    }

    /// Macro variables this command needs from the caller's arguments.
    pub fn macro_vars(&self, out: &mut Vec<String>) {
        let text = self.to_string();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("$(") {
            rest = &rest[i + 2..];
            if let Some(end) = rest.find(')') {
                out.push(rest[..end].to_owned());
                rest = &rest[end..];
            }
        }
    }

    /// Every function or function tag this command can invoke, with how arguments are passed.
    pub fn function_calls(&self, out: &mut Vec<FunctionCall>) {
        match self {
            Command::Function { function, args } => {
                let args = match args {
                    FnArgs::None => CallArgs::None,
                    FnArgs::Inline(c) => CallArgs::Inline(c.iter().map(|(k, _)| k.to_owned()).collect()),
                    FnArgs::With(..) => CallArgs::With,
                };
                out.push(FunctionCall { target: function.clone(), args });
            }
            Command::Schedule(Schedule::Function { function, .. }) => {
                out.push(FunctionCall { target: function.clone(), args: CallArgs::None })
            }
            Command::Execute(e) => {
                for c in &e.clauses {
                    if let Clause::If(Condition::Function(r)) | Clause::Unless(Condition::Function(r)) = c {
                        out.push(FunctionCall { target: r.clone(), args: CallArgs::None });
                    }
                }
                if let Some(r) = &e.run {
                    r.function_calls(out);
                }
            }
            Command::Return(Return::Run(c)) => c.function_calls(out),
            _ => {}
        }
    }
}

impl Command {
    /// Predicates that must exist when this command is parsed: `execute if predicate <id>`
    /// is resolved at load time, so a missing one rejects the whole function. (Selector
    /// `predicate=` arguments resolve later, at run time.)
    pub fn predicate_refs(&self, out: &mut Vec<ResourceLocation>) {
        match self {
            Command::Execute(e) => {
                for c in &e.clauses {
                    if let Clause::If(Condition::Predicate(id)) | Clause::Unless(Condition::Predicate(id)) = c {
                        out.push(id.clone());
                    }
                }
                if let Some(r) = &e.run {
                    r.predicate_refs(out);
                }
            }
            Command::Return(Return::Run(c)) => c.predicate_refs(out),
            _ => {}
        }
    }
}

/// How a call passes macro arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallArgs {
    None,
    /// Inline compound with these keys.
    Inline(Vec<String>),
    /// `with <storage|entity|block> ...`: keys unknown statically.
    With,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionCall {
    pub target: FunctionRef,
    pub args: CallArgs,
}

fn single_line(what: &str, s: &str) -> Result<()> {
    if s.is_empty() {
        bail!("{what} must not be empty");
    }
    if s.contains('\n') || s.contains('\r') {
        bail!("{what} must be a single line");
    }
    Ok(())
}

impl Check for Command {
    fn check(&self) -> Result<()> {
        match self {
            Command::Execute(e) => e.check(),
            Command::Scoreboard(s) => s.check(),
            Command::Data(d) => d.check(),
            Command::Function { args, .. } => match args {
                FnArgs::None => Ok(()),
                FnArgs::Inline(c) => c.check(),
                FnArgs::With(t, p) => {
                    t.check()?;
                    p.check()
                }
            },
            Command::Return(Return::Run(c)) => c.check(),
            Command::Return(_) => Ok(()),
            Command::Schedule(Schedule::Function { ticks, .. }) => {
                if *ticks == 0 {
                    bail!("cannot schedule a function for the current tick (0 ticks)");
                }
                if *ticks > i32::MAX as u32 {
                    bail!("schedule delay {ticks} exceeds the maximum");
                }
                Ok(())
            }
            Command::Schedule(Schedule::Clear(_)) => Ok(()),
            Command::Tag { targets, .. } => targets.check(),
            Command::Say(Arg::Hole(_)) => Ok(()),
            Command::Say(Arg::Lit(m)) => {
                single_line("say message", m)?;
                if m.contains("$(") {
                    bail!("a `say` message cannot contain `$(` (it would be read as a macro placeholder)");
                }
                Ok(())
            }
            Command::Tellraw { targets, message } => {
                targets.check()?;
                message.check()
            }
            Command::Kill(s) => s.check(),
            Command::Summon { pos, nbt, .. } => {
                if nbt.is_some() && pos.is_none() {
                    bail!("`summon` with NBT needs a position");
                }
                pos.check()?;
                nbt.check()
            }
            Command::Setblock { pos, block, .. } => {
                pos.check_block()?;
                block.check()
            }
            Command::Gamerule { .. } => Ok(()),
            Command::Effect(e) => e.check(),
            Command::Teleport { targets, dest } => {
                targets.check()?;
                dest.check()
            }
            Command::Macro(m) => m.check(),
            Command::Raw(s) => {
                single_line("raw command", s)?;
                if s.starts_with(['/', '$', '#']) || s.starts_with(char::is_whitespace) {
                    bail!("raw command must not start with `/`, `$`, `#` or whitespace: {s:?}");
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for Command {
    /// The command text without the macro `$` prefix; see [`Command::render_line`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Execute(e) => write!(f, "{e}"),
            Command::Scoreboard(s) => write!(f, "{s}"),
            Command::Data(d) => write!(f, "{d}"),
            Command::Function { function, args } => {
                write!(f, "function {function}")?;
                match args {
                    FnArgs::None => Ok(()),
                    FnArgs::Inline(c) => write!(f, " {c}"),
                    FnArgs::With(t, p) => write!(f, " with {t}{}", OptPath(p)),
                }
            }
            Command::Return(Return::Value(v)) => write!(f, "return {v}"),
            Command::Return(Return::Fail) => f.write_str("return fail"),
            Command::Return(Return::Run(c)) => write!(f, "return run {c}"),
            Command::Schedule(Schedule::Function { function, ticks, mode }) => {
                write!(f, "schedule function {function} {ticks}t")?;
                match mode {
                    Some(ScheduleMode::Append) => f.write_str(" append"),
                    Some(ScheduleMode::Replace) => f.write_str(" replace"),
                    None => Ok(()),
                }
            }
            Command::Schedule(Schedule::Clear(r)) => write!(f, "schedule clear {r}"),
            Command::Tag { targets, action } => match action {
                TagAction::Add(t) => write!(f, "tag {targets} add {t}"),
                TagAction::Remove(t) => write!(f, "tag {targets} remove {t}"),
                TagAction::List => write!(f, "tag {targets} list"),
            },
            Command::Say(m) => write!(f, "say {m}"),
            Command::Tellraw { targets, message } => write!(f, "tellraw {targets} {message}"),
            Command::Kill(None) => f.write_str("kill"),
            Command::Kill(Some(s)) => write!(f, "kill {s}"),
            Command::Summon { entity, pos, nbt } => {
                write!(f, "summon {entity}")?;
                if let Some(p) = pos {
                    write!(f, " {p}")?;
                    if let Some(n) = nbt {
                        write!(f, " {n}")?;
                    }
                }
                Ok(())
            }
            Command::Setblock { pos, block, mode } => {
                write!(f, "setblock {pos} {block}")?;
                match mode {
                    Some(SetblockMode::Destroy) => f.write_str(" destroy"),
                    Some(SetblockMode::Keep) => f.write_str(" keep"),
                    Some(SetblockMode::Replace) => f.write_str(" replace"),
                    Some(SetblockMode::Strict) => f.write_str(" strict"),
                    None => Ok(()),
                }
            }
            Command::Gamerule { rule, value } => {
                write!(f, "gamerule {rule}")?;
                match value {
                    Some(GameRuleValue::Bool(b)) => write!(f, " {b}"),
                    Some(GameRuleValue::Int(i)) => write!(f, " {i}"),
                    None => Ok(()),
                }
            }
            Command::Effect(e) => write!(f, "{e}"),
            Command::Teleport { targets, dest } => write!(f, "teleport {targets} {dest}"),
            Command::Macro(m) => write!(f, "{m}"),
            Command::Raw(s) => f.write_str(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datapack::names::{holder, rl, word};

    fn obj() -> Objective {
        word("pts")
    }

    #[test]
    fn execute_chain_and_flatten() {
        let inner = Command::execute().at(Selector::s()).run(Command::say("hi"));
        let cmd = Command::execute()
            .as_(Selector::a().tag(word("x")))
            .if_score_matches(Selector::s().into(), obj(), IntRange::at_least(1))
            .run(inner);
        assert_eq!(cmd.to_string(), "execute as @a[tag=x] if score @s pts matches 1.. at @s run say hi");
        assert!(cmd.check().is_ok());
        // empty chain collapses to the command itself
        assert_eq!(Command::execute().run(Command::say("x")).to_string(), "say x");
    }

    #[test]
    fn execute_termination_rules() {
        assert!(Command::execute().as_(Selector::s()).into_test().is_err());
        assert!(Command::execute().into_test().is_err());
        let c = Command::execute().if_(Condition::Entity(Selector::e().tag(word("t")))).into_test().unwrap();
        assert_eq!(c.to_string(), "execute if entity @e[tag=t]");
        let f = Command::execute().if_(Condition::Function(FunctionRef::Id(rl("a:b"))));
        assert!(f.clone().into_test().is_err());
        assert_eq!(f.run(Command::say("x")).to_string(), "execute if function a:b run say x");
    }

    #[test]
    fn execute_store() {
        let c = Command::execute()
            .store(StoreKind::Result, StoreTarget::Score { targets: ScoreHolder::fake(holder("#r")), objective: obj() })
            .store(
                StoreKind::Success,
                StoreTarget::Data {
                    target: DataTarget::Storage(rl("ns:s")),
                    path: NbtPath::new().key("ok"),
                    ty: NumType::Byte,
                    scale: 1.0,
                },
            )
            .run(Command::call(rl("ns:f")));
        assert_eq!(
            c.to_string(),
            "execute store result score #r pts store success storage ns:s ok byte 1 run function ns:f"
        );
        assert!(c.check().is_ok());
    }

    #[test]
    fn scoreboard_commands() {
        let c = Command::score_add(Selector::a(), obj(), -3);
        assert_eq!(c.to_string(), "scoreboard players remove @a pts 3");
        let c = Command::score_add(ScoreHolder::fake(holder("#n")), obj(), 4);
        assert_eq!(c.to_string(), "scoreboard players add #n pts 4");
        let c = Command::score_op(ScoreHolder::fake(holder("#a")), obj(), ScoreOp::Add, ScoreHolder::fake(holder("#b")), word("other"));
        assert_eq!(c.to_string(), "scoreboard players operation #a pts += #b other");
        let bad = Command::Scoreboard(Scoreboard::PlayersAdd { targets: Selector::s().into(), objective: obj(), value: -1 });
        assert!(bad.check().is_err());
        let get = Command::Scoreboard(Scoreboard::PlayersGet { target: ScoreHolder::All, objective: obj() });
        assert!(get.check().is_err());
        let add = Command::Scoreboard(Scoreboard::ObjectivesAdd {
            name: obj(),
            criteria: Criteria::Dummy,
            display_name: Some(Text::plain("Points")),
        });
        assert_eq!(add.to_string(), r#"scoreboard objectives add pts dummy "Points""#);
    }

    #[test]
    fn data_commands() {
        let t = DataTarget::Storage(rl("ns:vars"));
        let set = Command::Data(Data::Modify {
            target: t.clone(),
            path: NbtPath::new().key("list"),
            op: ModifyOp::Append,
            source: ModifySource::Value(super::super::nbt::Nbt::Int(5)),
        });
        assert_eq!(set.to_string(), "data modify storage ns:vars list append value 5");
        let from = Command::Data(Data::Modify {
            target: t.clone(),
            path: NbtPath::new().key("a"),
            op: ModifyOp::Insert(0),
            source: ModifySource::From { source: DataTarget::Entity(Selector::s()), path: Some(NbtPath::new().key("Pos").index(1)) },
        });
        assert_eq!(from.to_string(), "data modify storage ns:vars a insert 0 from entity @s Pos[1]");
        let get = Command::Data(Data::Get { target: t.clone(), path: Some(NbtPath::new().key("a")), scale: Some(100.0) });
        assert_eq!(get.to_string(), "data get storage ns:vars a 100");
        let bad = Command::Data(Data::Get { target: t.clone(), path: None, scale: Some(2.0) });
        assert!(bad.check().is_err());
        let compute = Command::Data(Data::Modify {
            target: t,
            path: NbtPath::new().key("f"),
            op: ModifyOp::Set,
            source: ModifySource::ComputeFloat { ctx: ComputeContext::Default, provider: Provider::constant(2.5f64) },
        });
        assert_eq!(
            compute.to_string(),
            r#"data modify storage ns:vars f set compute default float {type:"minecraft:constant",value:2.5d}"#
        );
        let multi = Command::Data(Data::Remove { target: DataTarget::Entity(Selector::a()), path: NbtPath::new().key("x") });
        assert!(multi.check().is_err());
    }

    #[test]
    fn functions_returns_schedule() {
        let args = Compound::new().with("n", 3).with("who", "Bob");
        let c = Command::Function { function: FunctionRef::Id(rl("ns:f")), args: FnArgs::Inline(args) };
        assert_eq!(c.to_string(), r#"function ns:f {n:3,who:"Bob"}"#);
        let w = Command::Function {
            function: FunctionRef::Id(rl("ns:f")),
            args: FnArgs::With(DataTarget::Storage(rl("ns:args")), Some(NbtPath::new().key("a"))),
        };
        assert_eq!(w.to_string(), "function ns:f with storage ns:args a");
        assert_eq!(Command::Return(Return::Fail).to_string(), "return fail");
        assert_eq!(Command::return_value(7).to_string(), "return 7");
        let r = Command::Return(Return::Run(Box::new(Command::call(rl("ns:g")))));
        assert_eq!(r.to_string(), "return run function ns:g");
        let s = Command::Schedule(Schedule::Function { function: FunctionRef::Id(rl("ns:t")), ticks: 20, mode: Some(ScheduleMode::Append) });
        assert_eq!(s.to_string(), "schedule function ns:t 20t append");
        let zero = Command::Schedule(Schedule::Function { function: FunctionRef::Id(rl("ns:t")), ticks: 0, mode: None });
        assert!(zero.check().is_err());
    }

    #[test]
    fn macros() {
        let m = MacroCommand::new().text("say Hello ").var("name");
        let c = Command::Macro(m);
        assert!(c.is_macro());
        assert_eq!(c.render_line(), "$say Hello $(name)");
        let wrapped = Command::execute().as_(Selector::a()).run(c.clone());
        assert!(wrapped.is_macro());
        assert_eq!(wrapped.render_line(), "$execute as @a run say Hello $(name)");
        let mut vars = vec![];
        wrapped.macro_vars(&mut vars);
        assert_eq!(vars, ["name"]);
        assert!(Command::Macro(MacroCommand::new().var("bad name")).check().is_err());
        assert!(Command::Macro(MacroCommand::new().text("say $(x)")).check().is_err());
        assert!(!Command::say("plain").is_macro());
    }

    #[test]
    fn effect_and_teleport() {
        let give = |seconds: Option<u32>, amplifier: Option<u32>, hide| {
            Command::Effect(Effect::Give {
                targets: Selector::s(),
                effect: rl("speed").into(),
                seconds: seconds.map(Num::Lit),
                amplifier: amplifier.map(Num::Lit),
                hide_particles: hide,
            })
        };
        assert_eq!(give(None, None, false).to_string(), "effect give @s minecraft:speed");
        assert_eq!(give(Some(30), None, false).to_string(), "effect give @s minecraft:speed 30");
        assert_eq!(give(Some(30), Some(2), false).to_string(), "effect give @s minecraft:speed 30 2");
        assert_eq!(give(Some(30), Some(2), true).to_string(), "effect give @s minecraft:speed 30 2 true");
        assert!(give(Some(30), Some(2), true).check().is_ok());
        assert!(give(None, Some(1), false).check().is_err());
        assert!(give(Some(1), None, true).check().is_err());
        assert!(give(Some(0), None, false).check().is_err());
        assert!(give(Some(1_000_001), None, false).check().is_err());
        assert!(give(Some(5), Some(256), false).check().is_err());
        let clear = |t: Option<Selector>, e: Option<ResourceLocation>| Command::Effect(Effect::Clear { targets: t, effect: e.map(Arg::Lit) });
        assert_eq!(clear(None, None).to_string(), "effect clear");
        assert_eq!(clear(Some(Selector::a()), None).to_string(), "effect clear @a");
        assert_eq!(clear(Some(Selector::a()), Some(rl("speed"))).to_string(), "effect clear @a minecraft:speed");
        assert!(clear(None, Some(rl("speed"))).check().is_err());
        let tp = Command::Teleport { targets: Selector::s(), dest: Pos::rel(0.0, 1.0, 0.0) };
        assert_eq!(tp.to_string(), "teleport @s ~ ~1 ~");
        assert!(Command::Teleport { targets: Selector::s(), dest: Pos::new(Coord::Rel(0.0), Coord::Local(1.0), Coord::Rel(0.0)) }.check().is_err());
    }

    #[test]
    fn holes_make_a_command_a_macro_line() {
        let tp = Command::Teleport { targets: Selector::s(), dest: Pos::new(Coord::Hole(Hole(0)), Coord::Abs(64.0), Coord::Hole(Hole(1))) };
        assert!(tp.is_macro());
        assert_eq!(tp.render_line(), "$teleport @s $(h0) 64 $(h1)");
        let mut vars = vec![];
        tp.macro_vars(&mut vars);
        assert_eq!(vars, ["h0", "h1"]);
        // through an `execute ... run`, a hole anywhere makes the whole line a macro
        let nested = Command::execute().as_(Selector::a()).run(tp.clone());
        assert_eq!(nested.render_line(), "$execute as @a run teleport @s $(h0) 64 $(h1)");
        let eff = Command::Effect(Effect::Give {
            targets: Selector::s(),
            effect: rl("speed").into(),
            seconds: Some(Num::Hole(Hole(0))),
            amplifier: Some(Num::Hole(Hole(1))),
            hide_particles: false,
        });
        assert_eq!(eff.render_line(), "$effect give @s minecraft:speed $(h0) $(h1)");
        // a literal `$(` in text is never a placeholder
        assert!(!Command::tellraw(Selector::a(), "cost $(5)").is_macro());
        assert!(Command::say("cost $(5)").check().is_err());
        assert!(!Command::say("cost $5").is_macro());
        assert!(!Command::say("plain").is_macro());
    }

    #[test]
    fn text_arguments_can_be_holes() {
        fn h<T>(n: u16) -> Arg<T> {
            Arg::Hole(Hole(n))
        }
        let tag = Command::Tag { targets: Selector::s(), action: TagAction::Add(h(0)) };
        assert!(tag.is_macro());
        assert_eq!(tag.render_line(), "$tag @s add $(h0)");
        let summon = Command::Summon { entity: h(0), pos: Some(Pos::here()), nbt: None };
        assert_eq!(summon.render_line(), "$summon $(h0) ~ ~ ~");
        let block = Command::Setblock { pos: Pos::block(1, 2, 3), block: BlockState::new(h(1)), mode: None };
        assert_eq!(block.render_line(), "$setblock 1 2 3 $(h1)");
        let clear = Command::Effect(Effect::Clear { targets: Some(Selector::a()), effect: Some(h(0)) });
        assert_eq!(clear.render_line(), "$effect clear @a $(h0)");
        let in_exec = Command::execute().clause(Clause::Summon(h(0))).run(Command::say("x"));
        assert_eq!(in_exec.render_line(), "$execute summon $(h0) run say x");
        assert_eq!(Command::Say(h(0)).render_line(), "$say $(h0)");
        assert!(Command::Say(h(0)).check().is_ok());
        let mut vars = vec![];
        block.macro_vars(&mut vars);
        assert_eq!(vars, ["h1"]);
    }

    #[test]
    fn string_templates_escape_literal_text_only() {
        let data = |parts| {
            Command::Data(Data::Modify {
                target: DataTarget::Storage(rl("t:vars")),
                path: NbtPath::new().key("s"),
                op: ModifyOp::Set,
                source: ModifySource::Template(parts),
            })
        };
        let joined = data(vec![TemplatePart::Lit("a\"b $(c) ".into()), TemplatePart::Hole(Hole(0)), TemplatePart::Lit("!".into())]);
        // quotes and `$` in the literal parts are escaped, so only the hole is a placeholder
        assert_eq!(joined.render_line(), r#"$data modify storage t:vars s set value "a\"b \u0024(c) $(h0)!""#);
        let mut vars = vec![];
        joined.macro_vars(&mut vars);
        assert_eq!(vars, ["h0"]);
        assert!(joined.check().is_ok());
        // a template without holes is an ordinary command
        let plain = data(vec![TemplatePart::Lit("x".into())]);
        assert!(!plain.is_macro());
        assert_eq!(plain.to_string(), r#"data modify storage t:vars s set value "x""#);
        let append = Command::Data(Data::Modify {
            target: DataTarget::Storage(rl("t:vars")),
            path: NbtPath::new().key("s"),
            op: ModifyOp::Append,
            source: ModifySource::Template(vec![]),
        });
        assert!(append.check().is_err());
    }

    #[test]
    fn misc_commands() {
        assert_eq!(Command::Kill(None).to_string(), "kill");
        let t = Command::Tag { targets: Selector::s(), action: TagAction::Add(word("done").into()) };
        assert_eq!(t.to_string(), "tag @s add done");
        let s = Command::Summon { entity: rl("marker").into(), pos: Some(Pos::here()), nbt: Some(Compound::new().with("Tags", vec![super::super::nbt::Nbt::str("m")])) };
        assert_eq!(s.to_string(), r#"summon minecraft:marker ~ ~ ~ {Tags:["m"]}"#);
        let sb = Command::Setblock {
            pos: Pos::block(0, 64, 0),
            block: BlockState::new(rl("oak_stairs")).prop(word("facing"), word("north")),
            mode: Some(SetblockMode::Replace),
        };
        assert_eq!(sb.to_string(), "setblock 0 64 0 minecraft:oak_stairs[facing=north] replace");
        let gr = Command::Gamerule { rule: rl("advance_time"), value: Some(GameRuleValue::Bool(false)) };
        assert_eq!(gr.to_string(), "gamerule minecraft:advance_time false");
        assert!(Command::Raw("/say hi".into()).check().is_err());
        assert!(Command::Raw("say hi".into()).check().is_ok());
        assert!(Command::Raw("say a\nsay b".into()).check().is_err());
    }
}
