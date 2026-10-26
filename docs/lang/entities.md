# Entity values

A variable can hold an entity and act on it:

```slab
import std::print;
import game::*;

objective points = "dummy";

fn fight() {
    let boss = summon("minecraft:zombie", 0, 64, 0);   // a handle to the new entity
    let me   = @p;                                      // the single entity a selector matches
    if !tp(boss, 0, 70, 0) { print("boss is gone"); }   // acting on a dead entity fails, catchably
    effect::give(boss, "minecraft:glowing", 10s);
    as boss { print("running as the boss"); }
    points[boss] += 5;
    if exists(boss) && boss != me { kill(boss); }
}
```

## The type

`game::Entity` (written `Entity` after `import game::Entity;` or `import game::*;`) is a storable value type like `int`: it can be a local, a parameter, a return value, a struct field or a list element (`list<Entity>`), and it can be compared with `==` / `!=` (same entity or not). It is **not** the same thing as a selector: `@e[type=zombie]` is a *query* that may match many entities and can only be written where the language expects one; an `Entity` is *one specific entity*.

## Getting one

| Expression | Result |
|---|---|
| `summon("id", x, y, z)` / `summon("id")` | a handle to the new entity (`0`-valued if the summon failed, so `exists` is false). The entity is marked as ours and removed by `_uninstall` |
| a selector that matches at most one entity: `@s @p @r @n`, or any selector with `limit=1` | converted to a handle wherever an `Entity` is expected (`let me = @p;`, an `Entity` parameter, ...). Gives the same handle every time for the same entity. A selector that can match several (`@a`, `@e[...]`) is a compile error |

Use `as` to act on all of several entities at once, or `for` to go through them one at a
time (below).

## Iterating a selector

`for` accepts any selector as the thing it iterates:

```slab
for z in @e[tag=bosses] {
    tp(z, 0, 90, 0);        // `z` is one entity: the one this run is executing as
    pts[z] += 1;
    tag::add(z, "visited");
}
```

The body is a context block (`as `selector` { ... }`) run once per match; inside it the loop
name is **the current entity as a handle** — an `Entity` value you can store, pass, compare
and act on, exactly like `@s` with an id. Use `_` as the name when the body doesn't need it
(`for _ in @e[tag=x] { kill(@s); }`).

Because the body runs like a context block:

* `break`/`continue` are errors in it (there is no enclosing compiled loop);
* assigning to the loop name is an error (it is the entity being visited, not a variable);
* the order the matches are visited in is Minecraft's (`sort` when you need one).

## Using one

Everywhere a selector is accepted, an `Entity` is too: `kill`, `tp`, `tag::add/remove/has`, `effect::give/clear`, `exists`, `as`/`at` context clauses, and as the holder in `objective[holder]`. These are the world-acting builtins, so they return `bool` and a dead or missing target is just a `false` you can test.

## How it works

A handle is an `int` id. The first time an entity becomes a handle it is given the next id from a global counter, stored as a score in the objective `<ns>_id` on the entity itself. Acting on a handle is addressing `@e[scores={<ns>_id=N},limit=1]` with `N` filled in at run time through the macro mechanism (`docs/lang/codegen.md`), so each use costs a macro call and a search for that score.

* ids start at 1 and are never reused (the counter survives `/reload`), so a stale handle never points at a different entity; `0` means "no entity";
* the id score disappears with the entity (non-player scores are keyed by UUID), so a dead entity's handle matches nothing: `exists(e)` is false and actions return `false`;
* players keep their id across logins;
* the helper functions `_alloc_id`, `_new_entity` and `_ensure_id` and the objective are only generated when a program uses entity values.

## Lifetime

A handle is a plain integer: copying it copies the reference, and nothing happens when it goes out of scope, since entities are meant to persist in the world. Entities the program summoned are tagged `<ns>.owned`, and `<ns>:_uninstall` kills them and removes the id objective. Entities that already existed (players, mobs found with a selector) are only numbered, never killed. If you want an entity gone, `kill` it.

## Limits

* an entity must be loaded to be found (the same as any selector);
* each use searches all loaded entities for the id score, like any selector;
* reading entity data (`Health`, `Pos`, ...) is not available yet; it needs floats;
* a selector with a variable part (`@e[distance=..r]`) is not supported.

## Reading an entity: the methods of `Entity`

There is one type, `Entity`, and every value of it has the same methods: a variable that holds an entity is a handle (an id), and each method looks the entity up when it is called, so it always sees the entity as it is *now*, and a method on a dead entity returns `Err` or `false`. The methods are ordinary slab code in `game.slab` (ctrl+click the names in the editor) over [`run_command`](stdlib.md), in one `impl Entity` block:

| | |
|---|---|
| `e.selector() -> str` | the text of the selector that finds it, `@e[scores={<ns>_id=7},limit=1]` (a compiler intrinsic: it only needs the namespace) |
| `e.is_alive() -> bool` | `exists(e)` |
| `e.position() -> Result<Vec3i, ()>` | three `run_command(f"data get entity {sel} Pos[0]")` (`[1]`, `[2]`), rounded **down** to whole blocks; `Err(())` when the entity is gone |
| `e.position_near()`, `e.position_far()` | the position with decimals (`docs/lang/doubles.md`) |
| `e.teleport_near(p)`, `e.teleport_far(p)` | move it to a position with decimals |

```slab
let cow = summon("minecraft:cow", 4, 70, -6);
let p = cow.position();          // Ok(Vec3i { x: 4, y: 70, z: -6 }): summon centres x and z (4.5, -5.5) and the position is rounded down
if p.is_ok() { print(p.unwrap().x); }
```

There is no separate snapshot type: copy the numbers you need out of the result (`Vec3i`, `NearVec3`, ...). Anything else can be written the same way in your own code: `run_command(f"data get entity {e.selector()} Health")`. Reading names and tags (strings and lists) needs a few more compiler intrinsics and is not done yet.
