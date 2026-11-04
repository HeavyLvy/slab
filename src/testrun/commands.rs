//! The console commands `slab test` gives the server, built from the typed commands of
//! `datapack` (nothing here writes Minecraft syntax by hand).

use crate::datapack::*;

/// The test area: a 17 x 17 square around the origin on the flat world's surface.
const ARENA: i32 = 8;
// a fill may touch 32,768 blocks at most (`max_block_modifications`)
const _: () = assert!(((2 * ARENA + 1) * (2 * ARENA + 1) * 64) <= 32_768);

/// The flat world's grass is at this height, so things stand at one block more.
pub const GROUND: i32 = -61;

fn render(c: Command) -> String {
    debug_assert!(c.check().is_ok(), "{c}");
    c.to_string()
}

/// Rules that keep the world still, and the chunks around the origin loaded (a server without
/// players loads nothing by itself).
pub fn setup() -> Vec<String> {
    let rule = |name: &str| Command::Gamerule { rule: rl(name), value: Some(GameRuleValue::Bool(false)) };
    vec![
        render(rule("advance_time")),
        render(rule("advance_weather")),
        render(rule("spawn_mobs")),
        render(Command::Forceload(Forceload::Add(ColumnPos::block(-16, -16), Some(ColumnPos::block(15, 15))))),
    ]
}

/// Says `ready` once the middle of the arena is loaded.
pub fn ready_probe(word: &str) -> String {
    render(Execute::new().if_(Condition::Loaded(Pos::block(0, GROUND + 1, 0))).run(Command::say(word)))
}

/// Everything in the arena back to the empty flat world: air above the floor, grass and dirt
/// below, and no entities (twice, because killing mobs drops items).
pub fn clean_arena() -> Vec<String> {
    let fill = |y0: i32, y1: i32, block: &str| render(Command::Fill { from: Pos::block(-ARENA, y0, -ARENA), to: Pos::block(ARENA, y1, ARENA), block: BlockState::new(rl(block)), mode: None });
    let not_players = Selector::e().arg(SelectorArg::Type { id: IdOrTag::Id(rl("player")), negated: true });
    vec![fill(GROUND + 1, GROUND + 64, "air"), fill(GROUND, GROUND, "grass_block"), fill(GROUND - 1, GROUND - 1, "dirt"), render(Command::Kill(Some(not_players.clone()))), render(Command::Kill(Some(not_players)))]
}

/// `function <id>`
pub fn call(function: &ResourceLocation) -> String {
    render(Command::call(function.clone()))
}

/// `schedule clear <id>`: a test that timed out must not go on in the next one.
pub fn clear_schedule(function: &ResourceLocation) -> String {
    render(Command::Schedule(Schedule::Clear(function.clone())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_are_what_the_server_expects() {
        let s = setup();
        assert_eq!(s[0], "gamerule minecraft:advance_time false");
        assert_eq!(s[3], "forceload add -16 -16 15 15");
        assert_eq!(ready_probe("hi"), "execute if loaded 0 -60 0 run say hi");
        let c = clean_arena();
        assert_eq!(c[0], "fill -8 -60 -8 8 3 8 minecraft:air");
        assert_eq!(c[1], "fill -8 -61 -8 8 -61 8 minecraft:grass_block");
        assert_eq!(c[3], "kill @e[type=!minecraft:player]");
    }
}
