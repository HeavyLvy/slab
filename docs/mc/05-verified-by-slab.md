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

## Strings in macros (checked on the real server)

* A string tag substituted by `$(h0)` is pasted as its raw text (no quotes), so it works as a resource id (`summon $(h0)`, `setblock .. $(h0)`), a tag name (`tag @e[..] add $(h1)`) and inside a quoted template (`data modify storage <s> <path> set value "minecraft:$(h0)_block"`). An int tag substitutes as a plain decimal, so numbers and strings can be joined in one template.
* In the template, a literal `$(` must be written `\u0024(` in the quoted text, otherwise it is taken as a placeholder; `write_quoted` escapes it.
* **UNVERIFIED**: a string value containing `"` or `\` should break only the template command it lands in (the line fails to parse; macro parse failures are silent, see above). The e2e project does not test it.

## Equality, command outcomes, entity reads, macro selectors (probed on the real server, step 0 of docs/lang/plan-enums-results.md)

**NBT equality** (used by slab for `==` on strings, lists and structs; the e2e project checks it, including in `&&`/`||` and a `while` condition). Copy the left value (`data modify storage S t set from storage S a`), then `execute store success score X run data modify storage S t set from storage S b`: `X` is **0 when the two values are equal** (the `set` changes nothing and fails) and 1 when they differ. Checked: equal strings, ints, lists, nested compounds and the empty string give 0; `"abc"` vs `"abd"`, list length, list order, a different compound value and an extra key give 1; **int `1` vs byte `1b` gives 1** and int `1` vs string `"1"` gives 1 (different tags are different values); **compound key order does not matter**. Against a literal, `execute store success score X if data storage S {a:"abc"}` gives 1 for a match and 0 otherwise.

**Two `store` clauses on one `execute`.** `execute store success score OK store result score V run <cmd>` writes both, in either order. A failing command writes `OK = 0, V = 0` (both are written, they are not left alone): `data get storage S missing` gives 0/0. A command can succeed with result 0: `scoreboard players get #zero p` gives OK 1, V 0. So `OK` is the success flag and `V` the integer result, independently.

**Function calls forward both.** `execute store success score OK store result score V run function f` where `f` is `return run <cmd>` gives: `data get` of 5 -> 1/5; a command that succeeds with result 0 -> **1/0** (success is 1 even though the value is 0); a failing command -> 0/0. The macro form (`$return run data get storage S $(k)` called `with storage`) behaves the same. A function that never returns a value writes **nothing** (both scores keep their old value), so a helper used this way must always `return run`. A macro helper that does the two stores itself (`$execute store success score #ok p store result score #v p run data get storage S $(k)`) writes 0/0 when the command fails.

**`datapack disable "file/<name>"` from a function** (with `function-permission-level=2`, the default) works: the function keeps running to its end, and the pack is disabled by the reload that follows (`datapack list enabled` then shows only `vanilla`). The pack id is `file/<folder or zip name>`.

**Reading entity data** with `data modify storage S x set from entity @e[scores={obj=$(id)},limit=1] <path>` (a macro line) works for `Tags` (a list; **order is not the order given at summon**, and an entity with no tags has **no `Tags` path at all**, so the read fails) and `CustomName` (a **string** tag, `"Bob"`, when it was given as a plain string; a **compound** such as `{color: "red", text: "Styled"}` when it was given as a component; **absent** when the entity has no name). `data get entity ... Pos[0] 1` through `execute store result` gives the coordinate truncated to an int (8.5 -> 8, 62.78 -> 62). **The entity's NBT as seen by `data get entity` has no `id` field**, so the entity type cannot be read that way (test it with `execute if entity @e[...,type=$(k)]` instead). An id with no matching entity makes the macro line fail silently.

**Macro placeholders in selector arguments** all work and compare as expected, with values from a storage compound: `tag=$(h)`, `tag=!$(h)`, `distance=..$(h)`, `distance=$(h)..`, `scores={obj=$(h)..}`, `scores={obj=$(h)}`, `limit=$(h)`, `type=$(h)` (both `cow` and `minecraft:cow`), `type=!$(h)`, `name=$(h)` (plain string), `x=$(h)`. They combine in one selector.

## Harness behaviour worth knowing

* Commands inside functions give **no feedback** to the console, so runtime failures aren't visible; the harness mainly proves *parsing/loading*. To assert runtime behaviour, write functions that `execute store ... score` a result and print it with `say`/`tellraw`, or read it with `scoreboard players get`.
* A pack placed in `world/datapacks/` before first start is already enabled (`Pack 'file/<name>' is already enabled!`).

## Not yet verified

* NBT path with a root filter followed by a key (`{a:1}.b`) - rendered but never sent to the game.
* `Text` hover/click on non-chat contexts, `Content::Nbt` with `Block` source resolving at run time.

**`run_command` (e2e `command_test`).** `execute store success score OK store result score V run <cmd>` for user-written text works the same for a literal command and for a macro line that is the whole command (`$return run $(h0)` called `with storage`): `scoreboard players get` of 41 gives OK=1 V=41, of 0 gives OK=1 V=0, of a holder with no score gives OK=0 V=0. `time query daytime` is rejected at load (no such timeline in 26.3), so use `list` or another stable command in the zoo.

**`data get entity ... Pos[i]` floors.** Through `run_command` (`Entity::position`, e2e `r_pos_*`): a marker moved to `5.5 -20.25 9.5` gives `Pos[0]` **5**, `Pos[1]` **-21** (floor, not truncation toward zero), `Pos[2]` 9. Keep such tests inside the force-loaded chunk 0 0: an entity in a neighbouring chunk (x=-7) is not selectable and the test fails randomly.

**Reading and writing doubles with scoreboard ints (probe on 26.3, for the fixed-point `double`).**
* `data get <target> <path> <scale>` stores **floor(value * scale)** as an int: 6.5 -> 6, -6.5 -> -7 (scale 1), -0.1234 x 10000 -> -1234. It **saturates** at +2147483647 / -2147483648 (no wrap): 3.0E9 -> 2147483647, 29999984.5 x 100 -> 2147483647. The same for `execute store result`. Works on double, float and int tags.
* The product is a floating-point multiply, so the floor can be one too low: 0.57 x 100 -> 56, 0.57 x 10000 -> **5699**, 0.29 x 100000 -> 28999. About 6% of random 4-decimal values are off at x10000 (checked offline with the same IEEE arithmetic); reading at x100000 and computing `(n + 5) / 10` (floored) fixes all of them.
* The scale limits the range: x1 reaches +-2147483647 (the whole part is exact for the whole world, border 29999984), x10 +-214748364, x100 +-21474836, x1000 +-2147483, x10000 +-214748, x100000 +-21474, x1000000 +-2147.
* `execute as <entity> at @s run tp @s ~-29999984 ~ ~` (relative to the entity itself, **not** to the console, which runs at the world spawn) moves a scratch marker from x=29999984.5678 to 0.5678000003 exactly enough that the fraction reads normally (5678 at x10000, 567800 at x1000000): the fraction of a far coordinate is available at full precision this way, but only for loaded chunks and it costs a scratch entity. A chunk must be force-loaded to read an entity in it.
* `tp` takes decimal text: `-6.0005`, `-0.0005`, `7.123456789` are stored as exactly those doubles (`-5.0E-4d`), and `$(w).$(f)` through a macro works. A fraction needs its leading zeros (`.0005`, not `.5`), and a negative value is written with a sign and the absolute whole part (`-6.5`, not `-7` plus `.5`). Integer coordinates are centred (`7` -> 7.5). `tp` beyond the border fails with "Invalid position for teleport" (30000000); 29999999.5 works.

**`particle` with options (zoo).** The zoo loads `particle minecraft:item{item:{id:"minecraft:diamond",count:1}} ~ ~1 ~ 0.2 0.2 0.2 0.1 20 force @a`, `particle minecraft:dust{color:[1f,0f,0f],scale:1.5f} ~ ~ ~ 0 0 0 0 0 normal` and `particle minecraft:block{block_state:"minecraft:stone"}` without parse errors on 26.3 (slab writes a whole float as `1f`), so the SNBT options go right after the id with no space, and the order is `<pos> <delta> <speed> <count> [force|normal] [<viewers>]`.

**Rotation, motion and local movement (e2e `motion_world_test`, 26.3).**
* `rotate <target> <yaw> <pitch>` takes decimals (`-45.5 30.25`) and sets `Rotation:[yaw,pitch]f` as given: **yaw is not normalised** (`270` and `540` stay as they are), the **pitch is clamped** to -90..90 (`100` -> 90). `data get entity <e> Rotation[0] 100000` reads it (float tag, floor of the product: `-45.5` -> -4550000). `rotate <e> facing entity <other>` turns to the other entity's feet.
* `execute as <e> at @s run tp @s ^l ^u ^f` moves along the entity's own axes and keeps its rotation: at yaw 0 `^ ^ ^5` is +z (south); at yaw 90 (west) `^ ^ ^2` is -x and `^-1.5` (a negative left, so to the right) is -z. Decimal text with a sign works (`^-1.5000`, `~-1.0000`). A plain `tp(e, ^ ^ ^5)` is measured from the command's source, not from `e`.
* `data merge entity <e> {Motion:[0.1d,0.2d,-0.3d]}` sets the velocity of a marker and of a `NoAI` pig, and `data get entity <e> Motion[i] 100000` reads it back (`20000` for 0.2). For entities the game moves, `Motion` changes every tick. Players cannot be changed with `data merge entity` (the documented rule, not exercised here).

**`max_command_sequence_length` (examples/projects/limit).** `gamerule max_command_sequence_length` with no value reports `... is currently set to N`, and `execute store result score X run gamerule max_command_sequence_length` stores N. A rule changed in the middle of a function chain does **not** apply to that chain (a loop kept stopping at 65536 commands after the rule was set to 1,000,000 by its own first command) but does apply to the next chain. The functions of `#minecraft:load` are separate chains: a function listed after the one that raised the rule ran 60,000 loop trips (more than 180,000 commands), while the same loop without the rule was cut off after 13,105 trips. The cut-off prints `Command execution stopped due to limit (executed 65536 commands)` only on the console.
