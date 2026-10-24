# 02 - Core "programming-relevant" commands (Minecraft Java 26.3, data pack format 121.0)

Target: Minecraft Java **26.3** "Wilderness Bound" (released 2026-09-15; `/version` on the real server prints `id = 26.3`, `data = 5023`, `protocol = 777`, `pack_resource = 97.1`, `pack_data = 121.0`).

## 0. How this document was produced / confidence legend

Three kinds of evidence were combined:

* **[G]** = ground truth generated from the official **26.3 `server.jar`** (sha1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`, downloaded from `piston-data.mojang.com`, run on Java 27 with `java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports`). The `reports/commands.json` (the full Brigadier tree, with argument parsers, numeric limits and **permission requirements**) and `reports/registries.json` (game rules, argument-type registry, number-provider types ...) are authoritative for every syntax tree and permission level in this file.
* **[V]** = verified by actually running the 26.3 dedicated server (offline mode, console as sender, `forceload`-ed chunk, a throw-away data pack) and observing command output / stored values. Console = permission "owners" (4), no executor entity, no player online. Things that need a real player (`/trigger`, `/title` output, `@p`, `@s` as player, `data ... entity @s` on players) could NOT be exercised and are marked as wiki-sourced.
* **[W]** = taken from minecraft.wiki raw wikitext (`https://minecraft.wiki/index.php?title=<Page>&action=raw`), which is current for 26.3 for most pages but stale for some (stale spots are called out). Pages used: `Commands`, `Commands/<each command>`, `Argument_types`, `Target_selectors`, `Coordinates`, `NBT_path`, `NBT_format`, `Text_component_format`, `Game_rule`, `Permission_level`, `Command_context`, `Function_(Java_Edition)`, `Scoreboard`, `Number_provider`, `Slot_sources`, `Java_Edition_26.1`, `_26.2`, `_26.3`, `_1.21.11`, `_1.21.9`.
* Anything not confirmed by any of the above is flagged **UNVERIFIED**.

In the auto-generated trees `m:` abbreviates `minecraft:` and `b:` abbreviates `brigadier:`.

Notation for syntax trees: `<name:parser{props}>` = argument node; bare word = literal; `$` = the node can end the command (executable); `=>execute` = after this node the parser restarts at the `execute` node (redirect, i.e. more sub-commands may follow).

Terminology used below (from the wiki's `Commands#Output`):
* Every command run yields an integer **`result`** and a 0/1 **`success`** (what `execute store result|success` captures) plus a human chat message.
* A command can be **unparseable** (syntax error, nothing runs), **fail** (runs, throws a CommandSyntaxException: success=0 result=0, red message), **succeed**, be **void** (only `/function`), be **terminated** (only `/execute`: branch count dropped to 0), or hit an **error** (non-Brigadier exception).
* `execute store` only stores when the branch ran to the end ("fully executed"); a failing command stores `0` (both result and success) - verified, see 3.1.

---

## 1. Global semantics a code generator must know

### 1.1 Permission levels in 26.3 [G]

The permission mechanism is still **five named levels**, but they are now expressed in the command tree as `permissions: {type: "minecraft:require", permission: {type: "minecraft:command_level", level: "<name>"}}` ([G] commands.json). Names -> numeric levels (the wiki and `server.properties` still use numbers): `all`=0, `moderators`=1 (**UNVERIFIED name**, not used by any vanilla command node), `gamemasters`=2, `admins`=3, `owners`=4.

Only three distinct requirements appear in the whole 26.3 command tree: `gamemasters` (66 nodes), `admins` (14), `owners` (9). Everything else has no requirement (level 0).

| Level | Commands (top-level node requirement, [G]) |
|---|---|
| none (0) | `help`, `list`, `me`, `msg`/`tell`/`w`, `teammsg`/`tm`, `trigger`, `random value|roll <range>` (WITHOUT sequence argument) |
| gamemasters (2) | `advancement attribute bossbar clear clone compute damage data datapack defaultgamemode dialog difficulty effect enchant execute experience/xp fetchprofile fill fillbiome forceload function gamemode gamerule give item kill locate loot particle place playsound posteffect recipe reload return ride rotate say schedule scoreboard seed setblock setworldspawn spawnpoint spectate spreadplayers stopsound stopwatch summon swing tag team teleport/tp tellraw test time title version waypoint weather worldborder`, plus `random reset ...` and `random value|roll <range> <sequence>` |
| admins (3) | `ban ban-ip banlist debug (incl. debug function) deop kick op pardon pardon-ip setidletimeout tick transfer whitelist` |
| owners (4) | `datapack create`, `jfr`, `perf`, `publish`, `unpublish`, `save-all`, `save-off`, `save-on`, `stop` |

Notes:
* `seed` is `gamemasters` in the tree [G]; the wiki says "0 in singleplayer" (integrated-server special case, **UNVERIFIED**).
* `/execute` itself is gamemasters (2). **`execute` cannot raise or lower the permission level** [W Command_context]; the permission is a property of the command source and is inherited unchanged.
* **Function context** [V][W]: commands inside a `.mcfunction` are parsed/executed with the *function permission level* = `function-permission-level` in `server.properties` (default **2**, range 1-4) capped to the caller's level ("limited to 2 or less" per wiki). Verified: a function containing `tick query` (admins, 3) **fails to load at `/reload`** ("Failed to load function t:needs3 ... Whilst parsing command on line 1: Unknown or incomplete command") while a function containing `gamerule pvp true` runs. Consequence: a whole function file is rejected if ANY non-macro line needs more than the function permission level; macro lines (`$...`) are parsed at call time.
* Functions called from `#minecraft:load`/`#minecraft:tick`, advancements, `schedule`: run as the server (no executor) at world spawn in the overworld (schedule) - see 4.3.
* `/trigger` and `/me /msg /teammsg` are the only commands vanilla lets level-0 players run; `/trigger` is how data packs take input from non-ops.
* Cheat toggle: in Java "Allow Commands" only affects the permission level the host/players receive (4 or 0); it is not a separate gate.

Source: https://minecraft.wiki/w/Permission_level , https://minecraft.wiki/w/Command_context , [G] commands.json. The wiki `Permission_level` page does not describe the rework (numbers only); I found no 1.21.11-26.3 changelog entry for it. **UNVERIFIED**: whether `ops.json` / `op-permission-level` semantics changed (they appear unchanged on the wiki).

### 1.2 Execution context (command source) [W Command_context]

Eight parameters: permission level, executor name, executor entity (may be absent: console, command block, `schedule`, `#minecraft:tick`...), dimension, position, rotation, anchor (feet/eyes), output callback. All are **static snapshots** in Java (changing an entity later does not change the context of the remaining commands of a function). `execute` sub-commands modify them for the *next command only*; they do not carry to following lines of a function. A function inherits the context (all but the output callback) of whoever invoked it.

Java 1.21.9+: server console / command-block-less commands run in the **respawn dimension** instead of always overworld [W 1.21.9].

### 1.3 Limits during a tick [V][W]

* `max_command_sequence_length` (gamerule, default 65536, min 0) - max commands one *execution source* may run in one tick (functions, `execute` forks...). Exceeding it prints `Command execution stopped due to limit (executed N commands)` and silently stops (no error value). Verified: a self-recursing function stopped after exactly 65536 commands (21845 loops of 3 commands) and after 100 when the rule was 100.
* `max_command_forks` (gamerule, default 65536, **min 0** [G], wiki says 1) - max number of contexts (branches) `execute` forks can create in a tick.
* Functions: macros `$(key)`; commands in a function run in the same tick; line continuation with a trailing `\` (verified: `say a \` + ` b` -> `say a b`, leading/trailing whitespace of the next line stripped); lines starting `#` are comments; blank lines ignored.

### 1.4 Success/result summary of the commands in this file

Every section below lists the per-command `result` (value stored by `store result`) and `success`. Quick cheat-sheet of the ones a compiler relies on:

| Command | success | result |
|---|---|---|
| `scoreboard players get` | 1 | the score |
| `scoreboard players set <t> <o> <n>` | 1 | number_of_targets * n (so `set ... 0` gives result 0 [V]) |
| `scoreboard players add|remove` | 1 | sum of the targets' new scores |
| `scoreboard players operation` | 1 | sum of the target scores after the op |
| `scoreboard players reset|enable|list` | 1 | #targets / #newly-enabled / #objectives of holder |
| `data get ... [path]` | 1 | numeric: floor(value) (or floor(value*scale)); list/array: length; string: length; compound: number of keys [W+V] |
| `data modify/merge/remove` | 1 | 1 (modify append/insert/prepend: #lists changed; set: #targets modified) [W] |
| `function` | see 4.1 | return value of function(s), void if none returned |
| `return` | 1 | the value |
| `random value|roll` | 1 | the number drawn |
| `execute if|unless` (at end) | 1 | see 3.1 table |
| `tag add|remove` | 1 | #entities changed |
| `kill`, `tellraw`, `title`, `msg`, `team join|leave`, `swing`, `posteffect` | 1 | #targets (tellraw/title/msg: #players) |
| `say`, `me`, `rotate`, `version`, `team modify prefix|suffix` | 1 | 1 |
| `time set|add` | 1 | **total elapsed ticks of the clock** (changed in 26.1) |
| `time query ...` | 1 | queried value |
| `gamerule <rule>` / `gamerule <rule> <v>` | 1 | current value (bool as 0/1) [W] |
| `difficulty` (query) | 1 | 0..3; `difficulty <x>`: 0 |
| `forceload add|remove|query` | 1 | #chunks added / removed / total |
| `schedule function` | 1 | game tick it will run at (mod 2^31-1) |
| `schedule clear` | 1 | #schedules cleared |
| `tick query|rate|freeze|unfreeze|step|sprint` | 1 | target rate / rate / **1** / **0** / 1 / 1 [V for freeze/unfreeze/rate] |
| `compute` | 1 | computed value (see 8.1) |
| `seed` | 1 | seed cast to int32 |

---

## 2. Argument types reference

Registry `minecraft:command_argument_type` in 26.3 has exactly these 59 entries [G]:
`brigadier:{bool,double,float,integer,long,string}` and `minecraft:{angle, block_pos, block_predicate, block_state, column_pos, component, context_float_provider, context_int_provider, dialog, dimension, entity, entity_anchor, feature, float_range, function, game_profile, gamemode, heightmap, hex_color, int_range, item_predicate, item_slot, item_slots, item_stack, loot_modifier, loot_predicate, loot_table, message, nbt_compound_tag, nbt_path, nbt_tag, objective, objective_criteria, operation, particle, resource, resource_key, resource_location, resource_or_tag, resource_or_tag_key, resource_selector, rotation, score_holder, scoreboard_slot, slot_source, style, swing_animation, swizzle, team, team_color, template_mirror, template_rotation, time, uuid, vec2, vec3}`.
(New since 1.21.5: `context_float_provider`, `context_int_provider` (26.3), `slot_source` (1.21.11), `swing_animation` (26.1/26.3), `dialog`, `hex_color`, `team_color`, `resource_selector`, `loot_*`, `feature` as inline-capable. Removed since 1.21.5: `entity_summon`, `item_enchantment`, `mob_effect` - replaced by `minecraft:resource`.)

Source for descriptions: https://minecraft.wiki/w/Argument_types , [G] parsers/properties.

### 2.1 Brigadier types

| Parser | Notes |
|---|---|
| `brigadier:bool` | `true` / `false` |
| `brigadier:integer{min,max}` | int32, optional min/max. Reading `9999999999` -> error `Invalid integer`. |
| `brigadier:long` | not used by any vanilla command |
| `brigadier:float`, `brigadier:double` | optional bounds; `.5`, `-.5`, `1` ok |
| `brigadier:string{type=word|phrase|greedy}` | **word**: unquoted `[A-Za-z0-9_\-.+]`; **phrase**: word or `"quoted"`/`'quoted'` string; **greedy**: rest of line |

### 2.2 `minecraft:*` parsers

* **`entity{amount=single|multiple,type=entities|players}`** - player name, selector or UUID. `single` + selector that can match >1 -> *unparseable* ("Only one entity is allowed, but the provided selector allows more than one") [V]. `type=players` + selector that could match non-players -> unparseable. See 2.3.
* **`score_holder{amount=single|multiple}`** - selector, player name, UUID, fake-player name, or `*` (all tracked holders; only valid for `multiple` targets; for `single` args `*` parses but resolves to nothing: `scoreboard players get * t` -> "No relevant score holders could be found"; `execute if score * t ...` silently terminates) [V]. Holder names are **unquoted words**; quotes are NOT supported (`"a b"` is a parse error); allowed chars include letters/digits (incl. non-ASCII `ünï`), `_ - . + : #`; even `""` and `0123` and UUID-looking text are accepted as names [V]. No length limit (removed 1.18). Spaces: use U+2007 figure space [W].
* **`objective`** (existing) and **`objective_criteria`** - objective names (for `scoreboard objectives add`, `brigadier:string{word}`) may contain only `[A-Za-z0-9_\-.+]`; `:`, `/`, non-ASCII are rejected [V]; no length limit [V] (a 60+ char name worked). Criteria: `dummy trigger deathCount playerKillCount totalKillCount health xp level food air armor` and compound `minecraft.<stat_type>:minecraft.<id>`, `teamkill.<color>`, `killedByTeam.<color>`; unknown -> `Unknown criterion 'foo'` [V].
* **`team{}`** - unquoted word. **`team_color`** - 16 lowercase underscore color names only (`dark_purple`; `darkred` rejected [V]); since 26.2 `reset` is accepted by `team modify <t> color reset`. Hex `#ff0000` rejected [V].
* **`resource_location`** - `[a-z0-9_.-]` namespace, `[a-z0-9_.-/]` path; default namespace `minecraft`; **uppercase letters rejected** (`T:ret1`, `Foo:bar` -> parse error [V]); used for storage ids, bossbar ids, stopwatch ids, predicates by name, etc. It is resolved at execution time against *unregistered* content (functions, bossbars, storages).
* **`function`** - resource location or `#tag` (function tag). **`resource{registry}`**, **`resource_or_tag{registry}`**, `resource_key`, `resource_or_tag_key`: existing registry entries (e.g. `minecraft:worldgen/biome`, `minecraft:entity_type`, `minecraft:world_clock`, `minecraft:timeline`). **`resource_selector{registry}`**: wildcards `*` (0+ chars) and `?` (1 char).
* **`dimension`** - resource location resolved at execution (`minecraft:overworld`, `the_nether`; namespace optional: `in the_nether` works [V]).
* **`block_pos`** - 3 ints/`~`/`^`; **`column_pos`** - 2 ints (x z) with `~`; **`vec3`**, **`vec2`** - doubles with `~` / `^` (vec3 only); **`rotation`** `<yaw> <pitch>` doubles with `~`; **`angle`** single float yaw; see 2.5.
* **`swizzle`** - non-repeating subset of `xyz` (`x`, `zx`, `yzx`...). **`entity_anchor`** - `eyes|feet`. **`heightmap`** - `world_surface|motion_blocking|motion_blocking_no_leaves|ocean_floor`. **`gamemode`** - `survival|creative|adventure|spectator`. **`scoreboard_slot`** - `list|sidebar|below_name|sidebar.team.<16 colors>` (`belowName` rejected [V]). **`operation`** - `= += -= *= /= %= < > ><`.
* **`int_range` / `float_range`** - `5`, `..5`, `5..`, `3..7` (inclusive; min>max -> error "Min cannot be bigger than max" [V]; `1..2..3` and `6.0` for int_range are errors [V]). Used by `scores={}`, `execute if score ... matches`, `distance=`, `level=`, `x_rotation=`, `random`, `execute if stopwatch`.
* **`time`** - number (float) + optional unit `d` (24000 ticks), `s` (20), `t` (1, default); result rounded to nearest tick (`1.5t`->2, `2.4t`->2, `1.5s`->30, `.5d`->12000) [V]; unit `w` -> `Invalid unit` [V]; per-use minimum in props (`schedule`: 0, `swing duration`: 1, `title times`: 0, `tick step|sprint`: 1, `time add`: int32 min).
* **`uuid`**, **`hex_color`** (3 or 6 hex digits), **`game_profile`**, **`message`** (greedy; embedded selectors are replaced by names), **`particle`**, **`item_slot`**/**`item_slots`** (`container.5`, `weapon`, `armor.*`, wildcard only in `item_slots`), **`item_stack`** (`id[comp=val,!comp]`), **`template_mirror|template_rotation`**, **`loot_table|loot_predicate|loot_modifier|feature|dialog|slot_source`** (a resource location OR an inline SNBT definition; an SNBT *string* is re-parsed as a resource location).
* **`style`** - SNBT style compound e.g. `{color:"red",bold:true}` (SNBT since 1.21.5).
* **`swing_animation`** - `whack|stab|none` ([V] `none` is accepted although the 26.3 patch note lists only whack/stab).
* **`context_int_provider` / `context_float_provider`** (26.3) - registry id of a data-pack-defined number provider, or an inline SNBT compound `{type:"minecraft:...",...}`. **A bare number is NOT accepted** in the command (`compute default integer 5` -> "Can't find element 'minecraft:5'"); use `{type:"minecraft:constant",value:5}`. (Bare numbers ARE fine nested inside `inputs:[...]` etc.) [V]. See 8.1.

### 2.3 Entity selectors [W Target_selectors][V]

Variables (Java): `@p` nearest player (ties: most recently joined), `@r` random player, `@a` all online players (alive or dead), `@e` all alive entities in loaded chunks (+ players), `@s` executor entity (alive or not; nothing if no executor), `@n` nearest alive entity (1.21, `limit=1` default; `@n[limit=2]` allowed [V]). Bedrock-only `@c @v @initiator` do not exist. Defaults: `@p`,`@r`,`@n` have implicit `limit=1`; sort default `nearest` for `@p`/`@n`, `random` for `@r`, `arbitrary` for `@a`/`@e`.

Syntax: `@x[arg=value,arg=value,...]` - no space between variable and `[` (`@e [tag=a]` -> error [V]); spaces around `[ = , ]` allowed [V]; all pairs ANDed; names/values case-sensitive; **quoted** values allowed.

| Argument | Value | Negation | Duplicates | Notes |
|---|---|---|---|---|
| `x`,`y`,`z` | double (no `~`) | - | no | origin for distance/volume/sort; restricts search to the *execution dimension* |
| `distance` | float_range (unsigned) | - | no (error "Option 'distance' isn't applicable here" [V]) | measured from feet point; sphere |
| `dx`,`dy`,`dz` | double (any sign) | - | no | box from (x,y,z) to (x+dx+1...) intersecting hitboxes; missing ones count as 0 |
| `x_rotation` | float_range | - | no | pitch -90..90 |
| `y_rotation` | float_range | - | no | yaw -180..180 |
| `scores` | `{obj=int_range,...}` | - | no | entity must HAVE a score in every listed objective |
| `tag` | name | `!name` | yes | `tag=` -> entities with zero tags; `tag=!` -> at least one tag [V `tag=` works] |
| `team` | name | `!name` | `=`-form once, `!`-forms many | `team=` -> not in a team; `team=!` -> in any team; (1.21.5: also non-living entities) |
| `name` | string (quote if spaces) | `!name` | `=` once | plain text name, not JSON |
| `type` | entity id or `#tag` | `!type` | `=` once; many `!` | the wiki says `type=` is not allowed with `@a/@p/@r`, but `@a[type=player]` and `@a[type=pig]` parsed fine in 26.3 [V] (they simply match players / nothing). `type=pig,type=cow` -> "Option 'type' isn't applicable here" [V] |
| `predicate` | resource location of a predicate | `!id` | yes | unknown id -> evaluates false (no parse error until run for `@e[predicate=t:nonexist]` [V]) |
| `nbt` | SNBT compound (partial match) | `!{...}` | yes | expensive |
| `level` | int_range | - | no | players only |
| `gamemode` | name | `!name` | `=` once | players only |
| `advancements` | `{id=bool}` or `{id={criterion=bool}}` | - | no | players only |
| `limit` | int >= 1 | - | no | `limit=0`/`-1` -> "Limit must be at least 1" [V] |
| `sort` | `nearest|furthest|random|arbitrary` | - | no | |

Unknown key -> `Unknown option 'x'` [V]. Single-type rules: `@s`, `@p/@r/@n` (no/1 limit), `@a/@e` with `limit=1` are "single"; commands requiring `amount=single` reject other selectors at parse time. Player-type: `@a`,`@p`, `@r`, `@e/@n[type=player]`; commands requiring `type=players` reject non-player selectors at parse time. Selecting by plain name: only online players; by UUID: any loaded entity.

### 2.4 SNBT, NBT paths, partial NBT matching

SNBT (`nbt_tag`, `nbt_compound_tag`) [W NBT_format][V]: `{k:v}` keys quoted or bare (bare keys `[A-Za-z0-9_\-.+]`), suffixes `b s L f d` (and `i`), `true/false` = `1b/0b`, hex ints `0x1F` (-> int 31) and underscores `1_000` (-> int 1000) accepted [V], `1e3` -> double `1000.0d` [V], typed arrays `[B; ...]`, `[I; ...]`, `[L; ...]`, strings in `"` or `'` (single-quote doubling `'it''s'` is NOT valid [V]; use `\'`). Lists may be **heterogeneous** since 1.21.5 (`[1,"a",{x:1}]` accepted and stored as-is [V]).

NBT path [W NBT_path][V]: nodes joined by `.` (the dot before `[` may be omitted):

| Node | Meaning |
|---|---|
| `name` / `"quoted name"` / `'q'` | child of compound; quote when the key has `[ ] . { } " space` etc. (`"a b"` verified) |
| `name{k:v,...}` | child compound that partially matches the filter |
| `{k:v}` | **root filter**, only as the first node; `{}` = root itself; fails (selects nothing) if root doesn't match |
| `[]` | all elements of a list/array |
| `[i]` | element i; negative counts from end (`[-1]` last) |
| `[{k:v}]` | all compound elements matching the filter |

Rules observed [V]: leading `.` -> "Invalid NBT path element"; `a..b` invalid; trailing `a.` is tolerated (treated as `a`); empty key paths no longer traversable since 1.21.5 (25w09a); `data get` needs a path selecting **exactly one** tag ("This argument accepts a single NBT value" for `[]`/`[{...}]` multi-selections); `data modify/remove/merge`-targets and `store` accept multi-selections. Matching is "partial": filter `{a:1}` matches any compound that contains `a:1b-exact-type` (type must match exactly: `1` != `1b` != `1d`; strings need full `"minecraft:stone"`); for lists every requested element must be present *somewhere* in the list (order/count ignored), **but an empty requested list `[]` only matches an empty list** [V `a{b:[]}` didn't match a non-empty list]; typed arrays are compared strictly.

### 2.5 Coordinates [W Coordinates][V]

* Absolute (`1 64 -3`, doubles for vec3, ints for block_pos which accept `~`/`^` too), relative `~` / `~dx` (offset from execution position), local `^left ^up ^forward` (frame from execution rotation; anchor feet/eyes applies).
* **World and local cannot be mixed** in one position ("Cannot mix world and local coordinates", [V]); workaround: `rotated ~ 0` etc.
* `block_pos` is the floor of the coordinates; entity position = feet center. y range checks: x,z in [-30000000, 30000000), y in [-20000000, 20000000) for `setworldspawn/spawnpoint/summon`.
* Rotations: yaw 0 = south (+Z), -90 = east (+X), 90 = west, +/-180 north; pitch -90 up .. 90 down. `~` in rotations is relative to execution rotation.

### 2.6 Text components in commands (`component`, `style`) [W Text_component_format][V]

Since 1.21.5 (25w02a) text components are **NBT/SNBT**: `{text:"Hello",color:"red"}`, `"plain string"`, `["a",{text:"b"}]`. JSON object syntax is still accepted because it is SNBT-compatible (`{"text":"json"}` accepted by `tellraw` [V]). A *single-quoted JSON string* is just a plain string: `scoreboard objectives modify t displayname '{"text":"json"}'` displays the literal text `{"text":"json"}` [V].

Content types (`type` optional, auto-detected in this order): `text`, `translatable`(`translate`,`fallback`,`with`), `score`(`{name,objective}`; `name:"*"` = reader's own), `selector`(`selector`,`separator`), `keybind`, `nbt`(`nbt`, `source`=`entity|block|storage` with `entity`/`block`/`storage`, `interpret`, `plain`, `separator`), `object` (1.21.9; `atlas`/`player` sprites). Style: `color` (name or `"#rrggbb"`), `font`, `bold italic underlined strikethrough obfuscated`, `shadow_color`, `insertion`, `click_event`(`open_url open_file run_command suggest_command change_page copy_to_clipboard show_dialog custom`), `hover_event`(`show_text show_item show_entity`), `extra`.
**Resolution**: `score`/`selector`/`nbt` are resolved server-side when sent (tellraw/title...), and for `bossbar set name`, `scoreboard objectives modify displayname`, `scoreboard players display name|numberformat fixed`, `team` prefix/suffix etc. they are resolved against the current executor `@s` (1.21.5+) - frozen at that moment, not live. 26.3: an `nbt` component in a raw chat message is resolved at most 64k times per message.

### 2.7 Item and block predicates (command arguments) [W Argument_types][V]

`block_predicate` (execute if block, fill/clone filters...): `<id|#tag>[state=val,...]{nbtPartialMatch}` e.g. `grass_block[snowy=false]`, `#minecraft:dirt`, `chest{Items:[]}`. Only the listed states are compared. Unknown state/value -> unparseable. `block_state` (setblock/fill): same but no tags and sets rather than tests.

`item_predicate` (execute if items, clear, ...): `<item_id|#tag|*>[test, test|test, ...]`:
* `component_id` (component exists), `component_id=<SNBT>` (exact value), `predicate_id~<SNBT>` (item sub-predicate, e.g. `damage~{durability:{min:3}}`), `count`, `count=N`, `count~N`, `count~{min:..,max:..}`, `!test` negation; `,` = AND, `|` = OR and **OR binds tighter than AND**. Verified in 26.3: `stone[count~{min:3}]`, `*[count=5]`, `#minecraft:stone_bricks` parse; `*[count>1]` does not ("Expected literal =").
`item_stack` (give...): `<id>[comp=val,!comp]`.

Slots: `slot_source` is new (below, 3.1 `if items|slots`): `container.*`, `armor.head`, `weapon.mainhand`, `hotbar.*` string notation still accepted as shorthand for `{type:"minecraft:slot_range",slots:"..."}`; inline compounds `{type:"minecraft:slot_range",slots:"weapon.*"}` and lists `[ ... ]` work [V]; `slot_range` has optional `source` (`container` default, `this`, `block_entity`, `target_entity`, ...). Types: `contents empty filtered group limit_slots slot_range` [G registry] (https://minecraft.wiki/w/Slot_sources).

---
## 3. `/execute`

Source: https://minecraft.wiki/w/Commands/execute , [G] commands.json, [V] tests. Permission: gamemasters (2). 14 sub-commands (`align anchored as at facing in on positioned rotated store summon if unless run`).

### 3.0 Full syntax tree [G]

`unless` has an identical tree to `if`. Every `=>execute` means "another sub-command (or `run`) must follow"; a node marked `$` may also end the command.

```
execute
  align <axes:swizzle> =>execute
  anchored <anchor:entity_anchor(eyes|feet)> =>execute
  as <targets:entity{multiple,entities}> =>execute
  at <targets:entity{multiple,entities}> =>execute
  facing <pos:vec3> =>execute
  facing entity <targets:entity{multiple,entities}> <anchor:entity_anchor> =>execute
  in <dimension:dimension> =>execute
  on (attacker|controller|leasher|origin|owner|passengers|target|vehicle) =>execute
  positioned <pos:vec3> =>execute
  positioned as <targets:entity{multiple,entities}> =>execute
  positioned over <heightmap:heightmap> =>execute
  rotated <rot:rotation> =>execute
  rotated as <targets:entity{multiple,entities}> =>execute
  summon <entity:resource{minecraft:entity_type}> =>execute
  store (result|success) <TARGET> =>execute      where TARGET is one of
       block <target:block_pos> <path:nbt_path> (byte|short|int|long|float|double) <scale:double>
       bossbar <id:resource_location> (value|max)
       entity <target:entity{single,entities}> <path:nbt_path> (byte|short|int|long|float|double) <scale:double>
       score <targets:score_holder{multiple}> <objective:objective>
       storage <target:resource_location> <path:nbt_path> (byte|short|int|long|float|double) <scale:double>
  (if|unless) biome <pos:block_pos> <biome:resource_or_tag{minecraft:worldgen/biome}>  $ =>execute
  (if|unless) block <pos:block_pos> <block:block_predicate>                         $ =>execute
  (if|unless) blocks <start:block_pos> <end:block_pos> <dest:block_pos> (all|masked) $ =>execute
  (if|unless) data block <source:block_pos> <path:nbt_path>                           $ =>execute
  (if|unless) data entity <source:entity{single,entities}> <path:nbt_path>            $ =>execute
  (if|unless) data storage <source:resource_location> <path:nbt_path>                 $ =>execute
  (if|unless) dimension <dimension:dimension>                                         $ =>execute
  (if|unless) entity <entities:entity{multiple,entities}>                             $ =>execute
  (if|unless) function <name:function>                                (NOT $, must be followed) =>execute
  (if|unless) items block <source:block_pos> <slots:slot_source> <item_predicate>     $ =>execute
  (if|unless) items entity <source:entity{multiple,entities}> <slots:slot_source> <item_predicate> $ =>execute
  (if|unless) loaded <pos:block_pos>                                                  $ =>execute
  (if|unless) predicate <predicate:loot_predicate>                                    $ =>execute
  (if|unless) score <target:score_holder{single}> <targetObjective:objective> (<|<=|=|>|>=) <source:score_holder{single}> <sourceObjective:objective> $ =>execute
  (if|unless) score <target:score_holder{single}> <targetObjective:objective> matches <range:int_range> $ =>execute
  (if|unless) slots block <source:block_pos> <slots:slot_source>                      $ =>execute   (26.3)
  (if|unless) slots entity <source:entity{multiple,entities}> <slots:slot_source>     $ =>execute   (26.3)
  (if|unless) stopwatch <id:resource_location> <range:float_range>                    $ =>execute   (1.21.11)
  run <command>
```

Rules: `run` is the only way to execute another command and must be last; a chain may also end on any `$` condition (`execute if entity @e[tag=a]` is a complete command whose result/success reflect the test). Modifier sub-commands (`as at ... store`) cannot end the chain. Sub-commands can appear in any order and any number of times. Anything after `run` is parsed from the **root**, so `... run execute ... run execute ...` is identical to a flat chain.
Score comparison operators are exactly `< <= = > >=` (no `!=`; use `unless ... =`). `if data` takes an NBT *path* (with optional `{...}` filter nodes), not a SNBT value.

### 3.1 Semantics

**Processing model [W][V]**: Java evaluates breadth-first: each sub-command is applied to *all* current branches (contexts) before the next sub-command runs; `run` command executes once per surviving branch, after all earlier sub-commands for all branches have been evaluated. So `execute as @e[tag=a] as @e[tag=a] run summon armor_stand` with 2 stands yields 4 summons, and `run` effects cannot influence the selection done by earlier sub-commands of the same `/execute` (selectors are all resolved first). To get depth-first behaviour call a function: `execute as @e run function ns:f` runs `f` fully for entity 1, then entity 2.

**Forking**: `as`, `at`, `positioned as`, `rotated as`, `facing entity`, `on passengers`, and `if/unless` (as a filter, when not last) can multiply or drop branches. A branch is dropped ("terminated") when a selector resolves to nothing / a condition fails (when not last). If all branches die, the command "terminated" (no error shown from console; `store` stores nothing).

**Execution context after each modifier**:
* `as <e>`: executor := e (and `@s`). Position/rotation/dimension/anchor unchanged.
* `at <e>`: position, rotation, dimension := e's (executor unchanged).
* `positioned <pos>`: position := pos (anchor reset to `feet`); `positioned as <e>`: position := e's (feet); `positioned over <hm>`: keeps x,z; y := 1 above the heightmap (terminates if the column isn't loaded; verified over overworld flat y=-60). Does not change rotation/dimension.
* `rotated <yaw> <pitch>` / `rotated as <e>`: rotation only. `facing <pos>` / `facing entity <e> <anchor>`: rotation := direction from the *execution position adjusted by the execution anchor* (`anchored eyes` => from the eyes) to the target (target anchor eyes/feet).
* `align <axes>`: floor the chosen axes of the position (`align xyz` = block corner; `align xz` keeps y fractional).
* `anchored eyes|feet`: sets anchor; affects local `^` coordinates and `facing`. (`positioned <pos>` resets it to feet.)
* `in <dim>`: dimension := dim; the execution position's x/z are divided by 8 (overworld -> nether) or multiplied by 8 (nether -> overworld) when relative/local coordinates are later resolved [W]; custom dimensions use their own coordinate scale. Use `positioned as @s` afterwards to keep the unscaled coordinates.
* `summon <type>`: creates a fresh entity at the execution position and makes it executor (so `@s` = new entity); terminates the whole `/execute` for `player` and `fishing_bobber` ("Can't summon entity of type ...") [V]; terminates a branch if position outside +/-30000000 (x,z) / +/-20000000 (y). Since 1.21.9 monsters can't be summoned in Peaceful (fails).
* `on <relation>`: executor := related entity: `attacker` (last entity that damaged executor within 5 s), `controller` (rider controlling it), `leasher`, `origin` (shooter of a projectile, igniter of TNT...), `owner` (tameable owner; 26.2: a vex's owner is its evoker), `passengers` (all direct passengers - forks), `target` (what it targets), `vehicle`. Terminates if no executor or relation not applicable.
* `store ...`: records a write-back target; see below.
* `run <cmd>`: executes cmd once per branch with the accumulated context.

**Store** [W][V]:
* `store (result|success) score <targets> <obj>`: score := the value (int). `*` selects all tracked holders [V]. Unknown objective -> error `Unknown scoreboard objective` and the command doesn't run [V].
* `store ... storage|entity|block <target> <path> <type> <scale>`: the integer value is **multiplied by scale (double)**, cast to the numeric NBT type, written at the path, created if missing. Casting is the Java cast: `byte` of 300 -> `44b` [V]; `int` of 7 with scale 0.5 -> `3` (7*0.5=3.5 truncated) [V]; `double` with scale 1 -> `7.0d` [V]. Cannot write to player NBT (silently useless/fails, [W]). A path whose parent list is too short silently does nothing (`arr.deep[2]` on empty list produced `{deep: []}`) [V].
* `store ... bossbar <id> value|max`: sets current value / max (clamped by the bossbar rules; 0 max is accepted by the store but the bossbar enforces min 1 for `set`) [V partially].
* Several `store` sub-commands may be chained (`store result score A store success score B run <cmd>`): both are captured from the same final command [V: result 6 / success 1]. `result` and `success` are of the *last* sub-command (a `run` command or the terminal condition).
* A failing/unrunnable final command stores `0`/`0` (result and success) [V: `store result score #w t if entity @e[tag=zzz]` -> 0].
* With forks, the store happens **once per branch after it finishes**; same target -> the **last** branch wins; no accumulation [V: `execute store result score #sum t as @e[tag=a] run scoreboard players set #w t 4` leaves 4; `store success ... run say` leaves 1].
* `run function f` : stored `result` = f's return value, `success` = 1 / 0 if `return fail`; if `f` is void nothing is stored (value untouched) [W]. For `run return ...` the stored values are the return's own (1.20.3+ allows storing the value).
* Non-integer outputs are floored before scaling [W]. Scores are int32; storing an int32 into float/double NBT is exact.

**Conditions, value of `result` when the condition is the LAST sub-command** [W][V]:

| Condition | passes when | `success` | `result` (if passes) | Fails/terminates also when |
|---|---|---|---|---|
| `block` | block at pos matches predicate | 1 | 1 | pos unloaded/out of world (fails even for `unless`!) |
| `blocks` | all (or all non-air-in-source for `masked`) blocks equal destination volume | 1 | `if`: number of compared blocks (all: volume; masked: #non-air source blocks) [V: 2x2x2 all -> 8; masked -> 4]; `unless`: 1 | any corner unloaded; source volume > 32768 |
| `biome` | biome/tag at pos | 1 | 1 | pos unloaded |
| `data block|entity|storage` | path selects >= 1 tag | 1 | `if`: number of selected tags [V `l[]` -> 3]; `unless`: 1 | block isn't a block entity / entity not found / pos unloaded |
| `dimension` | execution dim equals | 1 | 1 | |
| `entity` | selector matches >= 1 | 1 | `if`: number of matches [V -> 2]; `unless`: 1 | |
| `items` | **sum of item counts** in matching slots [V: 5 stone -> 5] | 1 | `if`: matching item count; `unless`: 1 | block not a container / entity absent |
| `slots` (26.3) | number of slots provided by the slot source that exist on the target [V: `armor.*` -> 5, `weapon.*` -> 2] | 1 | slot count (`if`) / 1 | |
| `loaded` | chunk at pos is entity-ticking | 1 | 1 | pos out of world |
| `predicate` | loot predicate true | 1 | 1 | unknown id -> error |
| `score` (both forms) | comparison true; **fails if either holder has no score in its objective** (`unless` then passes) [V] | 1 | 1 | `*` as holder -> terminates |
| `stopwatch` | elapsed seconds in range | 1 | 1 | stopwatch missing -> terminates [V: `if stopwatch t:nonex` silently nothing] |
| `function` | see below | - | - | cannot be the last sub-command |

`if score A obj matches R`: A must be a single holder; both operands missing -> `if` false, `unless` true [V]. `unless <cond>` passes iff `if <cond>` would NOT (including the fails-on-unloaded cases: unloaded pos makes both `if block` and `unless block` fail).
**Not last in the chain**: a passing condition lets the branch continue (acts as filter); a failing one terminates the branch.

`if|unless function <fn|#tag>` [W][V]: runs the function(s) as a *test*, **side effects happen**, context unchanged. `if`: continue iff the function returned success with non-zero value (void = counts as "false" and continues to next function in a tag; macro functions: treated as non-passing; non-existent function/empty tag terminates the whole `/execute`). `unless`: continue iff NOT that. Verified: `if function t:ret1` (returns 5) passes; `if function t:ret0` (`return fail`) fails; `unless function t:void` runs the function and continues; `if function <macro function with no args>` silently doesn't pass.

`execute if|unless predicate <inline>`: inline predicate in SNBT; in **26.3 the discriminator key is `type`** (was `condition` until 26.2); `value_check` was split into `int_value_check` / `float_value_check` with fields `value` + `test` (`test:6` equality or `test:{min:5,max:7}`), registry [G]: `all_of any_of damage_source_properties enchantment_active_check entity_properties entity_scores environment_attribute_check float_value_check int_value_check inverted killed_by_player location_check match_block match_tool random_chance random_chance_with_enchanted_bonus survives_explosion table_bonus time_check weather_check` (`reference` removed; a predicate-by-id is just the id). `all_of` no longer shortens to a bare list. [V] with `random_chance`, `int_value_check`, `float_value_check`, `inverted`, `all_of`.

**`execute` result as a whole**: the /execute command's own success-count (command blocks/comparators) = sum of branch successes; stored values are per branch (above).

**Return interplay**: `execute ... run return <v>` ends at the first branch that executes it - "only the first branch is executed" - and, inside a function, ends the function [V: `execute as @e[tag=a] run return 1` inside a function skipped the following line]. `return run execute ...` returns the first branch's result and fails (0) if the execute terminates.

**Gotchas**
* Entities summoned/killed by `run` are not seen by selectors of the same chain (breadth-first).
* `execute as @e ... ` from the console/command block without `at @s` runs with the console's position (spawn) - use `at @s`.
* `execute if entity @s[...]` is the idiom for "executor passes filter". With no executor `@s` selects nothing.
* `if score ... matches` with a range uses int32 `..`; there is no 64-bit compare.
* `positioned over` requires a loaded column. `if loaded` tests *entity-ticking* (full) status, not merely generated.

---

## 4. `/function`, `/return`, `/schedule`

### 4.1 `/function` [G][W][V]

```
function <name:function>                         (name = id or #tag)
function <name> <arguments:nbt_compound_tag>     (macro arguments)
function <name> with (block <pos> <path> | entity <e{single}> <path> | storage <id> <path>)   (path must select ONE compound)
```
Permission gamemasters.
* Runs all functions of a tag in order (duplicates once). Unknown function/tag -> fail (`Unknown function`). Empty tag: fails.
* **Macros** (1.20.2+): a function line starting with `$` is a macro line; `$(key)` placeholders (`[A-Za-z0-9_]+`) are substituted *each time the function runs* and the resulting command is parsed then (a parse error -> the **whole function is not run**, error `Failed to instantiate function ...`). Calling a function that contains macro lines without arguments fails: `Missing arguments to function t:mac` [V]. All keys used must be present; extra keys ignored. Value rendering [V]: string -> raw text without quotes; numbers -> plain decimal without suffix (`10b`->`10`, `1.5f`->`1.5`, `1.0d`->`1`, `true`->`1`); lists/compounds/arrays -> canonical SNBT (`[1,2]`). Up to 15 fractional digits [W].
* Return/result: a function that never executes `return` is **void**: `/function` yields no result (nothing stored, and in `if function` counts as non-pass). With `return N` (success): result N, success 1. With `return fail`: success 0, result 0. When a tag runs several functions, the results are **summed**... **but** inside `return run function #tag` execution stops after the first function that returns [W 1.20.3 23w44a]. Chat feedback `Function t:f returned 5` / `returned 0` [V].
* Execution context is inherited (executor, position, rotation, dimension, anchor, permission capped by function-permission-level - see 1.1). Output callback is not.
* Parsing: all non-macro lines are parsed at load; one bad line rejects the entire function file (`Failed to load function ...` at `/reload`) [V]. Commands in functions are written **without** leading `/`.
* Depth-first behaviour comes from functions: recursion allowed; total commands per tick bounded by `max_command_sequence_length`; a function can call itself (verified 21845 recursions stop at the limit, the tick continues).
* Execution order each tick [W]: advancement-reward functions, enchantment `run_function`, `#minecraft:tick` functions, scheduled functions, then command blocks/players. `#minecraft:load` functions run first on load and on every `/reload`, before any player is present (selectors find no players). Since 1.19.3 load runs before tick.

### 4.2 `/return` [G][W][V]

```
return <value:integer>        success, result = value; ends the current function
return fail                   failure (success 0, result 0); ends the function
return run <command>          runs command; function ends; function's success/result := command's success/result
```
Permission gamemasters. Outside a function (console) `return 3` is accepted and does nothing visible, but still has output values (`execute store result score #r t run return 9` stored **9** [V]).
`return run <fork>`: only the first branch executes. `execute ... run return run <cmd>`: if the execute terminates, the return does not run (function continues); `return run execute ...` always returns, with fail/0 when the execute terminates. `return run function f` where `f` is void: the outer function **fails**. Tags: `return run function #t` stops after the first function in the tag that returns.
`return` is the *only* way to produce a function result; typical lowering of `if/else-if` chain: `execute if <c1> run return run function a` then `execute if <c2> run return run function b` ... then fall-through.

### 4.3 `/schedule` [G][W][V]

```
schedule function <function:function> <time:time{min=0}> [append|replace]      (default: replace)
schedule clear <function:resource_location>      (plain id only: `schedule clear #ns:tag` is a PARSE ERROR in 26.3 [V by slab zoo test]; namespace may be omitted)
```
* Function runs later as the **server** (no executor; position = world spawn, overworld) regardless of `execute as/at` prefix [W] (the `execute` context is dropped).
* `replace` (default) overrides any pending schedule of the same function/tag; `append` keeps multiple at different times. Same function cannot be scheduled twice for the same game tick (append of identical tick is a no-op) [W].
* `time` `0` -> fail "Can't schedule for current tick" [V]; units d/s/t, fractional rounded [V: `1.5s`->30, `5`->5 ticks, `1.5t`->2]; `-1t` -> parse error.
* `1t` from inside a tick function runs later *in the same tick* (scheduled phase comes right after tick functions); `1t` from inside a scheduled function runs next tick [W].
* Tags: `schedule function #ns:tag 1t` OK (message "Scheduled tag ...") even if tag currently empty [V].
* Result: game tick at which it will run (mod 2147483647); `schedule clear` result = #cleared, fails (`No schedules with ID`) if none [V]. `schedule clear tickfn` without namespace resolves to `minecraft:tickfn` and works (reported "No schedules with ID minecraft:tickfn") [V].
* Pending schedules persist in the world save and survive `/reload`.

---

## 5. `/scoreboard`, `/trigger`, `/tag`

### 5.1 `/scoreboard` tree [G]
Permission gamemasters.
```
scoreboard
  objectives
    add
      <objective:b:string{type=word}>
        <criteria:m:objective_criteria> $
          <displayName:m:component> $
    list $
    modify
      <objective:m:objective>
        displayautoupdate
          <value:b:bool> $
        displayname
          <displayName:m:component> $
        numberformat $
          blank $
          fixed
            <contents:m:component> $
          styled
            <style:m:style> $
        rendertype
          hearts $
          integer $
    remove
      <objective:m:objective> $
    setdisplay
      <slot:m:scoreboard_slot> $
        <objective:m:objective> $
  players
    add
      <targets:m:score_holder{amount=multiple}>
        <objective:m:objective>
          <score:b:integer{min=0}> $
    display
      name
        <targets:m:score_holder{amount=multiple}>
          <objective:m:objective> $
            <name:m:component> $
      numberformat
        <targets:m:score_holder{amount=multiple}>
          <objective:m:objective> $
            blank $
            fixed
              <contents:m:component> $
            styled
              <style:m:style> $
    enable
      <targets:m:score_holder{amount=multiple}>
        <objective:m:objective> $
    get
      <target:m:score_holder{amount=single}>
        <objective:m:objective> $
    list $
      <target:m:score_holder{amount=single}> $
    operation
      <targets:m:score_holder{amount=multiple}>
        <targetObjective:m:objective>
          <operation:m:operation>
            <source:m:score_holder{amount=multiple}>
              <sourceObjective:m:objective> $
    remove
      <targets:m:score_holder{amount=multiple}>
        <objective:m:objective>
          <score:b:integer{min=0}> $
    reset
      <targets:m:score_holder{amount=multiple}> $
        <objective:m:objective> $
    set
      <targets:m:score_holder{amount=multiple}>
        <objective:m:objective>
          <score:b:integer> $
```
(Operators for `operation`: `= += -= *= /= %= < > ><`. `rendertype` hearts|integer. Scoreboard slots: `list sidebar below_name sidebar.team.<black|dark_blue|dark_green|dark_aqua|dark_red|dark_purple|gold|gray|dark_gray|blue|green|aqua|red|light_purple|yellow|white>` [W+V].)

**Objectives** [W][V]: names `[A-Za-z0-9_\-.+]+` (no `:`/`/`/non-ASCII; no length limit). `add` fails if exists ("An objective already exists by that name"). `remove` deletes all scores. Display name = component (SNBT). `displayautoupdate`, `numberformat` (blank | fixed <component> | styled <style>; no arg resets). `setdisplay <slot> [obj]` clears the slot when obj omitted; same objective already there -> fail. `sidebar` shows top 15 by score; fake holders whose name starts with `#` are hidden from the sidebar (the `#name` convention for private variables). Per-entry `players display name|numberformat`. Criteria list: see 2.2. **26.2**: a player's score is no longer shown in the tab list if they lack the objective. Objectives whose criterion is read-only (`health xp level food air armor`) can't be written by commands (`set` -> fail "objective is read-only").
**Criteria that auto-update**: `deathCount playerKillCount totalKillCount` (writable), `health xp level food air armor` (read-only), `trigger`, stat criteria `minecraft.<custom|mined|crafted|used|broken|picked_up|dropped|killed|killed_by>:minecraft.<id>`, `teamkill.<color>`, `killedByTeam.<color>`. Non-player entities only have `dummy`-type use (scores deleted when they die).

**Score holders** [V]: any string without spaces; `#`-prefixed names conventionally = fake players/constants; entities are identified by UUID string; `*` = all tracked holders (works for `set add remove reset operation` targets and `store` targets, NOT for `get`/`list <target>`/`if score`); names are case-sensitive; can be selector, so `scoreboard players set @e[tag=x] obj 5`. Players keep scores while offline.
**Score range**: int32. Arithmetic wraps like Java ints, **no error**: `add` 2147483647+1 -> `-2147483648`; `remove` below min wraps to `2147483647`; `*=` wraps (100000*100000 -> `1410065408`); `/=` is `Math.floorDiv` (`-7 /= 2` -> `-4`; `7 /= -2` -> `-4`; `MIN /= -1` -> `MIN`); `%=` is `Math.floorMod` (result has the divisor's sign: `-7 %= 2` -> `1`, `3 %= -4` -> `-1`); divide/modulo by 0 -> fail "Cannot divide by zero" (command failure, target unchanged); `><` swaps (the only op that modifies the source). `add`/`remove` amount must be `>= 0` (`add ... -1` is a parse error "Integer must not be less than 0"); `set` accepts any int32. Literal `9999999999` -> parse error. A missing score on either side of `operation` is created as 0 (`operation #n t = #unset t` -> `#n = 0` and **creates** `#unset` = 0) [V]. `=` etc. with several sources performs the op once per source in order; multiple targets: each target.
**Result values** [W][V]: `get` -> the score (fails "Can't get value ... none is set" if unset, so `execute store result score X run scoreboard players get Y` leaves X unchanged when Y is unset); `set` -> targets*value; `add/remove` -> sum of new scores; `operation` -> sum of the target scores after; `reset` -> #targets (and `reset *` = #tracked holders; `reset <t> <obj>` removes only that objective and **disables a trigger objective**); `enable` -> #newly enabled; `players list` -> #tracked / #objectives; `objectives list/add/remove` -> #objectives.
**Gotchas**: message `Set [t] for 0 entities to 0` appears when value is 0 (cosmetic; result = 0 which is also what `execute store result` receives, so use `store success` or `matches` rather than relying on `set` results); `scoreboard players get` of an unset score is a *failure*, but `if score` just fails; writing `execute store result score #a t run scoreboard players add #a t 5` stores the new value.

### 5.2 `/trigger` [G][W]
```
trigger <objective:objective> $
trigger <objective> add <value:integer>
trigger <objective> set <value:integer>
```
Level 0. Executor must be a **player** with the objective having criterion `trigger` and "enabled" for them (`scoreboard players enable <player> <obj>`); after use it is auto-disabled again. Plain `trigger obj` = `add 1`. Result = the player's resulting score. Fails (not parse error) if not a player / not enabled ("A player is required to run this command here" from console [V]). It's the only way for non-ops to feed data to a data pack (combine with `tellraw` clickable `run_command:"/trigger x set 3"` or dialogs).

### 5.3 `/tag` [G][V]
```
tag <targets:entity{multiple,entities}> add <name:word>     (fails if already present or already 1024 tags)
tag <targets> remove <name>                                (fails if none had it)
tag <targets> list
```
Result = #entities changed (`list`: total number of listed tags). Tags are plain strings in the entity's `Tags`, selectable via `tag=`. Messages: "Target either already has the tag or has too many tags", "Target does not have this tag" [V]. Works on players too (stored in player data) and any entity.

---

## 6. `/data`

Permission gamemasters. Source: https://minecraft.wiki/w/Commands/data , [G], [V].

### 6.1 Syntax [G]
```
data get (block <pos> | entity <e{single}> | storage <id>) [<path>] [<scale:double>]
data merge (block <pos> | entity <e{single}> | storage <id>) <nbt:compound>
data remove (block <pos> | entity <e{single}> | storage <id>) <path>
data modify (block <pos> | entity <e{single}> | storage <id>) <targetPath> <OP> <SOURCE>
   OP     = append | prepend | set | merge | insert <index:integer>
   SOURCE = from (block <pos> | entity <e{single}> | storage <id>) [<sourcePath>]
          | string (block <pos> | entity <e{single}> | storage <id>) [<sourcePath>] [<start:int> [<end:int>]]
          | value <nbt_tag>
          | compute (default | block <pos> | entity <e{single}>) (float <context_float_provider> | integer <context_int_provider>)      (26.3)
```
`insert <index>` takes the same four SOURCE forms. Storage id is a resource_location (`foo` == `minecraft:foo`; uppercase rejected) [V].

### 6.2 Semantics [W][V]
* **Storage** (`command storage`): global, persistent, per-id compound created on first write (`data modify storage ns:x a set value 1`); reading a never-written storage path -> "Found no elements matching ...". Lives in the world's `data/command_storage_<ns>.dat`.
* **Block** target requires a loaded block entity; **entity** target requires exactly one entity; **players cannot be modified** (`data modify/merge/remove entity <player>` fail "Unable to modify player data"; `get` works) [W].
* `get` output & result: no path -> whole compound, result 1 [V]. With a path: result = number elements/length: numeric -> `floor(value)` (double->floor: `3.9d`->3, `-3.9d`->-4 [V]; long `3000000000L` -> int clamps to `2147483647` [V]); list/array -> length; string -> char length; compound -> key count [W]. With `<scale>`: `floor(value*scale)` (`5b * -0.3` -> -2; `5b*0.3` -> 1; `Pos[1] 100` -> 6400) [V]; non-numeric + scale -> fail. Fails if the path doesn't exist ("Found no elements matching") or selects several tags.
* `merge`: recursive compound merge (scalars/lists replaced); "Nothing changed. The specified properties already have these values" -> **fail** when nothing differs [V]. `merge storage x {}` always fails.
* `remove`: removes all tags selected by the path; fails if nothing was removed or path is the root (`{}`) [V].
* `modify ... set`: replaces every selected tag. Path may not exist yet for compound keys: intermediate compounds are created (`x.a.b.c set value 1` on `{}` -> `{a:{b:{c:1}}}` [V]). A list-index step into a missing list creates the **empty list but sets nothing**: `x.a.arr[0] set value 1` reported "Nothing changed" yet left `arr: []` [V]; `lst[5] set` out of range -> fail [V]. Setting the root `{}` fails [V]. Type of the stored tag is exactly the source tag's type (no coercion): `5b`, `1.5f`, `3L`...
* `append`/`prepend`/`insert i`: target must be list or typed array; new element appended/prepended/inserted (negative `insert` index counts from the end with **-1 = after the last element** [V: `insert -1` appended]; index > size -> fail "Invalid list index: 99" [V]). Lists may be heterogeneous (since 25w09a). Typed arrays coerce numeric sources (`[I;1,2,3] append value 4L` -> int array got `4` [V]); non-numeric source -> fail ("Nothing changed") [V]. Result = #lists changed. Source `from` with a path selecting several tags appends all of them.
* `merge` (modify): target tags and source must both be compounds; recursive merge; `data modify storage s a.b[0] merge value {k:1}` OK.
* `string` source: takes a string (or number converted to string) and a substring `[start,end)`; negative indices count from end (`-5` -> last 5 chars), `start`/`end` out of range or start>end -> fail [V `str 6` -> "world"; `0 -6` -> "hello"; `-5` -> "world"].
* `from` source: copies tags **by value at that moment** (no reference); `set from` with a multi-valued source path uses the first.
* `compute` source (26.3): evaluates a context number provider and stores an **int tag** (`integer`) or **float tag** (`float`): `float` of `{mul:[2.5,3]}` -> `7.5f` [V]; no `scale`, no cast suffix argument in the final syntax (the snapshot syntax `... [integer]` no longer exists) [G]. This is the only exact float arithmetic available.
* Paths with `[{...}]` filters / `[]` apply the op to every match: `data modify storage s a.b[].z set value 0b` set `z` in all elements; `a.b[{id:"x"}].n set value 9` changed both matches [V].
* `execute if data ... <path>` / `store ... <path>` use the same path semantics (multi-match allowed).
* Performance (**UNVERIFIED**): `data get entity` serialises the whole entity; avoid in hot loops. Block entity targets reload chunk data; entity changes of some keys (e.g. `Air`, `Health`) may be post-processed.

---
## 7. Messaging, UI and state commands

### 7.1 `/tellraw`, `/title`, `/say`, `/me`, `/msg` (`/tell`,`/w`), `/teammsg` (`/tm`) [G][W]
```
tellraw <targets:entity{multiple,players}> <message:component>                      gamemasters
title <targets:entity{multiple,players}> (clear | reset)
title <targets> (title|subtitle|actionbar) <title:component>
title <targets> times <fadeIn:time{min=0}> <stay:time{min=0}> <fadeOut:time{min=0}>
say <message:message>                                                               gamemasters
me <action:message>                                                                 level 0
msg|tell|w <targets:entity{multiple,players}> <message:message>                    level 0
teammsg|tm <message:message>                                                        level 0 (executor must be an entity in a team)
```
* `tellraw`/`title`: `component` is SNBT (see 2.6). Non-player-type selector -> unparseable. No online target -> fail "No player was found" [V]; result = #players (so `store result ... run tellraw @a ...` counts recipients). Unknown JSON keys are ignored (`{text:"x",unknown:1}` parsed [V]). A syntax error in the component is a parse error.
* `title`: defaults fadeIn/stay/fadeOut = 10/70/20 ticks (`reset` restores and clears subtitle); `times` accepts `time` units (`1s`, `20`). Titles are per-player state; `title ... title` shows both pending subtitle and title; action bar is separate.
* `say`/`me`/`msg`: `message` may contain selectors that are expanded to entity names ("name1, name2 and name3") - **`say hi @e` expands `@e` in the text** [V]. `say` shows `[<executor name>]`; from console `[Server]`; entity name (`[Armor Stand]`) when run via `execute as`. `me` -> `* name text`. Results are 1 (say/me) or #recipients (msg).
* `teammsg`: fails when executor isn't an entity ("An entity is required ...") or has no team.
* Messages can be suppressed from command feedback by `gamerule send_command_feedback false` (affects the executor's own feedback, not chat broadcast of `say`/`tellraw`).

### 7.2 `/bossbar` [G][V]
```
bossbar
  add
    <id:m:resource_location>
      <name:m:component> $
  get
    <id:m:resource_location>
      max $
      players $
      value $
      visible $
  list $
  remove
    <id:m:resource_location> $
  set
    <id:m:resource_location>
      color
        blue $
        green $
        pink $
        purple $
        red $
        white $
        yellow $
      max
        <max:b:integer{min=1}> $
      name
        <name:m:component> $
      players $
        <targets:m:entity{type=players,amount=multiple}> $
      style
        notched_10 $
        notched_12 $
        notched_20 $
        notched_6 $
        progress $
      value
        <value:b:integer{min=0}> $
      visible
        <visible:b:bool> $
```
`id` is a resource location (lowercase). `add` fails if id exists ("A bossbar already exists with the ID"); `get/set/remove` fail if unknown. Defaults at creation: `max 100`, `value 0`, `color white`, `style progress`, `visible true`, `players` none. `set value` is clamped to `[0,max]` by the bar (setting 50 on max 20 reported "changed value to 50" and `get value` returned 50 - the display clamps but the stored value isn't [V]); `set max` min is 1. `set color|style|name|visible|players` fail with "Nothing changed ..." when the value is unchanged [V]. `set players` with no argument clears the audience. Results: `add/remove/list` -> #custom bars; `get max|set max` -> max; `get value|set value` -> value; `get players|set players` -> #players; `get visible` -> 1/0; other `set` -> 0. `execute store result|success bossbar <id> value|max` writes the bar value/max [V]. Name is a component resolved against `@s` at set time.

### 7.3 `/team` [G][W][V]
```
team
  add
    <team:b:string{type=word}> $
      <displayName:m:component> $
  empty
    <team:m:team> $
  join
    <team:m:team> $
      <members:m:score_holder{amount=multiple}> $
  leave
    <members:m:score_holder{amount=multiple}> $
  list $
    <team:m:team> $
  modify
    <team:m:team>
      collisionRule
        always $
        never $
        pushOtherTeams $
        pushOwnTeam $
      color
        reset $
        <value:m:team_color> $
      deathMessageVisibility
        always $
        hideForOtherTeams $
        hideForOwnTeam $
        never $
      displayName
        <displayName:m:component> $
      friendlyFire
        <allowed:b:bool> $
      nametagVisibility
        always $
        hideForOtherTeams $
        hideForOwnTeam $
        never $
      prefix
        <prefix:m:component> $
      seeFriendlyInvisibles
        <allowed:b:bool> $
      suffix
        <suffix:m:component> $
  remove
    <team:m:team> $
```
Teams are server-wide; a holder (player name, UUID or fake name such as `#fake`) is in at most one team. `join <team>` with no members = the executor entity ("An entity is required to run this command here" for console [V]). `leave` removes from any team. `empty` fails if already empty. `add` fails if exists; name is an unquoted word. Defaults: friendlyFire true, seeFriendlyInvisibles true, nametagVisibility/deathMessageVisibility always, collisionRule always, color reset. Quirks: the wiki documents the `collisionRule` names `pushOtherTeams`/`pushOwnTeam` as behaving contrary to their literal meaning (bug MC-87984): `pushOtherTeams` = "entities in this team can be pushed only by other entities in the same team", `pushOwnTeam` = "cannot be pushed by another entity of this team" [W]; `friendlyFire` has no effect when the `pvp` gamerule is false. Option values are case-sensitive (`hideForOtherTeams`). `color`: only the 16 lowercase names + `reset` (26.2 change; `darkred`, `DarkRed`, `#ff0000` rejected [V]). `prefix`/`suffix`/`displayName` are components.
Results: `team list` -> #teams; `list <team>` -> #members; `add/remove` -> #teams after; `empty` -> #members before; `join` (no members) -> 1; `join <members>` and `leave` -> **exact number actually changed since 26.3** (entities already in that team are excluded); `modify` options -> 0 (prefix/suffix: 1). Teams show in `@e[team=...]` and affect glow-outline colour, name colour, and `sidebar.team.<color>`.

### 7.4 `/random` [G][W][V]
```
random
  reset
    * $
      <seed:b:integer> $
        <includeWorldSeed:b:bool> $
          <includeSequenceId:b:bool> $
    <sequence:m:resource_location> $
      <seed:b:integer> $
        <includeWorldSeed:b:bool> $
          <includeSequenceId:b:bool> $
  roll
    <range:m:int_range> $
      <sequence:m:resource_location> $
  value
    <range:m:int_range> $
      <sequence:m:resource_location> $
```
`value` replies only to the executor; `roll` broadcasts ("Server rolled 4 (from 1 to 10)"). Range must contain **at least 2 values**: `5..5` -> fail "The range of the random value must be at least 1"; max range size 2147483646 (`1..2147483647` was accepted in 26.3 [V] although the wiki states the range size must be <= 2147483646; treat the wiki limit as the safe bound). `random value 6..1` -> parse error. Optional `<sequence>` = named reproducible stream (default: the world's non-sequenced RNG; `sequence` requires level 2). `reset *` removes all sequences (result = #sequences reset); `reset <seq> [seed [includeWorldSeed [includeSequenceId]]]` re-seeds (defaults 0/true/true). Result of value/roll = the number drawn -> `execute store result score #r t run random value 1..100` is the standard RNG primitive.

### 7.5 `/tick` [G][V]
```
tick query | freeze | unfreeze | rate <rate:float 1.0..10000.0> | step [<time>=1t] | step stop | sprint <time> | sprint stop
```
admins (3) - **not usable in functions at the default function permission level 2** (see 1.1; and level-3 commands don't parse inside functions). `rate` accepts 1-10000 TPS (`0.5` -> parse error "Float must not be less than 1.0"). `freeze` stops ticking of everything except players and the entity they ride; `step` only when frozen (else fail "Unable to step the game - the game must be frozen first"); `sprint` runs as fast as possible for N ticks (+1) ignoring rate. 26.3: `step`, `step stop`, `sprint stop` now signal an actual command error (fail) instead of printing and returning 0. Results [V]: `freeze` 1, `unfreeze` 0, `rate` = the rate, `step stop` fails if none in progress, `query` = target rate.

### 7.6 `/kill`, `/forceload`, `/difficulty`, `/setworldspawn`, `/spawnpoint`, `/seed`, `/datapack`, `/reload`, `/version`, `/debug`

**`/kill [<targets:entity{multiple,entities}>]`** (default = executor; with no executor -> fail). Deals `generic_kill` damage (bypasses invulnerability; creative-mode players are not actually killed but still counted - wiki). Result = #targets. Fails with `No entity was found` if the selector is empty [V]. 26.2: killing an end crystal no longer aborts the dragon respawn.

**`/forceload add <from:column_pos> [<to>]` / `remove <from> [<to>]` / `remove all` / `query [<pos>]`** [G][V]: operates on the execution dimension; columns are block x/z, converted to chunks; **max 256 chunks per `add`** (error `Too many chunks in the specified area (maximum 256, but specified 3969)`) [V]. `add` of already-forced chunks -> fail "No chunks were marked for force loading" [V]. Results: add -> #newly added (48 for 7x7 minus 1) [V], remove -> #removed, `remove all` -> 0, `query` -> #forced chunks in dimension, `query <pos>` -> 1 (fails if not forced). Chunk loading is asynchronous (not guaranteed loaded the same tick). Forced chunks are entity-ticking (random ticks since 1.21.5). Useful to keep a data-pack "scratch" area loaded.

**`/difficulty [peaceful|easy|normal|hard]`**: query form returns 0..3 and prints; set form returns 0; setting the same difficulty -> fail "The difficulty did not change; it is already set to Hard" [V].

**`/setworldspawn [<pos:block_pos> [<rotation:rotation>]]`** [G] (yaw+pitch; wiki still shows the old `[<pos>] [<angle>]`): sets spawn of the **execution dimension** (no longer overworld-only since 1.21.9); verified `setworldspawn 0 64 0 45 10` -> "[45.0, 10.0]"; a lone yaw -> `Incomplete (expected 2 coordinates)` [V]. Result 1. Spawn chunks no longer exist (1.21.9) so it doesn't force-load.
**`/spawnpoint [<targets:entity{multiple,players}> [<pos> [<rotation>]]]`**: default executor / execution position / executor facing; from console without player -> fail "A player is required to run this command here" [V]; result = #players.

**`/seed`**: prints world seed; result = low 32 bits of the seed as int32 (`-2662783660086904687` -> `-2100414319` [V]).

**`/datapack`** [G][V]
```
datapack list [available|enabled]
datapack enable <name:phrase> [first|last | before <existing> | after <existing>]
datapack disable <name:phrase>
datapack create <id:phrase> <description:component>          (owners only)
```
Pack names are like `vanilla`, `file/<folder>` (quote when containing `/`? `"file/slabtest"` worked quoted; phrase type allows `/` only in quotes **UNVERIFIED** unquoted), `mod/...`. enable/disable trigger a reload (all `#minecraft:load` functions rerun; scheduled functions persist). Result = #packs enabled after. Fails for unknown/already enabled/disabled packs or packs requiring disabled feature flags. **Do not `datapack disable vanilla`**: in my run the world could not start afterwards (server exited at startup) [V observation, cause inferred]. `datapack create` makes an empty folder pack in the world's `datapacks/` (owners; 1.21.6).

**`/reload`**: reloads data packs (functions, predicates, loot tables, advancements, recipes, tags, dimension-type... experimental registries need a world reload). Result 0. A function with a parse error stays unloaded and logs `Failed to load function ...` [V]; the rest still loads. Resets nothing in the scoreboard/storage. Triggers `#minecraft:load` again.

**`/version`**: prints server version info (id, name, data version, series, protocol, build_time, pack_resource, pack_data, stable). Always succeeds, result 1.

**`/debug start|stop|function <name>`** (admins=3): profiler / per-command trace written to `debug/` (`debug function` cannot be nested in `return run` or another `debug function`; results 1 / avg TPS / number of commands). `/jfr`, `/perf` (owners) are profilers, not relevant.

---

## 8. New / changed since 1.21.5

### 8.1 `/compute` (26.3-snapshot-10, syntax finalised pre-1) [G][W][V]

Evaluates a **number provider** (the loot-table number-provider language, now a general arithmetic expression language) and returns the value; gamemasters. This is the only vanilla way to do floating point arithmetic and overflow-checked int arithmetic.

```
compute
  block
    <computePos:m:block_pos>
      float
        <provider:m:context_float_provider> $
          <scale:b:float> $
      integer
        <provider:m:context_int_provider> $
  default
    float
      <provider:m:context_float_provider> $
        <scale:b:float> $
    integer
      <provider:m:context_int_provider> $
  entity
    <computeTarget:m:entity{type=entities,amount=single}>
      float
        <provider:m:context_float_provider> $
          <scale:b:float> $
      integer
        <provider:m:context_int_provider> $
```
* Context: `default` (origin = execution position, `this` = `@s`), `block <pos>` (adds block state / block entity), `entity <e>` (adds `target_entity`). Loot context types `minecraft:command_compute_default|position|entity`.
* `integer <provider>`: `result` = the int value. `float <provider> [scale=1.0]`: `result = floor(value * scale)` (floor towards -inf; message shows the float value and "rounded to N") [V: `sqrt(2)` -> 1; `10.0 * scale 100` -> 1000; `1.4142135 * 1000` -> 1414]. Integer providers truncate between intermediate steps, so int-mode can differ from float-mode at scale 1.
* The argument is either a **registry id** (a data-pack file `data/<ns>/context_int_provider/<name>.json` / `context_float_provider/<name>.json`) or an **inline SNBT compound** `{type:"minecraft:...",...}`. A bare literal number is NOT accepted as the command argument (error "Can't find element 'minecraft:5' in registry ..."); wrap `{type:"minecraft:constant",value:5}`. Inside nested fields bare numbers are accepted (`inputs:[7,3]`, `left:7`).
* Failure (success 0, red message "Number provider returned invalid value (...)"): division by zero (`/ by zero`), int overflow of add/mul/etc. (`Value 2147483648 can't be safely converted to int` - integer providers compute in 64-bit and fail instead of wrapping) [V]. Messages print the id of a registered provider if any.
* `execute store result score #r t run compute default integer {...}` therefore gives overflow-**checked** arithmetic (failure => `store` writes 0 and `store success` 0).
* `data modify <target> <path> <set|append|merge|prepend|insert i> compute <ctx> (integer <int_provider> | float <float_provider>)`: stores an **int tag** or **float tag** (full float precision; no scale) [V `7.5f`, `21`].

Provider types in registries [G]: **int** (`context_int_provider_type`): `abs add avg binomial conditional constant div environment_attribute floor_div floor_mod from_float max min mod mul negate number_dispatcher pow score storage sub uniform weighted_list`; **float**: `abs add avg ceil conditional constant cos div enchantment_level environment_attribute floor from_int length max min mod mul negate number_dispatcher pow round sin sqrt storage sub truncate uniform weighted_list`. Field names (from wiki `Number_provider`, [V] for those marked): `constant{value}`[V]; `add|mul|avg|min|max{inputs:[...]}`[V add,mul]; `sub|div|mod|floor_div|floor_mod{left,right}`[V]; `pow{base,exponent}`[V]; `abs|negate|ceil|floor|round|truncate|sqrt|sin|cos|from_int|from_float{input}`[V sqrt,from_int,from_float]; `length{inputs}`; `uniform{min,max}`(inclusive) ; `binomial{n,p}`; `conditional{condition:<predicate>,on_true,on_false=0}`; `number_dispatcher{cases:[{condition,value}],default=0}`; `weighted_list{distribution:[{data,weight}]}`; `score{target,score,fallback=0}` where `target` is `{type:"minecraft:fixed",name:"#w"}` (fixed holder / UUID) or `{type:"minecraft:context",target:"this|target_entity|..."}` or the string shorthand `"this"` [V: `target:"this"` accepted, returned 0 with no executor]; `storage{storage:"ns:id",path:"nbt.path",fallback=0}` [V, reads numeric tag; a float storage value works in float providers: `2.5f` -> `2.5`]; `environment_attribute{attribute}`; `enchantment_level{amount}` (no enchantment in command contexts).
Semantics [V]: int `div` truncates toward zero (7/2=3, -7/2=-3); `floor_div` -> -4; `mod` = Java `%` (-7 mod 2 = -1), `floor_mod` -> 1; `from_float` truncates toward zero (2.7->2, -2.7->-2); missing score/storage -> fallback (default 0); `pow 2^10` -> 1024.
Worked lowering example (score arithmetic with checked overflow): `execute store result score #out t run compute default integer {type:"minecraft:add",inputs:[{type:"minecraft:score",target:{type:"minecraft:fixed",name:"#a"},score:"t"},{type:"minecraft:score",target:{type:"minecraft:fixed",name:"#b"},score:"t"}]}`.
Performance of parsing the inline SNBT on every execution: **UNVERIFIED** (should be cached by a data-pack-defined provider id: put the JSON in `data/<ns>/context_int_provider/foo.json` and call `compute default integer ns:foo`).

### 8.2 `/stopwatch` (1.21.11) [G][W][V]
```
stopwatch create <id> | query <id> [<scale:double>] | restart <id> | remove <id>          gamemasters
execute (if|unless) stopwatch <id> <range:float_range>                                    (seconds)
```
Real-time clock (milliseconds, independent of tick rate/pause/lag; runs only while the world is loaded). `query` result = elapsed seconds * scale, floored and clamped to int32 (default scale 1; `scale 20` -> ticks); chat shows `1.2s`. `create` fails if exists; `query/restart/remove` fail if absent. `if stopwatch` on a missing stopwatch terminates silently [V]. Use it to detect lag or measure real durations.

### 8.3 `/time` (reworked in 26.1: world clocks) [G][W][V]
```
time (add <time> | set (<time> | <timemarker>) | pause | resume | rate <float 1e-5..1000> | query (gametime | time | <timeline> [repetition]))
time of <clock:resource{minecraft:world_clock}> (same sub-commands, no gametime)
```
gamemasters. Without `of`, the dimension's default clock (`minecraft:overworld` in the overworld, `minecraft:the_end` in the end; none in the nether -> error). `set <time>` = total ticks of the clock; `set noon|day|night|midnight` (time markers, resource locations of the clock; sets to the *next* occurrence) [V `set noon` -> 6000]. Results [V]: `set`/`add` -> clock total elapsed ticks (e.g. 1000, 1074), `query time` -> elapsed ticks, `query gametime` -> game time (unaffected by clocks), `query day` (timeline `minecraft:day`) -> ticks into the 24000-tick period, `query day repetition` -> #completed repetitions. `time rate` multiplies clock speed only (not simulation like `tick rate`). `time set X` when already at X, `pause` when paused, `resume` when running, `rate` unchanged -> command error [W]. `time query daytime` no longer exists (the timeline `minecraft:daytime` is not defined: error "Can't find element 'minecraft:daytime' of type 'minecraft:timeline'" [V]); `time query day` is the timeline `minecraft:day`. In the nether (no default clock) `time` without `of` fails: "There is no default clock in dimension minecraft:the_nether" [V]; `minecraft:the_nether` is not a world clock either [V]. Time markers like `day`, `noon`, `night`, `midnight` come from the clock's definition.

### 8.4 `/swing` (26.1, extended 26.3) [G][V]
```
swing [<targets:entity{multiple,entities}>] [(mainhand|offhand) [<animation:whack|stab|none> [<duration:time{min=1}>]]]
```
Defaults `@s`, mainhand, `whack`, `6t` (26.3 added animation and duration; hand is required before animation). Result = #entities. Succeeds even when the entity type can't visually swing (client decides; mannequins/armor stands show it). On players it no longer resets attack strength (26.3). `duration 0` -> parse error [V].

### 8.5 `/rotate`, `/posteffect`, `/dialog`, `/version`, `/waypoint`, `/fetchprofile`, `/test`, `/publish`/`/unpublish`
* `rotate <target:single entity> (<rotation> | facing <pos:vec3> | facing entity <e> [<anchor>])` (1.21.2; gamemasters since pre1) - sets rotation without moving; `rotate @e[tag=a] 0 0` multi-target -> parse error; result 1 [V].
* `posteffect add|remove <players> <id>`, `clear <players>`, `list <player>` (26.3 snap3): client-side post-processing shaders from `assets/<ns>/post_effect/*.json` (vanilla: `minecraft:blur creeper entity_outline invert spider`); server cannot verify the effect exists; result = #players (list: #effects). Players only.
* `dialog show <players> <dialog>` / `dialog clear <players>` (1.21.6): `<dialog>` = registry id (`minecraft:dialog`, data pack `data/<ns>/dialog/*.json`) or inline SNBT dialog (e.g. `{type:"minecraft:notice",title:"Hello"}`); built-ins `minecraft:server_links`, `minecraft:custom_options`, `minecraft:quick_actions`. Fails if target set contains non-players. Result = #players. Buttons can `run_command` (client sends the command as the player => runs at the player's permission, may need confirmation) or `custom` events. Use with `/trigger` for non-op input.
* `waypoint list` / `waypoint modify <entity> color (<team_color> | hex <hex_color> | reset)` / `style (set <rl> | reset)` (1.21.6; locator bar). Results 0 / #waypoints.
* `fetchprofile (name <greedy> | id <uuid> | entity <entity>)` (1.21.9; `entity` added 26.2): async profile lookup printing a clickable `minecraft:profile` component.
* `test` (GameTest framework; gamemasters; available in normal gameplay since 1.21.5): `clearall [radius] | clearthat | clearthese | create <id> [w [h d]] | locate <tests> | pos [var] | resetclosest|resetthat|resetthese | run <tests> [times [untilFailed [rotationSteps [testsPerRow]]]] | runclosest|runthat|runthese [times [untilFailed]] | runfailed ... | runmultiple <tests> [amount] | stop | verify <tests>` (`export*` only in IDE). Not needed by a compiler.
* `publish [<allowCommands:bool> [<port 0..65535>]]` (owners, integrated server only) - 26.3 **removed the `gamemode` argument**; `unpublish` (26.2, owners). In 26.2 the Open-to-LAN screen can no longer enable commands for the host.

### 8.6 Other command changes that touch codegen (1.21.5 -> 26.3 summary)

| Version | Change |
|---|---|
| 1.21.5 | Text components are SNBT (25w02a); component args resolved against `@s` (25w05a); `team=` selector also hits non-living; heterogeneous lists + no empty-key paths in `data` (25w09a); `/test` in gameplay |
| 1.21.6 | `/version` (25w15a), `/waypoint`, `/dialog` (25w20a), `datapack create`, `locator_bar` rule |
| 1.21.9 | `/fetchprofile`; `setworldspawn`/`spawnpoint` take pitch and work in any dimension; server commands default to the respawn dimension; spawn chunks removed (rule `spawn_chunk_radius` gone); `summon` of monsters fails in Peaceful; `allow_entering_nether_using_portals`, `spawn_monsters`, `pvp`, `command_blocks_work`, `spawner_blocks_work` rules |
| 1.21.11 | `/stopwatch` + `execute if|unless stopwatch`; `slot_source` argument/registry; **all game rules moved to a registry and renamed to snake_case ids (`gamerule doDaylightCycle` no longer parses)**; `worldborder` time args in ticks (`s`,`d` suffixes); resource suggestions in any namespace |
| 26.1 | `/swing`; `/time` rebuilt on world clocks (returns elapsed ticks; `of`, `pause`, `resume`, `rate`, `query <timeline>`); game rules screen in-game; (Java 25 required) |
| 26.2 | `/unpublish`; team color arg lowercase-underscore only; `execute on owner` for vexes; advancement/revoke output counts; `chat-spam-threshold-seconds`/`command-spam-threshold-seconds`; `fetchprofile entity`; entity predicate keys renamed to identifiers/`minecraft:entity_type` and sub-predicates moved to top-level `minecraft:type_specific/...` (affects `@e[predicate=]` data packs and inline `execute if predicate`) |
| 26.3 | `/compute`, `data modify ... compute`, `/posteffect`; `execute if|unless slots`; `if items`/`/item` take `slot_source`; `/item fill|override` + new `replace` semantics; predicates: `condition`->`type`, `value_check` -> `int_value_check`/`float_value_check`; loot functions `function`->`type`, `conditions`->`condition`; number providers split into `context_int_provider`/`context_float_provider` (data pack folders `context_int_provider/`, `context_float_provider/`, `slot_source/`); `swing` animation+duration; `team join|leave` exact counts; `tellraw` nbt-resolve limit 64k; `give`/`unpublish`/`tick step...` signal real errors; `/publish` lost `gamemode`; `spreadplayers` uses `#entities_can_teleport_to`; `place feature` accepts inline features; data pack format 121.0 / resource pack 97.1 |

---

## 9. Game rules (`/gamerule`) in 26.3

Mechanism (changed in 1.21.11 / 25w44a): game rules are a **registry** (`minecraft:game_rule`, 59 entries in 26.3 [G]); ids are snake_case resource locations; the command accepts the short or namespaced form (`gamerule pvp` and `gamerule minecraft:pvp` both work [V]); legacy camelCase names (`doDaylightCycle`) no longer parse ("Incorrect argument for command") [V]. Syntax [G]: `gamerule <rule> [<value>]`, value is `bool` or `integer` with the per-rule limits below; omit value to query. Out-of-range -> parse error ("Integer must not be less than 0: found -1", "must not be more than 8") [V]. Result = current/new value (bool as 1/0, ints as-is) [V: `pvp`->1, `random_tick_speed 7`->7]. Setting an unchanged value: the 26.3 fix list still contains MC-174958 "gamerule succeeds when the value is not changed" (observed behaviour: succeeds). `gamerule` is gamemasters. The old `gameLoopFunction` is long gone. Rule values are stored per world in `level.dat`/`data/game_rules.dat` (**UNVERIFIED** location) and can be set at world creation and in the in-game World Options screen. The generic datapack way to read a rule from a function: none except `gamerule <name>` result via `execute store result`.

Defaults verified by querying a fresh 26.3 world [V] (type = bool unless noted):

| Rule | Default | Type / range | Meaning |
|---|---|---|---|
| `advance_time` | true | bool | day/night + moon phases advance (formerly `doDaylightCycle`) |
| `advance_weather` | true | bool | weather changes naturally (`doWeatherCycle`) |
| `allow_entering_nether_using_portals` | true | bool | nether portals usable |
| `block_drops` | true | bool | blocks drop items (`doTileDrops`) |
| `block_explosion_drop_decay` | true | bool | block-explosion loot may decay |
| `command_block_output` | true | bool | command blocks notify admins |
| `command_blocks_work` | true | bool | command blocks enabled (`enableCommandBlocks`) |
| `drowning_damage` | true | bool | |
| `elytra_movement_check` | true | bool | server checks elytra speed (inverted old `disableElytraMovementCheck`) |
| `ender_pearls_vanish_on_death` | true | bool | |
| `entity_drops` | true | bool | non-mob entity drops (`doEntityDrops`) |
| `fall_damage` | true | bool | |
| `fire_damage` | true | bool | |
| `fire_spread_radius_around_player` | 128 | int, min -1 | max distance fire spreads from a player; 0 off; -1 unlimited (replaces `doFireTick`) |
| `forgive_dead_players` | true | bool | |
| `freeze_damage` | true | bool | |
| `global_sound_events` | true | bool | |
| `immediate_respawn` | false | bool | |
| `keep_inventory` | false | bool | |
| `lava_source_conversion` | false | bool | |
| `limited_crafting` | false | bool | |
| `locator_bar` | true | bool | |
| `log_admin_commands` | true | bool | |
| `max_block_modifications` | 32768 | int, min 1 | max blocks changed by fill/clone/fillbiome (`commandModificationBlockLimit`) |
| `max_command_forks` | 65536 | int, min 0 | max contexts/forks per tick (`maxCommandForkCount`) |
| `max_command_sequence_length` | 65536 | int, min 0 | max commands per execution source per tick (`maxCommandChainLength`) |
| `max_entity_cramming` | 24 | int, min 0 | |
| `max_minecart_speed` | 8 | int 1..1000 | **experiment-gated** (Minecart Improvements): rejected by the command on a default world [V]; in the tree/registry [G] |
| `max_snow_accumulation_height` | 1 | int 0..8 | |
| `mob_drops` | true | bool | `doMobLoot` |
| `mob_explosion_drop_decay` | true | bool | |
| `mob_griefing` | true | bool | |
| `natural_health_regeneration` | true | bool | |
| `player_movement_check` | true | bool | server checks player speed (inverted `disablePlayerMovementCheck`) |
| `players_nether_portal_creative_delay` | 0 | int, min 0 | |
| `players_nether_portal_default_delay` | 80 | int, min 0 | |
| `players_sleeping_percentage` | 100 | int, min 0 | |
| `projectiles_can_break_blocks` | true | bool | (26.3: adventure-mode players' projectiles need `can_break`) |
| `pvp` | true | bool | |
| `raids` | true | bool | inverted `disableRaids` |
| `random_tick_speed` | 3 | int, min 0 | |
| `reduced_debug_info` | false | bool | |
| `respawn_radius` | 10 | int, min 0 | (`spawnRadius`) |
| `send_command_feedback` | true | bool | |
| `show_advancement_messages` | true | bool | (`announceAdvancements`) |
| `show_death_messages` | true | bool | |
| `spawn_mobs` | true | bool | natural spawning (`doMobSpawning`) |
| `spawn_monsters` | true | bool | |
| `spawn_patrols` | true | bool | |
| `spawn_phantoms` | true | bool | (`doInsomnia`) |
| `spawn_wandering_traders` | true | bool | |
| `spawn_wardens` | true | bool | |
| `spawner_blocks_work` | true | bool | |
| `spectators_generate_chunks` | true | bool | 26.2: respawning as spectator in Hardcore no longer turns it off |
| `spread_vines` | true | bool | (`doVinesSpread`) |
| `tnt_explodes` | true | bool | |
| `tnt_explosion_drop_decay` | false | bool | |
| `universal_anger` | false | bool | |
| `water_source_conversion` | true | bool | |

Removed rules (relative to 1.21.5): `doFireTick`, `allowFireTicksAwayFromPlayer`, `spawnChunkRadius`, `disableRaids` etc. (renamed/inverted). Gamerules that a data pack author typically reads/sets: `send_command_feedback`, `log_admin_commands`, `command_block_output`, `max_command_sequence_length`, `max_command_forks`, `random_tick_speed`, `advance_time`.

---

## 10. Full Java command list in 26.3 (from `commands.json`) [G]

`advancement attribute ban ban-ip banlist bossbar clear clone compute damage data datapack debug defaultgamemode deop dialog difficulty effect enchant execute experience fetchprofile fill fillbiome forceload function gamemode gamerule give help item jfr kick kill list locate loot me msg op pardon pardon-ip particle perf place playsound posteffect publish random recipe reload return ride rotate save-all save-off save-on say schedule scoreboard seed setblock setidletimeout setworldspawn spawnpoint spectate spreadplayers stop stopsound stopwatch summon swing tag team teammsg teleport tell tellraw test tick time title tm tp transfer trigger unpublish version w waypoint weather whitelist worldborder xp`
(aliases: `tell`,`w` = `msg`; `tm` = `teammsg`; `tp` = `teleport`; `xp` = `experience`). Not in the tree on a normal dedicated server (registered only in dev/IDE or via debug flags): `warden_spawn_tracker`, `debugpath`, `debugmobspawning`, `raid`, `serverpack`, `spawn_armor_trims`, `debugconfig` [W]. No command was removed between 1.21.5 and 26.3; added: `version`, `waypoint`, `dialog`, `fetchprofile`, `stopwatch`, `swing`, `unpublish`, `posteffect`, `compute` (+ `test` became public, `rotate` existed since 1.21.2).

---

## 11. Codegen cheat-sheet (derived from the above)

1. **Constants/variables**: fake players `#name` in a `dummy` objective (objective names: `[A-Za-z0-9_.+-]`, any length). Int32 wraparound semantics are identical to Java `int` for `+ - *` (use `add/remove/operation`), `/` and `%` are **floor** division/modulo (differs from C for negatives), div/mod by zero is a runtime command failure (use `execute unless score #d v matches 0 run ...` guard). `add`/`remove` amounts must be >= 0: lower negative adds to `remove`, or use `operation`/`set`.
2. **Load constant `K`**: `scoreboard players set #k obj K` (parse error if outside int32); the `result` of `set` is `K` (so don't use it to store 0).
3. **Comparison**: `execute if score A o <|<=|=|>|>= B o`, `matches <range>`; missing scores make *both* `if` and `unless =` forms behave like "unset"; init everything.
4. **Boolean materialisation**: `execute store success score #t o if <cond>` (0/1, safe for unloaded positions: failure stores 0).
5. **Functions as procedures/expressions**: return value via `return <n>`/`return run ...`; callers `execute store result score #r o run function ns:f`. Void functions leave the stored target untouched. Early exit with `execute if ... run return ...`. Arguments via storage + macros (`function ns:f with storage ns:args`, `$(key)`) or via fake-player scores.
6. **Loops**: recursion with a termination condition (budget `max_command_sequence_length`, default 65536 commands/tick) or `schedule function ... 1t` to continue next tick (`1t` inside tick = same tick, inside schedule = next tick); every function runs in one tick.
7. **Lists/dicts**: command storage (`data modify storage`), paths with `[index]`, `[-1]`, `[{filter}]`; length = `execute store result score #n o run data get storage ns:x list`. `append`/`insert -1` for push; `remove ... [-1]` for pop; there is no direct "swap".
8. **Floats**: only via `/compute` (float providers) or `data modify ... compute`, or fixed-point scale trick with `execute store ... <type> <scale>` and `data get ... <scale>`.
9. **Permission**: generated functions must only use commands at or below `function-permission-level` (default 2) - notably **avoid `/tick`, `/debug` etc. in functions**. Everything in this file except `tick`, `debug` is level 2/0.
10. **Selectors in generated code**: always `@e[type=...]` or tag-scoped; `@s` only valid with an executor; guard `execute as` with `at @s` when position matters; use unique tag per compiler-managed entity.
11. **Reload hazards**: any bad non-macro line in a function file rejects the file; macro lines fail at call time.

---

## 12. Open uncertainties

* Exact names/handling of permission level 1 (`moderators`) and whether `ops.json`/server.properties changed - no source found (only numeric levels documented).
* `seed` permission for singleplayer without cheats (tree says gamemasters).
* Whether chained `execute store` into the same objective evaluates in order; and exact `store ... bossbar max 0` handling.
* Real-player-only behaviours (`/trigger`, `title`, `data ... entity <player>`, selectors `level/gamemode/advancements`) were not exercised on a live client.
* `datapack disable vanilla` breakage cause; unquoted `/` in pack names.
* `/random` upper range edge; `kill` experience/drop details; performance claims for inline `/compute` parsing.
* The wiki pages for `Argument_types`, `Commands/setworldspawn`, `Commands/gamerule`, `Number_provider` were stale/incomplete for 26.3 in places; the tree/parsers here come from `commands.json` ([G]) which is authoritative.
* Number-provider field names for `length`, `binomial`, `weighted_list`, `number_dispatcher`, `conditional`, `environment_attribute`, `enchantment_level` are from the wiki only.
