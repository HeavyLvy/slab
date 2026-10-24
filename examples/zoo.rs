//! Builds a pack that uses every command variant the generator supports, so the real game can
//! judge them: `cargo run --example zoo -- target/zoo && tools/verify_pack.py target/zoo --run zoo:all`

use std::path::PathBuf;

use slab::datapack::*;

fn obj() -> Objective {
    word("pts")
}

fn fake(name: &str) -> ScoreHolder {
    ScoreHolder::fake(holder(name))
}

fn marker() -> Selector {
    Selector::e().tag(word("zoo_dummy"))
}

fn scoreboard() -> Function {
    let mut f = Function::new();
    f.comment("scoreboard: every variant");
    let sb = Command::Scoreboard;
    f.push(sb(Scoreboard::ObjectivesAdd { name: obj(), criteria: Criteria::Dummy, display_name: Some(Text::plain("Points").color(NamedColor::Gold)) }));
    f.push(sb(Scoreboard::ObjectivesAdd { name: word("trig"), criteria: Criteria::Trigger, display_name: None }));
    f.push(sb(Scoreboard::ObjectivesAdd { name: word("deaths"), criteria: Criteria::Custom("deathCount".into()), display_name: None }));
    f.push(sb(Scoreboard::ObjectivesAdd { name: word("jumps"), criteria: Criteria::Custom("minecraft.custom:minecraft.jump".into()), display_name: None }));
    f.push(sb(Scoreboard::ObjectivesList));
    f.push(sb(Scoreboard::ObjectivesModify { objective: obj(), change: ObjectiveModify::DisplayName(Text::plain("Pts")) }));
    f.push(sb(Scoreboard::ObjectivesModify { objective: obj(), change: ObjectiveModify::RenderType(RenderType::Integer) }));
    f.push(sb(Scoreboard::ObjectivesModify { objective: obj(), change: ObjectiveModify::DisplayAutoUpdate(true) }));
    f.push(sb(Scoreboard::ObjectivesSetDisplay { slot: DisplaySlot::Sidebar, objective: Some(obj()) }));
    f.push(sb(Scoreboard::ObjectivesSetDisplay { slot: DisplaySlot::SidebarTeam(NamedColor::DarkPurple), objective: Some(obj()) }));
    f.push(sb(Scoreboard::ObjectivesSetDisplay { slot: DisplaySlot::BelowName, objective: Some(word("deaths")) }));
    f.push(sb(Scoreboard::ObjectivesSetDisplay { slot: DisplaySlot::List, objective: Some(obj()) }));
    f.push(sb(Scoreboard::ObjectivesSetDisplay { slot: DisplaySlot::Sidebar, objective: None }));
    f.push(Command::score_set(fake("#a"), obj(), 10));
    f.push(Command::score_set(Selector::a(), obj(), -5));
    f.push(Command::score_add(fake("#a"), obj(), 3));
    f.push(Command::score_add(fake("#a"), obj(), -3));
    f.push(Command::score_set(ScoreHolder::All, obj(), 0));
    f.push(Command::score_set(fake("#b"), obj(), 7));
    for op in [ScoreOp::Set, ScoreOp::Add, ScoreOp::Sub, ScoreOp::Mul, ScoreOp::Div, ScoreOp::Mod, ScoreOp::Min, ScoreOp::Max, ScoreOp::Swap] {
        f.push(Command::score_op(fake("#a"), obj(), op, fake("#b"), obj()));
    }
    f.push(sb(Scoreboard::PlayersGet { target: fake("#a"), objective: obj() }));
    f.push(sb(Scoreboard::PlayersGet { target: Selector::s().into(), objective: obj() }));
    f.push(sb(Scoreboard::PlayersEnable { targets: Selector::a().into(), objective: word("trig") }));
    f.push(sb(Scoreboard::PlayersList(None)));
    f.push(sb(Scoreboard::PlayersList(Some(fake("#a")))));
    f.push(sb(Scoreboard::PlayersReset { targets: fake("#a"), objective: Some(obj()) }));
    f.push(sb(Scoreboard::PlayersReset { targets: fake("#b"), objective: None }));
    f.push(sb(Scoreboard::PlayersReset { targets: ScoreHolder::All, objective: Some(obj()) }));
    f.push(sb(Scoreboard::ObjectivesRemove(word("jumps"))));
    f
}

fn execute() -> Function {
    let mut f = Function::new();
    f.comment("execute: every clause and condition");
    let s = Selector::s;
    let storage = || DataTarget::Storage(rl("zoo:vars"));
    let say = || Command::say("ok");
    let ex = Command::execute;

    f.push(Command::Summon { entity: rl("marker").into(), pos: Some(Pos::here()), nbt: Some(Compound::new().with("Tags", vec![Nbt::str("zoo_dummy")])) });
    f.push(ex().as_(Selector::a()).at(s()).run(say()));
    f.push(ex().positioned(Pos::abs(0.0, 64.0, 0.0)).clause(Clause::PositionedAs(marker().limit(1))).run(say()));
    f.push(ex().clause(Clause::PositionedOver(Heightmap::WorldSurface)).run(say()));
    f.push(ex().clause(Clause::Rotated(Rotation::abs(90.0, 0.0))).run(say()));
    f.push(ex().clause(Clause::Rotated(Rotation::rel(0.0, 0.0))).clause(Clause::RotatedAs(marker().limit(1))).run(say()));
    f.push(ex().clause(Clause::Facing(Pos::abs(0.0, 64.0, 0.0))).run(say()));
    f.push(ex().clause(Clause::FacingEntity(marker().limit(1), Anchor::Eyes)).run(say()));
    f.push(ex().clause(Clause::Align(Axes::XYZ)).clause(Clause::Align(Axes::XZ)).run(say()));
    f.push(ex().clause(Clause::Anchored(Anchor::Eyes)).positioned(Pos::local(0.0, 0.0, 1.0)).run(say()));
    f.push(ex().clause(Clause::In(rl("the_nether"))).clause(Clause::In(rl("overworld"))).run(say()));
    for rel in [Relation::Attacker, Relation::Controller, Relation::Leasher, Relation::Origin, Relation::Owner, Relation::Passengers, Relation::Target, Relation::Vehicle] {
        f.push(ex().as_(marker()).clause(Clause::On(rel)).run(say()));
    }
    f.push(ex().clause(Clause::Summon(rl("pig").into())).run(Command::Kill(Some(s()))));

    // store: all targets
    f.push(ex().store(StoreKind::Result, StoreTarget::Score { targets: fake("#r"), objective: obj() }).run(Command::Data(Data::Get { target: storage(), path: None, scale: None })));
    f.push(ex().store(StoreKind::Success, StoreTarget::Score { targets: Selector::a().into(), objective: obj() }).run(say()));
    for ty in [NumType::Byte, NumType::Short, NumType::Int, NumType::Long, NumType::Float, NumType::Double] {
        f.push(ex().store(StoreKind::Result, StoreTarget::Data { target: storage(), path: NbtPath::new().key("n"), ty, scale: 0.5 }).run(Command::score_set(fake("#a"), obj(), 5)));
    }
    f.push(ex().store(StoreKind::Result, StoreTarget::Data { target: DataTarget::Entity(marker().limit(1)), path: NbtPath::new().key("Air"), ty: NumType::Short, scale: 1.0 }).run(say()));
    f.push(ex().store(StoreKind::Result, StoreTarget::Data { target: DataTarget::Block(Pos::block(0, 64, 0)), path: NbtPath::new().key("x"), ty: NumType::Int, scale: 1.0 }).run(say()));
    f.push(ex().store(StoreKind::Result, StoreTarget::Bossbar { id: rl("zoo:bar"), field: BossbarField::Value }).run(say()));
    f.push(ex().store(StoreKind::Success, StoreTarget::Bossbar { id: rl("zoo:bar"), field: BossbarField::Max }).run(say()));

    // conditions, both polarities, each as a terminal test
    let conds = || -> Vec<Condition> {
        vec![
            Condition::Biome(Pos::here(), IdOrTag::Id(rl("plains"))),
            Condition::Biome(Pos::here(), IdOrTag::Tag(rl("is_overworld"))),
            Condition::Block(Pos::here(), BlockPredicate::block(rl("grass_block")).prop(word("snowy"), word("false"))),
            Condition::Block(Pos::block(0, 64, 0), BlockPredicate::new(IdOrTag::Tag(rl("logs")))),
            Condition::Blocks { start: Pos::here(), end: Pos::here(), dest: Pos::here(), mode: BlocksMode::All },
            Condition::Blocks { start: Pos::here(), end: Pos::here(), dest: Pos::here(), mode: BlocksMode::Masked },
            Condition::Data(storage(), NbtPath::new().key("n")),
            Condition::Data(DataTarget::Entity(s()), NbtPath::new().key("Tags").all()),
            Condition::Data(DataTarget::Block(Pos::here()), NbtPath::new().key("Items").element_filter(Compound::new().with("Slot", Nbt::Byte(0)))),
            Condition::Dimension(rl("overworld")),
            Condition::Entity(marker()),
            Condition::Loaded(Pos::here()),
            Condition::Predicate(rl("zoo:never")),
            Condition::Score { target: fake("#a"), objective: obj(), test: ScoreTest::Matches(IntRange::between(1, 5)) },
            Condition::Score { target: fake("#a"), objective: obj(), test: ScoreTest::Matches(IntRange::at_most(5)) },
            Condition::Score { target: fake("#a"), objective: obj(), test: ScoreTest::Matches(IntRange::exactly(5)) },
        ]
    };
    for c in conds() {
        f.push(ex().if_(c.clone()).into_test().unwrap());
        f.push(ex().unless(c).into_test().unwrap());
    }
    for cmp in [Cmp::Lt, Cmp::Le, Cmp::Eq, Cmp::Ge, Cmp::Gt] {
        f.push(ex().if_(Condition::Score { target: fake("#a"), objective: obj(), test: ScoreTest::Compare(cmp, fake("#b"), obj()) }).into_test().unwrap());
    }
    f.push(ex().if_(Condition::Function(FunctionRef::Id(rl("zoo:returns")))).run(say()));
    f.push(ex().unless(Condition::Function(FunctionRef::Id(rl("zoo:returns")))).run(say()));
    f.push(ex().if_(Condition::Entity(marker())).unless(Condition::Entity(Selector::p())).as_(marker().limit(1)).run(say()));
    f.push(Command::Kill(Some(marker())));
    f
}

fn data() -> Function {
    let mut f = Function::new();
    f.comment("data: every variant");
    let t = || DataTarget::Storage(rl("zoo:vars"));
    let d = Command::Data;
    f.push(d(Data::Merge { target: t(), nbt: Compound::new().with("n", 1).with("s", "hi").with("list", vec![Nbt::Int(1), Nbt::Int(2)]).with("obj", Compound::new().with("k", Nbt::Byte(1))) }));
    f.push(d(Data::Get { target: t(), path: None, scale: None }));
    f.push(d(Data::Get { target: t(), path: Some(NbtPath::new().key("n")), scale: None }));
    f.push(d(Data::Get { target: t(), path: Some(NbtPath::new().key("n")), scale: Some(2.5) }));
    f.push(d(Data::Get { target: t(), path: Some(NbtPath::new().key("list").index(-1)), scale: None }));
    f.push(d(Data::Get { target: t(), path: Some(NbtPath::new().root_filter(Compound::new().with("n", 1))), scale: None }));
    for (op, name) in [(ModifyOp::Append, "append"), (ModifyOp::Prepend, "prepend"), (ModifyOp::Insert(1), "insert"), (ModifyOp::Insert(-1), "insert_neg")] {
        let _ = name;
        f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("list"), op, source: ModifySource::Value(Nbt::Int(9)) }));
    }
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("a").key("b"), op: ModifyOp::Set, source: ModifySource::Value(Nbt::str("deep")) }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("obj"), op: ModifyOp::Merge, source: ModifySource::Value(Nbt::Compound(Compound::new().with("z", 1))) }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("copy"), op: ModifyOp::Set, source: ModifySource::From { source: t(), path: Some(NbtPath::new().key("list")) } }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("whole"), op: ModifyOp::Set, source: ModifySource::From { source: DataTarget::Entity(Selector::s()), path: None } }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("sub"), op: ModifyOp::Set, source: ModifySource::String { source: t(), path: Some(NbtPath::new().key("s")), start: Some(0), end: Some(1) } }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("sub2"), op: ModifyOp::Set, source: ModifySource::String { source: t(), path: Some(NbtPath::new().key("s")), start: Some(1), end: None } }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("sub3"), op: ModifyOp::Set, source: ModifySource::String { source: t(), path: Some(NbtPath::new().key("s")), start: None, end: None } }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("ci"), op: ModifyOp::Set, source: ModifySource::ComputeInt { ctx: ComputeContext::Default, provider: Provider::constant(5) } }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("cf"), op: ModifyOp::Set, source: ModifySource::ComputeFloat { ctx: ComputeContext::Default, provider: Provider::constant(2.5f32) } }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("ce"), op: ModifyOp::Set, source: ModifySource::ComputeInt { ctx: ComputeContext::Entity(Selector::s()), provider: Provider::constant(1) } }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("list").all(), op: ModifyOp::Set, source: ModifySource::Value(Nbt::Int(0)) }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("arr"), op: ModifyOp::Set, source: ModifySource::Value(Nbt::IntArray(vec![1, 2, 3])) }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("bytes"), op: ModifyOp::Set, source: ModifySource::Value(Nbt::ByteArray(vec![1, 2])) }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("longs"), op: ModifyOp::Set, source: ModifySource::Value(Nbt::LongArray(vec![1, 2])) }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("tricky key.with\"stuff"), op: ModifyOp::Set, source: ModifySource::Value(Nbt::str("quote \" and \\ and \n newline")) }));
    f.push(d(Data::Modify { target: t(), path: NbtPath::new().key("nums"), op: ModifyOp::Set, source: ModifySource::Value(Nbt::List(vec![Nbt::Byte(1), Nbt::Short(2), Nbt::Long(3), Nbt::Float(4.5), Nbt::Double(6.0)])) }));
    f.push(d(Data::Remove { target: t(), path: NbtPath::new().key("n") }));
    f.push(d(Data::Remove { target: t(), path: NbtPath::new().key("list").element_filter(Compound::new().with("x", 1)) }));
    f
}

fn returns() -> Function {
    let mut f = Function::new();
    f.push(Command::return_value(5));
    f
}

fn flow() -> Function {
    let mut f = Function::new();
    f.comment("function/return/schedule");
    f.push(Command::call(rl("zoo:returns")));
    f.push(Command::Function { function: FunctionRef::Tag(rl("minecraft:load")), args: FnArgs::None });
    f.push(Command::Function { function: FunctionRef::Id(rl("zoo:mac")), args: FnArgs::Inline(Compound::new().with("who", "Inline").with("n", 3)) });
    f.push(Command::Function { function: FunctionRef::Id(rl("zoo:mac")), args: FnArgs::With(DataTarget::Storage(rl("zoo:args")), None) });
    f.push(Command::Function { function: FunctionRef::Id(rl("zoo:mac")), args: FnArgs::With(DataTarget::Storage(rl("zoo:args")), Some(NbtPath::new().key("nested"))) });
    f.push(Command::Function { function: FunctionRef::Id(rl("zoo:mac")), args: FnArgs::With(DataTarget::Entity(Selector::s()), Some(NbtPath::new().key("data"))) });
    f.push(Command::Schedule(Schedule::Function { function: FunctionRef::Id(rl("zoo:returns")), ticks: 20, mode: None }));
    f.push(Command::Schedule(Schedule::Function { function: FunctionRef::Id(rl("zoo:returns")), ticks: 40, mode: Some(ScheduleMode::Append) }));
    f.push(Command::Schedule(Schedule::Function { function: FunctionRef::Id(rl("zoo:returns")), ticks: 60, mode: Some(ScheduleMode::Replace) }));
    f.push(Command::Schedule(Schedule::Function { function: FunctionRef::Tag(rl("zoo:tagged")), ticks: 5, mode: None }));
    f.push(Command::Schedule(Schedule::Clear(rl("zoo:returns"))));
    f.push(Command::Return(Return::Fail));
    f.push(Command::return_value(1));
    f
}

fn returns_all() -> Function {
    let mut f = Function::new();
    f.push(Command::Return(Return::Run(Box::new(Command::call(rl("zoo:returns"))))));
    f.push(Command::Return(Return::Fail));
    f
}

fn mac() -> Function {
    let mut f = Function::new();
    f.push(Command::Macro(MacroCommand::new().text("say Hello ").var("who")));
    f.push(Command::execute().as_(Selector::a()).run(Command::Macro(MacroCommand::new().text("scoreboard players set @s pts ").var("n"))));
    f.push(Command::Return(Return::Run(Box::new(Command::Macro(MacroCommand::new().text("say ").var("who"))))));
    f
}

fn misc() -> Function {
    let mut f = Function::new();
    f.comment("tag, messages, world");
    let s = Selector::s;
    f.push(Command::Tag { targets: s(), action: TagAction::Add(word("t1").into()) });
    f.push(Command::Tag { targets: s(), action: TagAction::List });
    f.push(Command::Tag { targets: s(), action: TagAction::Remove(word("t1").into()) });
    f.push(Command::say("hello from the zoo"));
    f.push(Command::Summon { entity: rl("pig").into(), pos: None, nbt: None });
    f.push(Command::Summon { entity: rl("pig").into(), pos: Some(Pos::local(0.0, 0.0, 2.0)), nbt: Some(Compound::new().with("NoAI", true).with("Tags", vec![Nbt::str("zoo_dummy")])) });
    f.push(Command::Kill(Some(Selector::e().tag(word("zoo_dummy")).entity_type(rl("pig")))));
    for mode in [None, Some(SetblockMode::Destroy), Some(SetblockMode::Keep), Some(SetblockMode::Replace), Some(SetblockMode::Strict)] {
        f.push(Command::Setblock { pos: Pos::block(0, -60, 0), block: BlockState::new(rl("oak_stairs")).prop(word("facing"), word("north")), mode });
    }
    f.push(Command::Setblock { pos: Pos::rel(0.0, 1.0, 0.0), block: BlockState::new(rl("chest")).nbt(Compound::new().with("CustomName", "Zoo Chest")), mode: None });
    f.push(Command::Gamerule { rule: rl("advance_time"), value: None });
    f.push(Command::Gamerule { rule: rl("advance_time"), value: Some(GameRuleValue::Bool(true)) });
    f.push(Command::Gamerule { rule: rl("random_tick_speed"), value: Some(GameRuleValue::Int(3)) });
    f.push(Command::Raw("list".into()));
    for (seconds, amp, hide) in [(None, None, false), (Some(30), None, false), (Some(30), Some(2), false), (Some(30), Some(2), true)] {
        f.push(Command::Effect(Effect::Give { targets: Selector::a(), effect: rl("speed").into(), seconds: seconds.map(Num::Lit), amplifier: amp.map(Num::Lit), hide_particles: hide }));
    }
    f.push(Command::Effect(Effect::Clear { targets: None, effect: None }));
    f.push(Command::Effect(Effect::Clear { targets: Some(Selector::a()), effect: None }));
    f.push(Command::Effect(Effect::Clear { targets: Some(Selector::a()), effect: Some(rl("speed").into()) }));
    f.push(Command::Teleport { targets: Selector::s(), dest: Pos::rel(0.0, 1.0, 0.0) });
    f.push(Command::Teleport { targets: Selector::a(), dest: Pos::abs(0.0, 64.0, 0.0) });
    f.push(Command::Teleport { targets: Selector::s(), dest: Pos::local(0.0, 0.0, 2.0) });
    f
}

fn text() -> Function {
    let mut f = Function::new();
    f.comment("tellraw: every text component content/style");
    let all = Selector::a;
    let t = |x: Text| Command::tellraw(all(), x);
    f.push(t(Text::plain("plain")));
    f.push(t(Text::plain("styled \"quotes\" \\ back").color(NamedColor::Red).bold(true).italic(false)));
    f.push(t(Text::plain("hex").hex_color(0x12ab34)));
    f.push(t(Text::score(Selector::s(), obj())));
    f.push(t(Text::score(fake("#a"), obj())));
    f.push(t(Text::score(ScoreHolder::All, obj())));
    f.push(t(Text::selector(Selector::a())));
    f.push(t(Text::new(Content::Selector { selector: Selector::a(), separator: Some(Box::new(Text::plain(" | "))) })));
    f.push(t(Text::new(Content::Keybind("key.inventory".into()))));
    f.push(t(Text::new(Content::Translate { key: "chat.type.text".into(), fallback: Some("<%s> %s".into()), with: vec![Text::plain("a"), Text::plain("b")] })));
    f.push(t(Text::new(Content::Nbt { path: NbtPath::new().key("n"), source: DataTarget::Storage(rl("zoo:vars")), interpret: false })));
    f.push(t(Text::new(Content::Nbt { path: NbtPath::new().key("Pos").index(0), source: DataTarget::Entity(Selector::s()), interpret: false })));
    f.push(t(Text::new(Content::Nbt { path: NbtPath::new().key("x"), source: DataTarget::Block(Pos::here()), interpret: true })));
    f.push(t(Text::plain("click me")
        .on_click(ClickEvent::RunCommand(Box::new(Command::Scoreboard(Scoreboard::PlayersSet { targets: Selector::s().into(), objective: word("trig"), value: 1 }))))
        .on_hover(HoverEvent::ShowText(Box::new(Text::plain("hover"))))
        .then(Text::plain(" more").color(NamedColor::Aqua))
        .then(Text::plain(" url").on_click(ClickEvent::OpenUrl("https://example.com".into())))
        .then(Text::plain(" copy").on_click(ClickEvent::CopyToClipboard("copied".into())))
        .then(Text::plain(" suggest").on_click(ClickEvent::SuggestCommand(Box::new(Command::say("hi")))))));
    f
}

/// Commands with typed holes (`$(h0)`...): a macro function, only valid when called with arguments.
fn holes() -> Function {
    let mut f = Function::new();
    let h = |n: u16| Coord::Hole(Hole(n));
    f.comment("every kind of hole the command AST supports");
    f.push(Command::Teleport { targets: Selector::a(), dest: Pos::new(h(0), Coord::Abs(64.0), h(1)) });
    f.push(Command::Setblock { pos: Pos::new(h(0), h(1), h(0)), block: BlockState::new(rl("stone")), mode: None });
    f.push(Command::Summon { entity: rl("marker").into(), pos: Some(Pos::new(h(0), h(1), h(0))), nbt: Some(Compound::new().with("Tags", vec![Nbt::str("zoo_dummy")])) });
    f.push(Command::Effect(Effect::Give { targets: Selector::a(), effect: rl("speed").into(), seconds: Some(Num::Hole(Hole(0))), amplifier: Some(Num::Hole(Hole(1))), hide_particles: false }));
    let xs = |n: u16| NbtPath::new().key("xs").index_hole(Hole(n));
    f.push(Command::Data(Data::Modify {
        target: DataTarget::Storage(rl("zoo:vars")),
        path: xs(0),
        op: ModifyOp::Set,
        source: ModifySource::From { source: DataTarget::Storage(rl("zoo:vars")), path: Some(xs(1)) },
    }));
    f.push(Command::execute().clause(Clause::Positioned(Pos::new(h(0), Coord::Abs(64.0), h(1)))).clause(Clause::Rotated(Rotation { yaw: h(0), pitch: Coord::Abs(0.0) })).run(Command::say("held")));
    // text chosen at run time: ids, tag names, a say message and a joined string
    fn s<T>(n: u16) -> Arg<T> {
        Arg::Hole(Hole(n))
    }
    f.push(Command::Setblock { pos: Pos::block(0, 64, 0), block: BlockState::new(s(0)), mode: None });
    f.push(Command::Summon { entity: s(0), pos: Some(Pos::here()), nbt: Some(Compound::new().with("Tags", vec![Nbt::str("zoo_dummy")])) });
    f.push(Command::Tag { targets: Selector::s(), action: TagAction::Add(s(0)) });
    f.push(Command::Tag { targets: Selector::s(), action: TagAction::Remove(s(1)) });
    f.push(Command::Effect(Effect::Give { targets: Selector::a(), effect: s(0), seconds: None, amplifier: None, hide_particles: false }));
    f.push(Command::Effect(Effect::Clear { targets: Some(Selector::a()), effect: Some(s(0)) }));
    f.push(Command::execute().clause(Clause::Summon(s(0))).run(Command::say("summoned")));
    f.push(Command::Say(s(0)));
    f.push(Command::Data(Data::Modify {
        target: DataTarget::Storage(rl("zoo:vars")),
        path: NbtPath::new().key("joined"),
        op: ModifyOp::Set,
        source: ModifySource::Template(vec![TemplatePart::Lit("a \"quoted\" $(not a hole) ".into()), TemplatePart::Hole(Hole(0)), TemplatePart::Lit("-".into()), TemplatePart::Hole(Hole(1))]),
    }));
    f.push(Command::Kill(Some(Selector::entity_id(word("zoo_id"), Hole(0)))));
    f.push(Command::Teleport { targets: Selector::entity_id(word("zoo_id"), Hole(1)).tag(word("zoo_dummy")), dest: Pos::here() });
    f
}

fn main() {
    let out: PathBuf = std::env::args().nth(1).unwrap_or_else(|| "target/zoo".into()).into();
    let mut pack = DataPack::new("slab syntax zoo");
    let mut all = Function::new();
    all.comment("run every family; the game reports any line it cannot parse at load time");
    for (name, f) in [
        ("scoreboard", scoreboard()),
        ("execute", execute()),
        ("data", data()),
        ("flow", flow()),
        ("misc", misc()),
        ("text", text()),
    ] {
        let id = ResourceLocation::new("zoo", name).unwrap();
        all.push(Command::call(id.clone()));
        pack.add_function(id, f).unwrap();
    }
    all.push(Command::Function { function: FunctionRef::Id(rl("zoo:holes")), args: FnArgs::Inline(Compound::new().with("h0", 3).with("h1", 2)) });
    for (name, f) in [("returns", returns()), ("returns_all", returns_all()), ("mac", mac()), ("holes", holes())] {
        pack.add_function(ResourceLocation::new("zoo", name).unwrap(), f).unwrap();
    }
    pack.add_function(rl("zoo:all"), all).unwrap();
    let mut load = Function::new();
    load.push(Command::say("zoo loaded"));
    pack.add_function(rl("zoo:load"), load).unwrap();
    pack.on_load(rl("zoo:load"));
    pack.add_resource(
        "predicate",
        rl("zoo:never"),
        Json::object([("type", Json::str("minecraft:random_chance")), ("chance", Json::Float(0.0))]),
    )
    .unwrap();
    pack.add_tag("function", rl("zoo:tagged"), Tag::new().with(IdOrTag::Id(rl("zoo:returns")))).unwrap();

    match pack.write_dir(&out, true) {
        Ok(()) => println!("wrote {}", out.display()),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
