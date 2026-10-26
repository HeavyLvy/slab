# 01 - Data pack structure and functions (Minecraft Java 26.3)

Target: Minecraft Java Edition **26.3** ("Wilderness Bound", 2026-09-15). Data pack format **121.0**, resource pack format 97.1.

Research notes: compiled from minecraft.wiki pages (fetched through a summarizing tool, so exact wording may be slightly paraphrased) plus version pages. The misode technical changelog is a JS app and could not be scraped; everything was cross-checked against the wiki version pages instead. Anything not directly confirmed is marked **UNVERIFIED**. Note 26.4 snapshots already exist (as of 2026-10); this document is about 26.3 only.

---

## 1. pack.mcmeta

### 1.1 Format (since 1.21.9 / 25w31a)

`pack.mcmeta` is the only mandatory file; it must be in the root of the pack folder or zip. Fields under the top-level `"pack"` object:

| Field | Required | Meaning |
|---|---|---|
| `description` | yes | Text component (plain string, or JSON text component object). Shown in the Data Packs screen and `/datapack list`. |
| `min_format` | yes (since 1.21.9) | Lowest supported pack format. |
| `max_format` | yes (since 1.21.9) | Highest supported pack format. |
| `pack_format` | no | Only needed if the pack must also work on pre-1.21.9 versions (data pack format < 82). |
| `supported_formats` | no | Removed from the format in 25w31a; only for old-version compat; must be absent otherwise. |

Top-level (siblings of `pack`): `features`, `filter`, `overlays` (all optional). `language` is resource-packs only.

### 1.2 How the [major, minor] numbers are written

Since 1.21.9 formats have a minor version. Minor is bumped for non-breaking changes; major for breaking ones.

- A single integer `N` or `[N]` means `[N, 0]` when used as `min_format`.
- A two-element list `[major, minor]` is a full version, e.g. `[88, 0]`.
- For `max_format`, a single integer means "any minor of that major" (minor becomes `0x7fffffff`).
- The wiki's tables print them as decimals (`101.1`, `107.1`, `121.0`) but in JSON you write arrays (or a bare integer).

Data pack format table:

| Format | Versions |
|---|---|
| 48 | 1.21 - 1.21.1 |
| 57 | 1.21.2 - 1.21.3 |
| 61 | 1.21.4 |
| 71 | 1.21.5 |
| 80 | 1.21.6 |
| 81 | 1.21.7 - 1.21.8 |
| 88.0 | 1.21.9 - 1.21.10 |
| 94.1 | 1.21.11 |
| 101.1 | 26.1 - 26.1.2 |
| 107.1 | 26.2 |
| 121.0 | 26.3 |

(Snapshot values seen along the way: 92.0 = 25w44a, 95.0 = 26.1 snapshot 1, 99.0 = 26.1 snapshot 6; 26.3 snapshots were 108.0 - 120.0.)

### 1.3 Ready-to-copy example for 26.3

```json
{
  "pack": {
    "description": "My slab-compiled data pack",
    "min_format": [121, 0],
    "max_format": [121, 0]
  }
}
```

Recommendation for the compiler: emit `"min_format": [121, 0]` and `"max_format": 121` (bare int = any minor, so the pack keeps loading if 26.3.x bumps the minor). Do **not** emit `pack_format` or `supported_formats` for a 26.3-only target. **UNVERIFIED**: whether emitting a `max_format` of `[121, 0]` rejects a hypothetical 121.1 game; per the wiki semantics a bare integer is the safe choice. Known bug MC-310337: the log reports an incorrect minimum format when `pack_format` is used; irrelevant if you don't emit it.

Old-version compatibility form (not needed for slab):

```json
{ "pack": { "description": "x", "pack_format": 48,
  "supported_formats": { "min_inclusive": 42, "max_inclusive": 90 },
  "min_format": 42, "max_format": 90 } }
```

### 1.4 overlays

```json
{
  "pack": { "description": "x", "min_format": [88, 0], "max_format": 121 },
  "overlays": {
    "entries": [
      { "directory": "overlay_a", "min_format": 88, "max_format": 90 },
      { "directory": "overlay_b", "min_format": [89, 2], "max_format": 121 }
    ]
  }
}
```

- `entries` ordered; earlier applied first, later override. Overlay directory sits in the pack root and contains a `data/` folder that is layered on top of the base contents when the game's format is within `[min_format, max_format]`.
- `directory` chars: a-z, 0-9, `_`, `-`, `.` (wiki history section also mentions A-Z; inconsistent, use lowercase).
- `formats` key (old-style) only for old-version compat; not allowed otherwise.
- `pack.mcmeta` and `pack.png` inside an overlay directory are ignored.
- Added in 1.20.2 (23w31a); overlays syntax updated in 1.21.9 to min/max_format.

### 1.5 filter

```json
"filter": {
  "block": [
    { "namespace": "examplens", "path": "^worldgen/.*" },
    { "path": "^recipe/old_.*" }
  ]
}
```

Each entry has optional `namespace` and `path` **regular expressions** (omitted = match all). Matching files from packs loaded **before** this one are treated as absent. Does not affect the pack containing the filter. (Added 1.19, 22w11a.) Note the `path` is relative to `data/<namespace>/` and starts with the registry folder (e.g. `recipe/...`).

### 1.6 features (experimental feature flags)

```json
"features": { "enabled": ["minecraft:example_flag"] }
```

A pack that needs a flag not enabled cannot be loaded via `/datapack enable` ("Pack `<name>` requires enabling a feature flag that is not enabled via data pack when creating world"). Feature flags can only be enabled through a data pack when **creating** the world. Packs using experimental settings can't be played on Realms and need a world re-open rather than `/reload` (experimental features use dynamic registries). Not relevant to a basic slab pack.

### 1.7 Other

- Non-ASCII UTF-8 allowed directly in the JSON (since 22w46a).
- `pack.png` optional, in pack root, shown in selection screen (since 1.16.2).

Sources: https://minecraft.wiki/w/Pack.mcmeta , https://minecraft.wiki/w/Pack_format , https://minecraft.wiki/w/Data_pack , https://minecraft.wiki/w/Java_Edition_26.3 (121.0), https://minecraft.wiki/w/Java_Edition_26.2 (107.1), https://minecraft.wiki/w/Java_Edition_26.1 (101.1)

---

## 2. Directory layout (26.3)

```
<pack>/
  pack.mcmeta
  pack.png                       (optional)
  data/
    <namespace>/
      function/                  *.mcfunction
      tags/
        function/                load.json / tick.json etc.
        <registry>/...           see 2.3
      advancement/
      loot_table/
      predicate/
      item_modifier/
      recipe/
      structure/                 *.nbt
      enchantment/
      ...
```

Both `<registry folder>` and `<path>` may contain subfolders. A file `data/<ns>/<registry>/<path>.json` has ID `<ns>:<path>`.

### 2.1 Folders under `data/<namespace>/` (per the wiki Data_pack page; wiki marks many as "experimental" = dynamic registry that requires a world reopen, not `/reload`)

Reloadable/non-registry: `function`, `advancement`, `loot_table`, `predicate`, `item_modifier`, `recipe`, `structure`, `slot_source` (new 25w44a/26.x), `context_int_provider` and `context_float_provider` (new 26.3), `tags`.

Registries (dynamic): `banner_pattern`, `block_transformer` (new 26.3), `cat_variant`, `chat_type`, `chicken_variant`, `cow_variant`, `damage_type`, `decorated_pot_pattern`, `dialog`, `dimension`, `dimension_type`, `enchantment`, `enchantment_provider`, `frog_variant`, `instrument`, `jukebox_song`, `painting_variant`, `pig_variant`, `sulfur_cube_archetype` (new 26.2), `test_environment`, `test_instance`, `timeline`, `trade_set` (26.1), `trial_spawner`, `trim_material`, `trim_pattern`, `villager_trade` (26.1), `wolf_sound_variant`, `wolf_variant`, `world_clock` (26.1), `zombie_nautilus_variant`, and per 26.1 also `cat_sound_variant`, `pig_sound_variant` (and cow/chicken sound variants per the wiki history; **UNVERIFIED** exact folder names for those beyond cat/pig).

`worldgen/` subfolders, per the wiki Data_pack page: `biome`, `configured_carver`, `configured_feature`, `density_function`, `noise`, `noise_settings`, `placed_feature`, `processor_list`, `structure`, `structure_set`, `template_pool`, `world_preset`, `flat_level_generator_preset`, `multi_noise_biome_source_parameter_list`.

**26.3 changes to worldgen folders (from the 26.3 version page; the Data_pack page list has not caught up):**
- `worldgen/configured_feature` **moved to** `worldgen/feature`
- `worldgen/configured_carver` **renamed to** `worldgen/carver`
- NEW `worldgen/block_state_provider`
- NEW `worldgen/material_condition` and `worldgen/material_rule` (noise settings field `surface_rule` renamed `material_rule`)
- NEW registry `number_provider`, plus `context_int_provider` / `context_float_provider` (the latter two as data-pack folders).

(Folder name `trial_spawner` and `dimension`: dimension data files stay in `data/<ns>/dimension/` for datapack-defined dimensions; in the *world save* all default dimensions moved to a `dimensions/` subfolder in 26.1 - save-folder only, see section 9.)

### 2.2 Singular vs plural (the 1.21 rename)

Since 24w19a / 24w21a (1.21) folders are **singular**: `function` (was `functions`), `advancement` (was `advancements`), `loot_table` (`loot_tables`), `recipe` (`recipes`), `structure` (`structures`), `predicate` (`predicates`), `item_modifier` (`item_modifiers`). Tag folders: `item`, `block`, `entity_type`, `fluid`, `game_event` (were plural) and `function` (24w21a). Plural folders silently no longer load. Nothing reverted in 26.x; no further singular/plural renames seen. Exceptions: the `tags` folder itself stays plural.

### 2.3 Tag folders (`data/<ns>/tags/<folder>/`)

Vanilla tag registries: `banner_pattern`, `block`, `damage_type`, `dialog`, `enchantment`, `entity_type`, `fluid`, `function`, `game_event`, `instrument`, `item`, `painting_variant`, `point_of_interest_type`, `potion` (26.1), `timeline`, `villager_trade` (26.1), `worldgen/biome`, `worldgen/feature` (was `worldgen/configured_feature`, renamed in 26.3 snapshot 1; configured-feature tags added in 26.1-pre1), `worldgen/flat_level_generator_preset`, `worldgen/structure`, `worldgen/world_preset`. `cat_variant` tag folder unused but creatable. 26.1 snapshot 1 also added block sound set tags (**UNVERIFIED** details; the wiki marks it "upcoming 26.4" ambiguously).

Sources: https://minecraft.wiki/w/Data_pack , https://minecraft.wiki/w/Tag_(Java_Edition) , https://minecraft.wiki/w/Java_Edition_26.3 , https://minecraft.wiki/w/Java_Edition_26.2 , https://minecraft.wiki/w/Java_Edition_26.1

---

## 3. Namespaced IDs / resource locations

- Form `namespace:path`. Default namespace `minecraft` when omitted (`diamond` == `minecraft:diamond`).
- Allowed in both namespace and path: `0-9 a-z _ - .`. Uppercase **not** allowed (since 16w32a). `/` allowed in path only (directory separator), illegal in namespace.
- In 26.1 snapshot 6 `..` is no longer a valid namespace.
- Preferred style: snake_case.
- File mapping: `abc:run_game` -> `data/abc/function/run_game.mcfunction`. Tags referenced as `#namespace:path`.
- The `minecraft` namespace holds vanilla data; defining the same ID in your pack overrides it (last pack in order wins; tags merge unless `replace: true`).
- `/function`/`/datapack` etc. take function IDs as resource locations: `/function my_ns:sub/dir/name`.
- Namespaces for `/data ... storage` and scoreboard objective names follow different rules (objectives: arbitrary strings; storage: resource location).

Sources: https://minecraft.wiki/w/Resource_location , https://minecraft.wiki/w/Data_pack

---

## 4. .mcfunction file format and execution

### 4.1 File format

- Path `data/<ns>/function/<path>.mcfunction`. UTF-8 text.
- One command per line, **without leading `/`** ("one valid command is placed per line, without the usual forward slash"). A leading `/` is a parse error (**UNVERIFIED** precisely, but wiki says commands are written without it).
- Comments: a line starting with `#` (spaces/tabs may precede). `//` is not a comment. Blank lines are fine.
- Leading/trailing whitespace around a command is allowed.
- No length limit like the 32,500-char command-block limit: function commands may exceed it.
- **Line continuation** (1.20.2, 23w31a): a single `\` as the last non-whitespace character continues the command on the next line; leading and trailing whitespace of the following line is stripped before joining. So multi-line commands are supported via `\` and only that. Unchanged in 26.x (no 26.x history entry for it).
- Non-macro lines are parsed when the function loads/reloads; if any non-macro line fails to parse, **the whole function fails to load** (and the failing function makes the `/reload` log an error; **UNVERIFIED** whether the whole reload aborts - for syntax errors in functions the other datapack contents still load, the function is simply missing). Therefore the compiler must emit syntactically valid commands for 26.3.
- Functions are fully parsed and cached on load (17w45a). Use `/reload` after edits.

### 4.2 Limits and gamerules (names as of 26.x)

All game rules were moved to a registry and renamed to snake_case resource locations in 25w44a (so before 26.1 release). Command: `/gamerule <rule> [<value>]` (the `minecraft:` prefix is optional - **UNVERIFIED** against official source, one aggregator site says the prefix is optional; wiki shows unprefixed `max_command_sequence_length`).

| Old name | 26.x name | Default |
|---|---|---|
| `maxCommandChainLength` | `max_command_sequence_length` | 65536 |
| `maxCommandForkCount` | `max_command_forks` | 65536 |
| `commandModificationBlockLimit` | `max_block_modifications` | 32768 |
| `commandBlocksEnabled` | `command_blocks_work` | true |
| `sendCommandFeedback` | `send_command_feedback` | true |
| `commandBlockOutput` | `command_block_output` | true |
| `logAdminCommands` | `log_admin_commands` | true |

- `max_command_sequence_length`: "maximum length of a chain of commands that can be executed during one tick from one execution source" (min 0). Commands beyond the limit are ignored. Applies to command blocks and functions.
- `max_command_forks`: "maximum number of forks (contexts) that can be created during one tick" (min 1).
- A function called from a function (or `#tag`) counts its commands toward the single chain of its source. Known bug reports (from Mojang bug tracker, pre-1.20.3) say a nested function can be skipped if the queue is full while later parent commands still run; **UNVERIFIED** whether still the case.
- Recursion: **UNVERIFIED** - there is no explicit "depth" limit documented on the wiki since 1.20.3's queued execution rewrite; recursion is bounded by `max_command_sequence_length` (and the old stack-overflow risk was removed). Design the compiler to bound recursion itself.
- Many other gamerules were renamed too (e.g. `doDaylightCycle` -> `advance_time`, `keepInventory` -> `keep_inventory`, `disableRaids` -> `raids` with inverted value); list in https://minecraft.wiki/w/Java_Edition_25w44a . `gameLoopFunction` was removed long ago in favor of `#minecraft:tick`.

### 4.3 Execution model

- All commands in a function run in one tick, including nested function calls. (Since 1.20.3 execution is queue-based: **UNVERIFIED** details.)
- A function inherits the caller's context (executing entity, position, rotation, dimension, anchor). An `execute` modifies context only for its own chain.
- Permission level: singleplayer/LAN functions run at up to level 2; multiplayer uses `function-permission-level` in `server.properties`.
- Order within a tick (wiki): 1 advancement-invoked functions; 2 enchantment-invoked functions; 3 `#minecraft:tick` functions; 4 scheduled (`/schedule`) functions in queue order; 5 command-block / player-run functions. On datapack (re)load `#minecraft:load` runs first.
- `/schedule function <id> <time>`: functions run at tick time; note MC-254661: `/schedule clear` requires a namespace in its function argument.

### 4.4 `return`

- `return <value>` (32-bit int; ends function with success), `return fail` (ends with failure, value 0), `return run <command>` (ends function, takes that command's success/result).
- Stops the function at that line, value goes to the caller (`/function` or `execute if function`). With `execute if function`, nonzero = pass. A function with no `return` is void.
- `return run execute ...` always returns even if the execute fails; `execute ... run return run ...` does not return if the execute fails.
- After a forking `execute`, only the first branch runs before the return.
- Calling a function **tag** stops after the first `/return` in any function (23w44a). `/function`'s own result: success 1; value is sum of non-void return values (tag with multiple functions), except under `return run function` where the first non-void wins.
- `/function` no longer returns the count of commands executed (since 1.20.3 / 23w41a); `execute store ... run function` stores the function's `return` value.
- `/function` failure: function/tag doesn't exist, or macro args problem.

Sources: https://minecraft.wiki/w/Function_(Java_Edition) , https://minecraft.wiki/w/Game_rule , https://minecraft.wiki/w/Java_Edition_25w44a , https://minecraft.wiki/w/Commands/return , https://minecraft.wiki/w/Commands/function

---

## 5. Function macros

Added 1.20.2 (23w31a). No 26.x changes documented.

### 5.1 Syntax

- A macro line begins with `$` and contains `$(key)` placeholders:
  ```mcfunction
  $say Hello $(name), you have $(count) points
  $execute as @a[name=$(player)] run scoreboard players set @s pts $(count)
  ```
  A line without `$` is a normal line (any `$(...)` in it is not substituted). A function with at least one `$` line is a "macro function".
- Key characters: `a-z A-Z 0-9 _`.
- Calling:
  ```mcfunction
  function ns:foo {key_1:"Example String", key_2:10}        # inline SNBT compound
  function ns:foo with storage ns:args                      # root compound of storage
  function ns:foo with storage ns:args some.path            # path must select one compound
  function ns:foo with entity @s SelectedItem
  function ns:foo with block ~ ~ ~ some.path
  execute as @p run function ns:bar2 with entity @s SelectedItem
  ```
  Syntax: `function <name> <arguments>` or `function <name> with (block <pos>|entity <source>|storage <id>) [<path>]`. `<name>` may be `#tag`.
- Failure cases: plain `/function` on a function that has macro lines fails; a missing key fails the whole call (no commands of that function run); a macro line that becomes an unparseable command fails the whole function call. Extra keys in the compound are ignored. `with entity` fails if source doesn't resolve; `with storage` fails if no tag / not a compound / path selects more than one; `with block` fails if unloaded or not a block entity.

### 5.2 Substitution / number formatting

- Strings inserted raw, **without quotes** (so to put a string in a quoted context write the quotes in the macro line: `$tellraw @a {"text":"$(msg)"}`; embedding quotes/special chars in the string is your problem).
- Numbers: plain text, no type suffix (since 1.20.2 pre-release 1), max 15 fraction digits. So `10b` -> `10`, `1.5f` -> `1.5`. Beware: floats such as `1.0d` (double) text form is **UNVERIFIED** (probably `1.0` or `1`).
- `true`/`false` -> `1`/`0`.
- Lists, compounds, arrays: canonical SNBT text.
- Substitution is textual before parse: you may macro-ize anything in a command including selectors, coordinates, subcommands, even whole sub-commands (`$$(cmd)` style: `$$(command)` is a `$` line whose whole body is the value) - the line has to parse after substitution. **UNVERIFIED** that the wiki documents `$$(cmd)`; the mechanism follows from the textual rule.

### 5.3 Parsing, caching, performance

- Macro lines are parsed at call time with the substituted values, not at load time. Non-macro lines of a macro function are parsed at load time. (Internally vanilla caches instantiated macro functions keyed by argument values; **UNVERIFIED** size of the cache - recall ~8 entries.)
- Calling cost: every distinct argument set requires parsing, so avoid macros in hot paths (tick loops) where possible; compiler should use macros only when a value truly is dynamic.
- Limits on number of arguments / line length: **UNVERIFIED** (none documented).
- Cannot macro-ize the function ID of `/function` itself except via a `$function $(id)` line (it works since it is just text).

Sources: https://minecraft.wiki/w/Function_(Java_Edition) , https://minecraft.wiki/w/Commands/function

---

## 6. Function tags (`#minecraft:load`, `#minecraft:tick`)

- Files: `data/minecraft/tags/function/load.json` and `data/minecraft/tags/function/tick.json` (singular `function` since 1.21 / 24w21a).
- Format:
  ```json
  {
    "values": [
      "mypack:load",
      "#mypack:setup_tag",
      { "id": "othermod:init", "required": false }
    ]
  }
  ```
  `tick.json` example: `{"values": ["mypack:tick"]}`.
- `#minecraft:load` runs when the world/server loads and on every datapack reload (`/reload`, `/datapack enable|disable` which triggers a reload). It runs before any player has joined, so `tellraw`/`title` to players show nothing on world load. Runs before `#minecraft:tick` (since 22w46a).
- `#minecraft:tick` runs at the start of every game tick (see ordering in 4.3) - i.e. 20x/second.
- Tagged functions run in listed order; the same function listed twice runs only at its first occurrence. Multiple packs merge their tags (`replace: false`), in pack load order.
- Ordering between different packs' `load` functions follows pack order (lower priority pack first). **UNVERIFIED** whether the first-listed pack's functions run first.
- Because load runs on every reload, slab-generated load code must be idempotent (e.g. `scoreboard objectives add` is safe-ish: it errors silently if exists; initialise vars guarded).

Sources: https://minecraft.wiki/w/Function_(Java_Edition) , https://minecraft.wiki/w/Tag_(Java_Edition)

---

## 7. Tag file JSON format (general)

```json
{
  "replace": false,
  "values": [
    "namespace:object_id",
    "#namespace:other_tag",
    { "id": "namespace:object_id", "required": false }
  ]
}
```

- `replace` (optional bool, default `false`): `true` discards entries from lower-priority packs.
- `values` entries: object ID string; `#tag` string (same registry); or object `{ "id": <string either form>, "required": <bool> }`.
- `required` defaults `true`; if an entry marked `required:false` is missing, only it is skipped; otherwise a missing entry makes the tag fail to load.
- Path: `data/<ns>/tags/<registry folder>/<path>.json`; ID `<ns>:<path>`.
- 26.3: "Codec for element lists/tags now allows mixing inline values and references" (affects registries accepting inline-or-reference lists; not the tag file format).

Sources: https://minecraft.wiki/w/Tag_(Java_Edition) , https://minecraft.wiki/w/Java_Edition_26.3

---

## 8. Persistence: storage, scoreboards, reload

- **Scoreboard** (objectives, scores, teams, tags on entities): stored in `<world>/data/minecraft/scoreboard.dat` (path since 26.1 snapshot 6; previously `data/scoreboard.dat`). Persists across restarts and `/reload`. `/reload` never clears scores.
- **Command storage** (`/data ... storage <ns:id>`): stored per namespace in `<world>/data/<namespace>/command_storage.dat` since 26.1 snapshot 6 (previously `data/command_storage_<namespace>.dat`). Gzip NBT; root `data.contents.<path>`, plus `DataVersion`. Persists across restarts and `/reload`. Created on first write; empty storages are not kept.
- Gamerules: moved to a `game_rules` file in `data/` (26.1). Persist per world.
- Entities, tags, bossbars, etc. persist normally. Per-world state like `/schedule`d functions persist (stored in a scheduled-events file in `data/`; **UNVERIFIED** path/behavior across reload).
- `/reload`: reloads function files, tags, loot tables, recipes, advancements, predicates, item modifiers; re-runs `#minecraft:load`. If any data is invalid, the reload is not applied and the previous data is kept. Experimental (dynamic registry) data requires leaving and rejoining the world.
- The load order of datapacks is stored in `level.dat` (changes stored only after successful reload since 20w22a).
- Dimensions in the save folder: `<world>/dimensions/<ns>/<dimension>/...` (26.1 snapshot 6). Not relevant to authoring.

Sources: https://minecraft.wiki/w/Command_storage , https://minecraft.wiki/w/Scoreboard , https://minecraft.wiki/w/Java_Edition_26.1_Snapshot_6 , https://minecraft.wiki/w/Commands/reload , https://minecraft.wiki/w/Data_pack

---

## 9. Installing / enabling / zipping

- **Existing world:** put the folder or `.zip` in `.minecraft/saves/<world>/datapacks/`, then `/reload` (or reopen world). New packs appear in `/datapack list available` and must be `/datapack enable`d (packs dropped in `datapacks/` are auto-listed; for a world already open you may need `/reload` first; **UNVERIFIED** whether new-since-world-open packs auto-enable - normally they appear as available then need enabling).
- **New world:** Create New World -> More tab -> Data Packs (drag-and-drop a pack in).
- **Multiplayer server:** `<world>/datapacks/`; `/reload` or restart.
- **Names for /datapack:** folder/zip packs are `file/<name>` (e.g. `file/mypack.zip`), vanilla is `vanilla`; quote if the name has other characters. (**UNVERIFIED** from the wiki `file/` prefix page text, but this is long-standing behavior.)
- `/datapack enable <name> [first|last|before <pack>|after <pack>]`, `/datapack disable <name>`, `/datapack list [available|enabled]`. Permission level 2. Later packs have higher priority and override earlier ones. A built-in pack enabled at world creation cannot be disabled by command.
- **Zip rules:** a pack is a folder or a `.zip` containing `pack.mcmeta` **at the root of the zip** (zip the contents, not the containing folder). Use forward slashes and deflate/store; no encryption.
- `pack.png`: root, optional, shown in the screen (1.16.2+).
- `required_features`: not a pack.mcmeta key; the mechanism is `features.enabled` (section 1.6).
- Safe Mode (world fails to load) disables all packs except vanilla.

Sources: https://minecraft.wiki/w/Data_pack , https://minecraft.wiki/w/Commands/datapack

---

## 10. Changes in 26.1 / 26.2 / 26.3 relevant to data pack authors (structure)

### 26.1 (format 101.1; snapshots 92.0 -> 99.0 ...)
- All game rules in a registry with snake_case IDs (25w44a, format 92.0), e.g. `max_command_sequence_length`. Game Tests `game_rules` environment uses a `rules` map.
- New `slot_source` data folder (25w44a); new loot pool entry `minecraft:slots`.
- World save folder reorganised: `data/<namespace>/...` for scoreboard/command storage, default dimensions in `dimensions/`, structure-block output `generated/<ns>/structure` (26.1 snapshot 6, format 99.0).
- `world_clock` registry (`data/<ns>/world_clock/<id>.json`); `/time [of <clock>] set ...`; dimension_type `default_clock`, `has_ender_dragon_fight`.
- Data-driven villager trades (`villager_trade`, `trade_set` folders), loot function `set_random_dyes`/`set_random_potion`, number provider `sum`, loot context `villager_trade`.
- New sound variant registries (wolf `adult_sounds`/`baby_sounds`; cat, pig, cow, chicken sound variants).
- Many block tag renames/splits (`#dirt` split; `#bamboo_plantable_on` -> `#supports_bamboo`...). Potion tags, configured feature tags.
- `..` no longer a valid namespace.
- `/swing`, `/fetchprofile entity`.

### 26.2 (format 107.1)
- `sulfur_cube_archetype` registry; `sulfur_cube_content` component.
- Entity predicate overhaul: `type` -> `minecraft:entity_type`; type-specific predicates moved to top level with new names (`minecraft:type_specific/cube_mob`); unknown fields now **rejected** (not ignored); new `minecraft:entity_tags` predicate.
- Dripstone features renamed to speleothem; `weird_scaled_sampler` removed (use `interval_select`); `noise_gradient` surface rule reworked; many fields now accept single ID/list as well as tag.
- Tag renames (`#concrete_powder` -> `#concrete_powders`, etc.). Team colors only lowercase with underscores. `HurtByTimestamp` entity tag removed.

### 26.3 (format 121.0) - the large one
- Folder renames: `worldgen/configured_feature` -> `worldgen/feature`; `worldgen/configured_carver` -> `worldgen/carver`; tag folder `worldgen/configured_feature` -> `worldgen/feature`. New folders: `worldgen/block_state_provider`, `worldgen/material_condition`, `worldgen/material_rule`, `block_transformer`, `slot_source` (as files), `context_int_provider`, `context_float_provider`. Noise settings `surface_rule` -> `material_rule`.
- **Field renames across JSON formats** (affects any codegen of loot/predicate/advancement JSON): predicates `condition` -> `type`; loot functions `function` -> `type`; loot pool entries `conditions` -> `condition`, `functions` -> `modifier`; block state `Name`/`Properties` -> `id`/`properties`; `minecraft:reference` removed from loot functions/predicates/slot sources; `value_check` split into int/float; `block_state_property` replaced by `match_block`; number provider types renamed (e.g. `sum` -> `add`); `player_generates_container_loot` `loot_table` -> `loot_tables`; **a list of predicates is no longer accepted as shorthand for `all_of`**; cooking recipes' `cookingtime` is now required; brewing recipes data-driven (`minecraft:brewing`). (Exact form of every rename: see other docs / the 26.3 version page; **UNVERIFIED** completeness.)
- Tag renames: `#convertable_to_mud` -> `#convertible_to_mud`; some structure tags (`#on_woodland_explorer_maps` -> `#on_woodland_mansion_maps`).
- Commands: new `/compute <target> integer <int_provider>` (and float form, evaluated in `command_compute_*` contexts); `/data modify ... compute ...` source; `/execute ... slots` condition and `items` takes slot source; `/item` gains `fill`/`override`; `/place feature` allows inline definitions; `/publish` loses gamemode arg; `/posteffect` add/clear/list/remove; `/swing` animation args.
- New `minecraft:block_transformer` registry + data component of same name; `brewing_fuel` component replaces `#brewing_fuel` tag.
- New `invulnerable_time` entity tag; new environment attributes `gameplay/straw_bed_rule`, `gameplay/natural_mob_spawns`; `minecraft:dimension_origin` structure placement; vertical anchor `relative_to_sea_level`.
- Game rule note: adventure-mode projectiles + `projectiles_can_break_blocks`.
- Nothing found in 26.1 - 26.3 that changes `.mcfunction` syntax, macros, line continuation, `pack.mcmeta` structure, or `#minecraft:load`/`tick` file paths. **UNVERIFIED** only to the extent that the wiki text was truncated (26.3 page read in 100k-char chunks 0-400k; first chunk read by topic filter).
- Known issues listed at 26.3: MC-254661, MC-310337, MC-252369 (feature cycle may stop packs loading).

Sources: https://minecraft.wiki/w/Java_Edition_26.1 , https://minecraft.wiki/w/Java_Edition_26.2 , https://minecraft.wiki/w/Java_Edition_26.3 , https://minecraft.wiki/w/Java_Edition_25w44a , https://minecraft.wiki/w/Java_Edition_26.1_Snapshot_6 , https://misode.github.io/changelog/ (JS app; not scraped)

---

## 11. Open uncertainties (summary)

- Whether `/gamerule` accepts both `foo` and `minecraft:foo` (very likely yes).
- Exact recursion/depth and nested-function skipping semantics in the 1.20.3+ queued executor.
- Macro cache size, argument-count/line-length limits, double formatting.
- Exact folder names of cow/chicken sound variants; completeness of 26.3 JSON field renames.
- `/schedule` persistence across reload.
