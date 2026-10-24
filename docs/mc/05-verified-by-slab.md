# 05 - Facts verified by slab against a real 26.3 server

Produced by `cargo run --example zoo -- target/zoo` + `tools/verify_pack.py target/zoo --run zoo:all`
(real dedicated server, official 26.3 `server.jar`). Anything here outranks the other docs.
Add to this file whenever the harness teaches us something.

## The game rejected these (parse/load errors)

| Output | Error | Consequence for the generator |
|---|---|---|
| `schedule clear #ns:tag` | `Expected whitespace to end one argument` | `schedule clear` takes a plain resource location. `Schedule::Clear` holds a `ResourceLocation`. |
| `execute if predicate ns:p` with no such predicate | `Can't find element 'ns:p' in registry 'minecraft:predicate'` and the **whole function fails to load** | Predicate must exist as a pack resource at load time. `DataPack::validate` checks this. |
| `execute if block ~ ~ ~ stone[x=y]` | `Block minecraft:stone does not have property 'x'` | Block property names/values are validated against the block registry at parse time. slab doesn't know the registry yet, so a bad property is only caught by the game. |

## The game accepted these (renderings confirmed)

* Every `scoreboard` form in `Scoreboard`: objectives add/list/modify/remove/setdisplay (incl. `sidebar.team.dark_purple`, `below_name`), players set/add/remove/reset/get/enable/list/operation (all nine operators).
* Every `execute` clause: `as at positioned [as|over] rotated [as] facing [entity] align anchored in on summon store if unless`, including all `on` relations, all six `store ... <type> <scale>` NBT types with scale `0.5`, bossbar store, and all conditions in both polarities (`biome block blocks data dimension entity function loaded predicate score`).
* Flattened `execute ... run execute ...` chains are not needed, but a flat chain was accepted.
* `data get|merge|remove|modify` with block/entity/storage targets and `value|from|string|compute` sources, `append|prepend|insert N|insert -1|set|merge`, NBT paths with `[]`, `[-1]`, `[{filter}]`, root filter `{k:v}`, quoted keys with dots and quotes.
* `data modify ... compute default|entity <selector> integer|float {type:"minecraft:constant",value:...}`.
* `function` with inline compound, `with storage <id> [path]`, `with entity <sel> <path>`; `#tag` function refs.
* `schedule function` with and without `append|replace`, on ids and `#tags`.
* `return <n>`, `return fail`, `return run function ...`.
* `$` macro lines, including `$execute ... run ...` and `return run` of a macro line; inline-compound call substitutes (`Hello Inline`).
* `gamerule minecraft:advance_time false` (namespaced) and bare queries.
* `setblock` with all modes (`destroy keep replace strict`), block-state props and block-entity NBT.
* `tellraw` with: plain string shorthand, styled compounds (`bold:1b`, hex colors `"#12ab34"`), `score`, `selector` (+separator), `keybind`, `translate` (+fallback, `with`), `nbt` (`storage`/`entity`/`block` keys without an explicit `source`/`type`, `interpret:1b`), `click_event` (`run_command` with **no leading slash**, `open_url`, `copy_to_clipboard`, `suggest_command`), `hover_event` `show_text`, and `extra`.
* Pack layout produced by `DataPack::write_dir`: `pack.mcmeta` with `"min_format": [121,0]` and `"max_format": 121`, `data/<ns>/function/*.mcfunction`, `data/<ns>/tags/function/*.json`, `data/<ns>/predicate/*.json` (26.3 `random_chance` with `"chance": 0.0`).

## IR programs run on the server (`cargo run --example programs`)

Results read back with `scoreboard players get` / `data get` (`--checks`), all 24 matched:

* `execute <tests> if function <blk> run return 1` propagates a `return 1` from a nested function, including out of a `while` inside an `if`. A function that just falls off the end (void) or does `return 0` does **not** continue the chain, as documented.
* `return run function <self>` works as a loop (tail call). 5000 iterations is fine.
* **Per-tick command limit:** a loop of 100000 iterations stops with the console message `Command execution stopped due to limit (executed 65536 commands)` and nothing after it runs (a following `scoreboard players operation` and `tellraw` did not execute). `gamerule max_command_sequence_length` reads `65536`.
* Scoreboard `/=` and `%=` are floored: `-7/2 = -4`, `-7%2 = 1`, `7/-2 = -4` (matches the docs).
* `execute store success score <h> <obj> if <test>` is a single command that materialises a condition as 0/1.
* `data modify storage <id> <list> append value 0` creates the list when it doesn't exist (the recursion stack relies on it); `data modify ... append from storage ...` with `[-1]` index paths and `execute store result storage <id> <list>[-1] int 1 run scoreboard players get ...` work for the save/restore pattern.
* `data get storage <id> <list>` on a list returns its length (used for `Len`); `execute store result score <h> <obj> run data get storage <id> <path>` converts a numeric tag to a score.
* Storage holds `["ann", "bob", 42]` (strings and an int in one list) after `append value "ann"` / `append from` of an int tag: heterogeneous lists work as documented for 1.21.5+.
* `scoreboard players get` output format: `#g.x has 55 [prog]`; `data get storage` output: `Storage prog:globals has the following contents: "hello"`.

## `execute store` with several conditions (found by the e2e project)

* `execute store success score X if A if B`: when `A` fails the chain ends **before** reaching the store and X is **not written** (it keeps its old value). Only a failing *last* condition stores 0. A loop condition materialised this way never exited because the flag kept its stale 1. slab now clears the flag first whenever there is more than one test (`Lower::store_success`).

## Running functions from the console (found by the e2e project)

* A function run from the server console executes at the **world spawn point**, which differs per world. `summon` there works ("Summoned new Marker") but `@e[...]` only finds entities in *loaded, entity-ticking* chunks, so a test that summons "here" is flaky. Summon at explicit coordinates and `forceload add 0 0` first (`verify_pack.py --setup "forceload add 0 0"` polls `execute if loaded 0 64 0` until the chunk is ready). Real programs running on players are not affected.
* `#[every(10)]` and `#[tick]` (a `#minecraft:tick` function, plus a counter function) run on a headless server with nobody online: 74 ticks and 7 triggers within ~2 s.
* Pack scoreboard/storage layout produced by `slab build`: `#g.<global>` holders, `<ns>:globals`, `<ns>:vars`, `<ns>:ret`, `<ns>:stack` (checked with `scoreboard players get` / `data get storage`).

## Cleanup and `_uninstall` (checked on the real server)

* `data get storage <id> <missing path>` -> `Found no elements matching <path>`: how a removed local shows up. `data remove storage` of a path that does not exist inside a function is silent (no error, no abort), so cleanup can run on slots that were never set.
* Removing every key of a storage leaves `data get storage <id>` printing `Storage <id> has the following contents: {}`.
* After `scoreboard objectives remove` for every objective, `scoreboard objectives list` prints `There are no objectives`.
* `execute if entity @e[tag=x]` -> `Test passed. Count: N` / `Test failed`; `tag @e[tag=x] remove x` removes the tag from entities that are not otherwise touched (they survive; verified with a marker the program did not summon).
* `summon minecraft:marker x y z {Tags:["ns.owned"]}` applies the tag at creation, so `kill @e[tag=ns.owned]` finds it.

## Macros (found by running slab output)

* A function containing `$` lines runs when called `function <f> with storage <id>`; the storage's keys fill `$(key)`. slab stores integers as int tags under `h0`, `h1`, ... in `<ns>:macro` and substitution gives plain decimals, including negatives (`xs[-1]`). Verified in coordinates (`$teleport @s $(h0) 64 $(h1)`, `$setblock ...`, `$summon ... {Tags:[...]}`), effect arguments (`$effect give @s minecraft:speed $(h0) $(h1)`), NBT path indices (`f.v_xs[$(h0)]` on both sides of `data modify`), and in the middle of an `execute` chain (`$execute positioned $(h0) 64 $(h1) run function ...`).
* Substitution happens (and the line is parsed) when the macro function is *called*, so a syntax mistake in a macro line only shows up at run time as `Failed to instantiate function`, never at `/reload`. The syntax zoo calls its macro function for that reason.
* Whole-number coordinates are block-centered for entities: `teleport @s 8 70 8` puts the entity at `8.5 70.0 8.5`.
* `data modify ... set value 5` and `set from storage ...` both work with a hole index on the left side.

## Command success as a value (checked on the real server)

* `execute store success score X run <command>` stores 1 if the command worked and 0 if it failed, for `teleport`, `tag`, `kill` and `setblock`: no matching entity -> 0; `tag add` of a tag the entity already has -> 0 (first time -> 1); `setblock` of the block that is already there -> 0 (a changed block -> 1).
* A macro function whose only line is `$return run teleport @s $(h0) 64 2`, called as `execute store success score X run function <f> with storage <id>`, stores the teleport's success (1 with a target, 0 without), so `return run` forwards the inner command's success through a macro call.

## Entity ids and single-target arguments (found by running slab output)

* `scoreboard players get @e[scores={obj=7}] t` is a **parse error**: `Only one entity is allowed, but the provided selector allows more than one`. `score_holder{amount=single}` arguments (`get`, `if score`, ...) need a selector that is provably single, so the id selector carries `limit=1` (`@e[scores={obj=7},limit=1]` works). `set`/`add`/`operation` accept the multi-entity form.
* A macro line that fails to parse at call time is **silent** inside a function: no `Failed to instantiate` in the log and no abort, the command just doesn't run. Only the result (here a score reading 0) reveals it, so selectors built into macro lines must be valid for the argument they are used in.
* `execute summon <type> run function <f>` runs `<f>` as the new entity; `scoreboard players operation @s <obj> = #counter <main>` inside it gives that entity an id score. Scores on non-player entities are keyed by UUID and disappear with the entity.

## Harness behaviour worth knowing

* Commands inside functions give **no feedback** to the console, so runtime failures aren't visible; the harness mainly proves *parsing/loading*. To assert runtime behaviour, write functions that `execute store ... score` a result and print it with `say`/`tellraw`, or read it with `scoreboard players get`.
* A pack placed in `world/datapacks/` before first start is already enabled (`Pack 'file/<name>' is already enabled!`).

## Not yet verified

* NBT path with a root filter followed by a key (`{a:1}.b`) - rendered but never sent to the game.
* `Text` hover/click on non-chat contexts, `Content::Nbt` with `Block` source resolving at run time.
