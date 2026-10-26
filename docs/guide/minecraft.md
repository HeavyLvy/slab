# Talking to Minecraft

Everything that touches the world lives in the `game` module (`import game::*;`), and `std` has `print`. This page is the tour; the [standard library reference](../reference/std.md) lists every function.

## When code runs: entry points

A pack's functions run when something calls them. Three attributes make a function start by itself:

```slab
global seconds: int = 0;

#[load]                                  // when the pack loads and on every /reload
fn boot() {
    print("loaded");
}

#[tick]                                  // every game tick, 20 times a second
fn each_tick() {
    seconds += 0;
}

#[every(1s)]                             // every N ticks; `1s` is 20, a plain number is ticks
fn heartbeat() {
    seconds += 1;
}
```

Other functions run when these call them, or when you `/function <namespace>:<name>`.

## Selectors

Selectors are Minecraft's way to name entities, written the same way: `@a` (all players), `@p` (nearest player), `@e` (all entities), `@s` (the entity the code runs as), `@r`, `@n`, with options in brackets:

```slab
fn selectors(radius: int, tag_name: str) {
    kill(@e[type=minecraft:zombie, distance=..10, tag=!boss]);
    say("hi");
    let near = @e[tag=$tag_name, distance=..${radius * 2}, limit=1];   // `$name` and `${expression}`
    if exists(near) { print("something is near"); }
}
```

* Inside `@e[` the editor completes the options and explains them.
* `$name` or `${expression}` puts a variable into an option; slab uses a macro function for it.
* A selector that can only match one entity (`@s`, `@p`, `@r`, `@n`, or `limit=1`) can be stored in a variable as an `Entity`.

## Context blocks: `as`, `at`, `be`, …

These are `/execute` written as blocks. The block runs once for every entity the selector matches:

```slab
fn contexts() {
    as @a[tag=ready] at @s {                      // as each player, at where they are
        effect::give(@s, "minecraft:speed", 10s, 0);
        tp(@s, ~, ~1, ~);                         // ~ is relative to where the block runs
    }
    be @e[type=minecraft:zombie] {                // `be e` is `as e at @s` in one word
        say("braaains");
    }
    in "minecraft:the_nether" {                   // in another dimension
        positioned 0 64 0 {
            setblock(~, ~, ~, "minecraft:stone");
        }
    }
    as @e[tag=ring] at @p rotated as @s {         // `^` follows the facing of the entity after `rotated as`
        tp(@s, ^, ^, ^5);
    }
}
```

The clauses are `as`, `at`, `be`, `in`, `positioned x y z`, `rotated yaw pitch` / `rotated as e`, `facing x y z`, `align xz`, `anchored eyes`, and they chain: `as @a at @s positioned ~ ~2 ~ { ... }`. Inside a block `@s` is the entity it runs as. Because a block may run once per entity, `return` is not allowed inside one.

To visit entities one by one with a name for each:

```slab
fn each_zombie() {
    for z in @e[type=minecraft:zombie] {          // the body runs as each zombie; `z` names it
        effect::give(z, "minecraft:glowing", 5s, 0);
    }
}
```

## Entities as values

`summon` gives you an `Entity` you can keep:

```slab
fn entities() {
    let cow = summon("minecraft:cow", 0, 70, 0);   // returns a handle to it
    tag::add(cow, "pet");
    tp(cow, 5, 70, 5);
    if cow.is_alive() {
        let spot = cow.position();                  // Result<Vec3i, ()>: Err when it is gone
        if let Ok(p) = spot { print(f"the cow is at {p.x} {p.y} {p.z}"); }
    }
    cow.set_rotation(90, 0);                        // yaw and pitch, in degrees
    cow.move_local(0, 0, 3);                        // 3 blocks ahead, along where it faces
    kill(cow);
}
```

A handle stays valid as long as the entity lives; after it dies every action on it does nothing and reports failure. [Entities in depth](../lang/entities.md) explains how handles work, rotation, velocity and local (`^`) movement, and [numbers with decimals](../lang/doubles.md) the `NearDouble`/`FarDouble` types behind positions with decimals.

Commands that act on the world return a `bool` (did it work?) which you can ignore: `kill(@e[type=zombie]);` costs nothing extra.

## Scoreboards

```slab
objective kills = "playerKillCount";             // declares a scoreboard objective

fn score_board() {
    kills[@s] += 1;                              // a score of an entity
    kills["#total"] = 0;                         // or of a fake player
    if kills[@s] >= 10 { say("ten kills!"); }
}
```

## Blocks, effects, particles, other commands

```slab
fn world() {
    setblock(0, 64, 0, "minecraft:diamond_block");
    effect::give(@a, "minecraft:regeneration", 10s, 1);
    effect::clear(@a);
    particle("minecraft:campfire_signal_smoke", ~, ~1, ~);
    // a particle with data, a spread, a count and a mode
    particle("item", { item: { id: "minecraft:diamond", count: 1 } }, ~, ~1, ~, 0.2, 0.2, 0.2, 0.1, 20, "force");
}

fn anything_else() {
    run_command("time set day");                                 // any command, as text
    let result = run_command("scoreboard players get #score pack");   // Result<int, ()>: the command's result
    if result.is_ok() { print("it worked"); }
}
```

`run_command` is the escape hatch for anything slab has no function for yet. Its text may be computed (`run_command(f"say {name}")`).

## Long jobs

If a function needs far more than the game's 65,536 commands per tick, make it a [`task fn`](../lang/tasks.md): its loops run a slice per tick. And the pack tells you when it happens anyway: a function that was cut off by the limit reports it in chat.

## Cleaning up

Everything a pack creates is removed by `/function <namespace>:_uninstall`: its scoreboards, its storage, the entities it summoned and the tags it added.
