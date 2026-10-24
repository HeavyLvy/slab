# Version deltas: Java 1.21.4 (pack format 61) to 26.3 (data pack format 121.0)

Research date: 2026-10-22. Target: Java Edition **26.3 "Wilderness Bound"** (released 2026-09-15, data pack format `121.0`, resource pack format `97.1`, data version 5023, protocol 777, Java 25 required).

Methodology and trust levels:
- Primary source: the full text of misode's technical-changes repo (https://github.com/misode/technical-changes, rendered at https://misode.github.io/changelog/). Cloned and read file by file for 1.21.5, 1.21.6, 1.21.7, 1.21.9, 1.21.11, 26.1, 26.2, 26.3 (and 26.4 snapshot 1). Last commit in that repo: 2026-09-24. The repo only contains snapshots/pre-releases that had technical changes; changes marked `obsolete` there were superseded within the same cycle and are NOT listed here unless they matter.
- Secondary: minecraft.wiki pages (Pack_format, Data_pack, Game_rule, Server.properties, Commands/time, Commands/execute, Text_component_format, Function_(Java_Edition), Java_Edition_1.21.5 / 1.21.6 / 1.21.9 / 1.21.11 / 26.1 / 26.2 / 26.3). WebFetch only summarizes each page, so wiki facts are second-hand and some pages are flagged "needs update"; the 26.3 page was only read up to char 400000 of 472815.
- Anything not confirmed by one of those is tagged **UNVERIFIED**.
- Slab-relevant caveat: the misode changelog uses internal snapshot naming. A feature "added in 25w44a" is simply "in 1.21.11" for the final release; I give the final-release view and mention snapshot only for provenance.

---

## 0. Top 25 things to know when targeting 26.3

1. **Pack metadata is `min_format`/`max_format`**, not `pack_format`. Minimal 26.3 `pack.mcmeta`: `{"pack":{"description":"...","min_format":121,"max_format":121}}` (a bare int means `[N,0]`; a full version is `[major,minor]`). `pack_format` is optional (only needed if the pack claims support for data pack format < 82); `supported_formats` and overlay `formats` are removed (illegal unless the range reaches below 82). Minor versions are backwards compatible: 26.3 = `121.0`; a later `121.x` must still load a `[121,0]` pack (inferred from 25w31a notes; **UNVERIFIED** for the exact comparison rule).
2. **Text components are NBT/SNBT, not JSON strings** (since 1.21.5). `/tellraw @a {text:"hi",color:"red"}`, `custom_name={text:'x'}`. Registry-style keys are snake_case: `hover_event`, `click_event`. Raw JSON-string form is gone from commands and item components.
3. **Click/hover event fields renamed** (1.21.5): `run_command` -> `command` (leading `/` optional), `open_url` -> `url`, `change_page` -> `page` (positive int), `show_text` -> `value`, `show_item` inlined (`id`,`count`,`components`), `show_entity` -> `id`,`uuid`,`name`. New click events: `custom` (id + NBT payload), `show_dialog`. `copy_to_clipboard` still uses `value`.
4. **Game rules are a registry with snake_case resource-location IDs** (1.21.11): `doDaylightCycle` -> `advance_time`, `keepInventory` -> `keep_inventory`, etc. Three rules had their meaning **inverted** (`raids`, `elytra_movement_check`, `player_movement_check`). `doFireTick` is gone (replaced by integer `fire_spread_radius_around_player`). `/gamerule` examples on the wiki use bare names (`/gamerule advance_time false`); both `gamerule advance_time` and `gamerule minecraft:advance_time` are accepted (verified on a live 26.3 server, see 02-commands-core.md section 9).
5. **Time is clock-based** (26.1): `/time [of <clock>] set|add|pause|resume|rate|query ...`; `world_clock` and `timeline` registries; named times (`day`, `noon`, ...) are *time markers* defined by data. `/time set`/`add` now return total elapsed ticks of the clock, not time of day. Dimension types have `default_clock`; in dimensions without a default clock, `/time` without `of` errors.
6. **Environment attributes** (1.21.11) replaced many dimension-type and biome-effect fields and several biome tags (`#snow_golem_melts`, `#increased_fire_burnout`, `#without_patrol_spawns`, ...). Dimension type `natural`, `ultrawarm`, `piglin_safe`, `has_raids`, `bed_works`, `respawn_anchor_works`, `fixed_time`, `effects`, `cloud_height` are removed/migrated.
7. **Entity predicate format is component-map style** (26.2): keys are identifiers (`minecraft:effects`, `minecraft:entity_type`, `minecraft:type_specific/player`), unknown keys are rejected, and `type` -> `minecraft:entity_type`. Bare legacy names still resolve (default namespace `minecraft`), but old nested `type_specific:{type:...}` is gone.
8. **Loot/predicate/modifier/advancement JSON schema overhaul** (26.3): predicates use `"type"` (not `"condition"`), item modifiers use `"type"` (not `"function"`) and `"condition"` (singular, never a bare list), pool fields `conditions`->`condition`, `functions`->`modifier`; bare-list shorthand removed (must use explicit `all_of`); `reference` predicate/modifier removed (just use the ID string); `block_state_property` -> `match_block`; number providers must always carry `type`.
9. **Number providers split in two registries** (26.3): `context_int_provider` and `context_float_provider`, with new arithmetic (`add`,`sub`,`mul`,`div`,`mod`,`pow`,`min`,`max`,`avg`,`abs`,...). New `/compute` command and `/data modify ... compute` evaluate them.
10. **Slot sources** (1.21.11, files in 26.3): `slot_source` registry/folder; `/item replace|fill|override`, `/item ... from entity` multi-entity, `/execute if|unless slots`, and `/execute if|unless items` all accept slot-source references or inline definitions.
11. **`/item replace` semantics changed** (26.3): excess destination slots are ignored and excess source items are ignored (no error). New `fill` (repeat) and `override` (clear leftover) sub-commands.
12. **Block-state NBT/JSON shape changed** (26.3): `{Name,Properties}` -> `{id,properties}`; a default state may be a bare block-ID string. (Command-syntax block states like `stone[facing=north]` are unaffected, **UNVERIFIED** but implied.)
13. **Data components on items replaced many NBT fields; tooltip hiding moved to `tooltip_display`** (1.21.5): `show_in_tooltip` fields and `hide_tooltip`/`hide_additional_tooltip` removed. Several components got flattened forms (`enchantments={sharpness:2}`, `dyed_color=12345`, `attribute_modifiers=[...]`, `can_place_on={blocks:'stone'}`).
14. **Entity NBT consolidation** (1.21.5): `ArmorItems`/`HandItems`/`body_armor_item` -> `equipment:{head,chest,legs,feet,mainhand,offhand,body,saddle}`; `*DropChances` -> `drop_chances`; `Pos`-style triplets (`TileX/Y/Z`, `SleepingX/Y/Z`, `HomePosX/Y/Z`, ...) -> single array fields; player spawn -> `respawn:{pos,yaw,pitch,dimension,forced}`; `FallDistance` -> `fall_distance` (double).
15. **`/setblock` and `/fill` block-entity data rules changed** (1.21.5): omitted data is preserved, specified data overwrites, `{}` clears. New `strict` option (no block updates) on `/fill`, `/clone`, `/setblock`, `/place template`.
16. **SNBT grammar expanded** (1.21.5): heterogeneous lists, hex/binary/underscore/scientific numbers, `s`/`u` signed/unsigned suffixes, unicode escapes, trailing commas, `bool(..)`, `uuid(..)`. Leading-zero integers and unquoted strings starting with a digit/`.`/`+`/`-` are now errors. `/data` can no longer traverse empty-key paths.
17. **New commands since 1.21.4**: `/test` (1.21.5), `/waypoint`, `/version`, `/datapack create`, `/dialog` (1.21.6), `/fetchprofile` (1.21.9, `entity` mode 26.1), `/stopwatch` + `execute if stopwatch` (1.21.11), `/swing` (26.1), `/unpublish` (26.2), `/posteffect`, `/compute`, `execute if slots` (26.3). **Removed/changed**: `/publish` lost its `gamemode` arg (26.3), `/worldborder` time arg now ticks (1.21.11).
18. **Dialogs** (1.21.6): server-side UI JSON in `dialog` folder; `/dialog show|clear`; dialog tags `pause_screen_additions`, `quick_actions`; `command_template` submit uses macros and shows a confirmation if the resulting command needs permission > 0.
19. **World border is per-dimension** (1.21.9) and `/worldborder` timing runs on game ticks (1.21.11; default unit ticks, suffix `s`/`d` allowed).
20. **Server properties moved to game rules** (1.21.9): `allow-nether`, `spawn-monsters`, `enable-command-block`, `pvp` removed from `server.properties`. `white-list` defaults to `true` (26.3). `function-permission-level` still exists (default 2).
21. **Permissions**: numeric levels 0-4 are unchanged in `ops.json`/`server.properties`, but the command tree now expresses them as *named* levels (`gamemasters`=2, `admins`=3, `owners`=4; verified from the 26.3 server's `commands.json`, see 02-commands-core.md section 1). No rework of the level semantics was found in the changelogs. `ops.json` still has `level` 0-4 and `op-permission-level`/`function-permission-level` remain. What did change: command report nodes carry `required_level` (1.21.6); `run_command` click events needing level > 0 or unparsable require user confirmation (1.21.6); sign click events no longer run commands unless `allow_op_features` is set (26.3). See section "Permissions" below.
22. **World save layout changed** (26.1): `dimensions/<ns>/<id>/`, `players/`, namespaced `data/<ns>/...`, `game_rules.dat`, `scoreboard.dat` -> `data/minecraft/scoreboard.dat`, command storage in namespace subfolders, `level.dat` slimmed. Matters for any tooling that reads worlds directly.
23. **Worldgen JSON got heavy breaking changes in 26.3**: `worldgen/configured_feature` -> `worldgen/feature` (config inlined), `configured_carver` -> `carver`, noise-settings `surface_rule` -> `material_rule`, many density-function field renames, block-state-provider type renames (`simple_state_provider` -> `simple`, ...). If Slab emits worldgen, re-check everything against 26.3.
24. **Functions/macros: no syntax change found** 1.21.4 -> 26.3. `$(...)` macros, `#minecraft:load`/`#minecraft:tick`, `/return`, `/function ... with`, `execute if function` unchanged on the wiki. `function`/`tags/function` singular folders (since 1.21, 24w21a) remain. Only Mojira bug noted: `schedule clear` requires a namespace (MC-254661, 26.3 fixes list).
25. **Registries and folders are singular, resource-location typed, and increasingly "file or inline"**: in 26.3 loot tables, predicates, item modifiers, number providers, slot sources, block transformers, block state providers, material rules/conditions can all be referenced by ID or written inline; lists can mix IDs and inline values. Emit IDs where possible and keep a cache of which fields accept which.

---

## 1. Version -> pack format table

| Version | Date | Data pack format | Resource pack format | `pack.mcmeta` convention |
|---|---|---|---|---|
| 1.21.4 | 2024-12-03 (from memory, **UNVERIFIED**) | 61 | 46 | `pack_format` |
| 1.21.5 | 2025-03-25 (**UNVERIFIED**) | 71 | 55 | `pack_format` |
| 1.21.6 | 2025-06-17 (**UNVERIFIED**) | 80 | 63 | `pack_format` |
| 1.21.7, 1.21.8 | 2025-06-30, 2025-07-17 (**UNVERIFIED**) | 81 | 64 | `pack_format` |
| 1.21.9, 1.21.10 | 2025-09-30, 2025-10-07 (**UNVERIFIED**) | 88.0 | 69.0 | `min_format`+`max_format` (e.g. `[88,0]`), `pack_format` optional; `supported_formats` removed |
| 1.21.11 | 2025-12-09 (**UNVERIFIED**) | 94.1 | 75.0 | same as above |
| 26.1, 26.1.1, 26.1.2 | 2026-03-24, 04-01, 04-09 | 101.1 | 84.0 | same |
| 26.2 | 2026-06-16 | 107.1 | 88.0 | same |
| 26.3 | 2026-09-15 | **121.0** | **97.1** | same |
| 26.4 snapshots (in progress) | from 2026-09-22 | 122.0 (s1), 122.1 (s2), 123.0 (s3), 124.0 (s4), 125.0 (s5) | 98.0 (s1) | same |

Notes:
- 26.3 dev snapshots used data pack format 108.0-118.0 (114 skipped), pre-1 = 119.0, pre-2/3 and RCs = 120.0 (pre-3 = 121.0 per misode; wiki merges rows), release 121.0. Packs targeting snapshots must match exactly; only releases matter for Slab.
- Format numbers jumped (88 -> 121 across 26.x) because every snapshot bumped the major number during 26.x development; do not infer "number of breaking changes" from the gap.
- Dates of 26.x from the wiki Java_Edition_version_history page; update names: 26.1.2 "Tiny Takeover", 26.2 "Chaos Cubed", 26.3 "Wilderness Bound".

### pack.mcmeta convention details (25w31a, shipped in 1.21.9)
Misode notes for 25w31a:
- Pack versions now have minor versions. A minor increment is backwards-compatible (all packs made for earlier iterations of the same major keep working). Packs can set requirements on minor versions if they rely on resources introduced in a minor version.
- `min_format` and `max_format` are required in `pack` (versions `[major, minor]`; a single int N == `[N,0]`; a one-element list == that int).
- `pack_format` is optional now; it is still *required* if the pack declares support for a data pack format < 82 / resource pack format < 65.
- `supported_formats` removed (still required if range includes < 82 / < 65; otherwise not allowed).
- Overlay entries: `formats` removed; use `min_format` and `max_format` in each entry (same encoding). (Again, `formats` still required in all overlay entries if any overlay range includes < 82 / < 65.)
- Wiki recommended example for 1.21.9+: `{"pack":{"description":"...","min_format":88,"max_format":88}}`.
- Wiki vanilla example uses `"min_format":[88,0],"max_format":[88,0]`.
- 26.3 wiki bug note: MC-310337 "The game log outputs the incorrect minimum format version requiring the pack_format field". Do not trust the log text.
- `filter` section (since 1.19) and `features` (feature flags) unchanged. Locator-bar feature flag `locator_bar` existed only in 25w15a-25w16a snapshots.

---

## 2. Per-version changes

Legend: **[B]** = breaking for existing packs/commands, **[N]** = notable/new. Everything is "final release" view unless noted.

### 2.1 Java 1.21.5 (data pack format 71; resource pack 55)
Sources: https://github.com/misode/technical-changes/tree/main/1.21.5 (1.21.5-pre1, 25w02a-25w10a), https://minecraft.wiki/w/Java_Edition_1.21.5, https://minecraft.wiki/w/Data_pack, https://minecraft.wiki/w/Text_component_format

**Commands**
- [B] `/tellraw`, `/title`, `/bossbar`, `/team`, `/scoreboard ... styled` take SNBT text components, not JSON strings:
  - old: `tellraw @s '{"text":"Hello world"}'`  -> new: `tellraw @s {text:'Hello world'}`
  - old: `scoreboard objectives modify o numberformat styled '{"color":"red"}'` -> new: `... styled {color:"red"}` (style as SNBT).
- [B] `/setblock` and `/fill` block-entity data rules:
  - existing BE data is preserved if not specified;
  - specified data overwrites;
  - `{}` clears: `setblock ~ ~ ~ chest{}`;
  - success if either state or BE data changed.
- [N] `strict` option (place without block/shape updates):
  - `fill <from> <to> <block> [replace <filter>] [strict|destroy|hollow|outline]`
  - `clone [from <dim>] <begin> <end> [to <dim>] <dest> [strict] [[replace|masked|filtered <filter>] [force|move|normal]]`
  - `setblock <pos> <block> [destroy|keep|replace|strict]`
  - `place template <template> [<pos> [<rotation> [<mirror> [<integrity> [<seed> [strict]]]]]]`
- [N] `/fill ... replace <filter>` is no longer terminal (can be followed by another option).
- [N] `/data`: heterogeneous lists are transparently created/modified; paths with an empty key (`foo.''.bar`) can no longer be traversed.
- [N] Text-component arguments of `/bossbar`, `/scoreboard`, `/team` are resolved in the context of `@s` before use.
- [B] Item slot `horse.saddle` -> `saddle` (works for any entity able to wear a saddle, driven by `equippable`).
- [N] New `/test` command (GameTest): `test create <id> [<w> [<h> <d>]]`, `test run <selector> [<n>] [<untilFailed>] [<rotationSteps>] [<testsPerRow>]`, `runthese`, `runclosest`, `runthat`, `runfailed`, `runmultiple`, `verify`, `locate`, `pos`, `stop`, `clearall`, `clearthat`, `clearthese`, `resetclosest`, `resetthat`, `resetthese`. Server entry point `net.minecraft.gametest.Main` (options `--packs`, `--report`, `--tests`, `--universe`, `--verify`).
- Fixes: `/teleport` x-rotation no longer clamped relative to ±90; `/data remove` works on block loot tables, CustomName etc.
- 1.21.5-pre1 briefly reverted "boolean accepts numbers" behavior for inline args (loot tables, predicates, modifiers, components, `execute if predicate/items`) - net effect: inline arguments still accept numbers where booleans expected. (Marked obsolete in misode.)

**Text components / SNBT / NBT**
- [B] Text components stored as NBT; in item components and commands they are written as SNBT: `custom_name={text:'Renamed item'}` (was `custom_name='{"text":"Renamed item"}'`). Booleans vs ints are indistinguishable in NBT, so use string form for translation args that must be booleans.
- [B] `hoverEvent` -> `hover_event`, `clickEvent` -> `click_event`.
  - `show_text`: `contents` -> `value` (went through `text` in 25w02a then `value` in 25w03a).
  - `show_item`: inlined: `{action:'show_item',id:'minecraft:stick',count:2}` (+ optional `components`).
  - `show_entity`: inlined; old `id` -> `uuid`, old `type` -> `id`; optional `name`.
  - `open_url`: `value` -> `url` (only `http`/`https` parse); `run_command`/`suggest_command`: `value` -> `command` (`/` prefix optional); `change_page`: `value` -> `page` (positive integer); `copy_to_clipboard` unchanged.
  - Invalid events now fail to parse instead of silently doing nothing.
- [N] SNBT expanded (25w04a, 25w09a, 25w10a):
  - heterogeneous lists: `['a',{'b':3}]` stored as `[{'':'a'},{'b':3}]`; arrays (`[I;...]`) remain type-restricted;
  - numbers: `.1`, `1.`, `1.2e3`, `0xCAFE`, `0b101`, underscores `1_000`, suffixes `sb ub ss us si ui sl ul` (`240ub` == `-16sb`; `240sb` fails), no leading zeros, no `NaN`/`Inf`/hex floats; implicit infinite floats (`1e1000`) rejected;
  - strings: escapes `\x`, `\u`, `\U`, `\N{name}`, `\b \s \t \n \f \r`; unquoted strings may not start with `0-9 . + -`;
  - arrays accept narrower types (`[I;1b,2s,3]`) and untyped elements assume the array type;
  - one trailing comma allowed in lists/compounds;
  - new operations `bool(arg)` and `uuid(str)`.

**Data pack JSON**
- [B] Many entity sub-predicates removed (axolotl, fox, mooshroom, rabbit, horse, llama, villager, parrot, salmon, tropical_fish, painting, cat, frog, wolf, pig, sheep `color`); use the new `components` field in entity predicates with variant components (`wolf/variant`, `cat/variant`, ...). New `predicates` field for partial component match on entities and block predicates (`components`, `predicates` on block predicates too).
- [B] `stepping_on` entity predicate now only true if the entity is on the ground.
- [B] Advancement `display.background` no longer has the `textures/` prefix or `.png` suffix.
- [B] Recipes: `smithing_trim` now requires `base`, `template`, `addition` and has `pattern`; `crafting_transmute.base` required; `smithing_transform` base required; `crafting_transmute.result` can be `{id,count,components}`. Trim pattern/material definitions no longer carry an `item` field (determined by recipe / `provides_trim_material`).
- [N] Mob variant registries became data-driven: `pig_variant`, `cat_variant`, `frog_variant`, `cow_variant`, `chicken_variant`, `wolf_sound_variant`. Variants replaced `biome` fields with `spawn_conditions: [{priority, condition?}]` where condition types are `biome`, `moon_brightness`, `structures`. Wolf variant textures grouped into `assets:{angry,tame,wild}`; pig variant `texture` -> `asset_id`.
- [N] GameTest: `test_instance` and `test_environment` registries (environment types `all_of`, `function`, `game_rules`, `weather`, `time_of_day`), `test_block`, POI type `test_instance` (1.21.6).
- [N] Loot: `toggle_tooltips` accepts any component ID.
- [N] Tags (selection): block `#replaceable_by_mushrooms`, `#sword_instantly_mines`, `#edible_for_sheep`, `#camels_spawnable_on`, `#plays_ambient_desert_block_sounds` (renamed 1.21.6); item `#book_cloning_target`, `#eggs`; entity `#can_equip_saddle`, `#can_wear_horse_armor`; renamed block tag `#dead_bush_may_place_on` -> `#dry_vegetation_may_place_on` (renamed again 26.1); removed cat tags `#default_spawns`, `#full_moon_spawns`; structure tags `#on_<biome>_village_maps`.

**Components (items/entities)**
- [B] Removed `hide_additional_tooltip`, `hide_tooltip`, and all `show_in_tooltip` fields (`attribute_modifiers`, `dyed_color`, `can_place_on`, `can_break`, `enchantments`, `stored_enchantments`, `jukebox_playable`, `trim`, `unbreakable`). Replacement: `tooltip_display={hide_tooltip:true,hidden_components:['minecraft:enchantments']}`.
- [N] Flattened/simplified encodings (from wiki 1.21.5):
  - `attribute_modifiers={modifiers:[...]}` -> `attribute_modifiers=[...]`
  - `can_place_on={predicates:[{blocks:'stone'}]}` -> `can_place_on={blocks:'stone'}`
  - `dyed_color={rgb:12345}` -> `dyed_color=12345`
  - `enchantments={levels:{sharpness:2}}` -> `enchantments={sharpness:2}`
- [N] New: `blocks_attacks`, `break_sound`, `weapon{item_damage_per_attack, disable_blocking_for_seconds}`, `potion_duration_scale`, `provides_banner_patterns`, `provides_trim_material`, `tooltip_display`, `tool.can_destroy_blocks_in_creative`, `equippable.equip_on_interact` (misode text says `equip_on_interaction`; wiki says `equip_on_interact` - use wiki; **UNVERIFIED**).
- [N] Entity-and-item components for variants: `villager/variant`, `wolf/variant`, `wolf/collar`, `wolf/sound_variant`, `fox/variant`, `salmon/size`, `parrot/variant`, `tropical_fish/{pattern,base_color,pattern_color}`, `mooshroom/variant`, `rabbit/variant`, `pig/variant`, `frog/variant`, `horse/variant`, `painting/variant`, `llama/variant`, `axolotl/variant`, `cat/{variant,collar}`, `sheep/color`, `shulker/color`, `cow/variant`, `chicken/variant`. `custom_name`, `potion_contents`, `potion_duration_scale` transfer from spawning item to entity. All entities now have a `custom_data` component (key `data`, stored only if non-empty).
- [N] Item model `select` gets a `component` property; `condition` gets `component`.

**Entity/block NBT** (all breaking unless noted)
- `ArmorItems`/`HandItems`/`body_armor_item` -> `equipment:{head,chest,legs,feet,mainhand,offhand,body,saddle}` (players also store armor/offhand in `equipment`); `ArmorDropChances`/`HandDropChances`/`body_armor_drop_chance` -> `drop_chances` (default 0.085f not stored).
- `Saddle` bool (pig/strider) -> `equipment.saddle`.
- `FallDistance` -> `fall_distance` (double).
- `potion` entity split into `splash_potion` and `lingering_potion`.
- Position merges: `TileX/Y/Z` -> `block_pos` (item frames, leash knot), `SleepingX/Y/Z` -> `sleeping_pos`, turtle `HomePosX/Y/Z` -> `home_pos` (TravelPos* removed), vex `BoundX/Y/Z` -> `bound_pos`, phantom `AX/AY/AZ` -> `anchor_pos`, `Size` -> `size`; dolphin `TreasurePos*` removed.
- Player: `SpawnX/Y/Z/Angle/Dimension/Forced` -> `respawn:{pos,angle,dimension,forced}` (1.21.9 later changes `angle` -> `yaw`, adds required `pitch`, `dimension` required); `enteredNetherPosition` -> `entered_nether_pos` (list of doubles).
- Minecart: `CustomDisplayTile` removed. `Pos`/`Motion`/`Rotation` with wrong arity are discarded.
- Many fields no longer persist when removed via `/data remove` (fox `Trusted`, item `Owner`/`Thrower`, display `glow_color_override`, block `CustomName`/`LootTable`, etc.) and many have new defaults (25w09a list: area_effect_cloud `Duration` -1 = infinite, creeper `Fuse` 30, tnt `fuse` 80, etc.).
- Area effect cloud: `potion_duration_scale`.
- Misc: `Owner` of tamed animals not restricted to players; `Health`/`Air` default to max.

**Gamerules**
- [N] `allowFireTicksAwayFromPlayer` (default false), `tntExplodes` (default true). (Both later renamed/removed - see 1.21.11.)

**Misc**
- Shaders: core/post shader program JSON removed (resource packs only).
- Game test server main class added.

### 2.2 Java 1.21.6 (data pack 80; resource pack 63)
Sources: https://github.com/misode/technical-changes/tree/main/1.21.6, https://minecraft.wiki/w/Java_Edition_1.21.6

**Commands**
- [N] `/dialog show <targets> <dialog>` (ID or inline) and `/dialog clear <targets>`.
- [N] `/waypoint list`; `/waypoint modify <selector> color <named>|color hex <RRGGBB>|color reset|style set <style>|style reset` (the wiki lists `style`; misode's early snapshot listed `fade`, replaced by `style`).
- [N] `/datapack create <id> <name>` (creates an empty directory pack in the world; name is a text component placed in `pack.mcmeta`). Server owner only.
- [N] `/version` (no args; op or singleplayer).
- [N] `/playsound` new sound category `ui` (keeps playing while paused).
- [N] Data generator `commands.json` report nodes now have `required_level`. Command syntax report lists permission level.
- [N] Commands from `run_command` click events (chat, books) that cannot be parsed or require level > 0 now need user confirmation.

**Data pack**
- [N] Dialog system: `data/<ns>/dialog/*.json`. Types `confirmation`, `notice`, `dialog_list`, `multi_action`, `server_links`; body `item`, `plain_message`; inputs `boolean`, `number_range`, `single_option`, `text`; submit methods `command_template` (macros, confirmation if permission > 0), `custom_form`, `custom_template`. Dialog tags `#minecraft:pause_screen_additions`, `#minecraft:quick_actions` (Quick Actions key defaults to `G`). Click actions `dynamic/run_command` (template with `$(key)`), `dynamic/custom`, `show_dialog`, `custom`.
- [N] Text click events `custom` (`id`, `payload` as full NBT tag, no vanilla server effect) and `show_dialog`.
- [N] Dimension type optional `cloud_height`.
- [N] `attribute_modifiers` entries can have `display:{type:default|hidden|override,value:<text>}`.
- [B] `area_effect_cloud` NBT `Particle` -> `custom_particle` (colour no longer inherited from potion contents for `entity_effect`/`tinted_leaves`).
- [B] `painting/variant` component no longer allows inline variants.
- [N] Mob NBT `home_pos`, `home_radius` on all mobs; `owner` (UUID) on `tnt` and `vex`.
- [N] Attributes `waypoint_transmit_range`, `waypoint_receive_range` (0..60000000, players default 60000000), `camera_distance` (4), ghast `flying_speed`.
- [N] Advancement trigger `player_sheared_equipment` (`item`, `entity`, `player`); `equippable` gets `can_be_sheared`, `shearing_sound`.
- [B] Tag rename `#plays_ambient_desert_block_sounds` -> `#triggers_ambient_desert_sand_block_sounds`.
- [B] Sound event `block.sand.wind` -> `block.dry_grass.ambient`.
- [B] Resource-pack-side: shader uniforms are uniform blocks; item models need `oversized_in_gui:true` to exceed slot; `head` model type no longer reads `profile` (use `player_head`).

**Gamerules**: [N] `locatorBar` (briefly `useLocatorBar`; default true on servers; disabling removes all waypoints).

### 2.3 Java 1.21.7 / 1.21.8 (data pack 81; resource pack 64)
Sources: misode `1.21.7/1.21.7-rc2.md`; https://minecraft.wiki/w/Pack_format
- Only a pack-format bump (music disc `lava_chicken` item + sound event). No command or data-driven schema changes found.

### 2.4 Java 1.21.9 / 1.21.10 (data pack 88.0; resource pack 69.0)
Sources: https://github.com/misode/technical-changes/tree/main/1.21.9, https://minecraft.wiki/w/Java_Edition_1.21.9, https://minecraft.wiki/w/Server.properties, https://minecraft.wiki/w/Game_rule
(1.21.10 has no additional technical changes in misode; pack formats identical.)

**Pack metadata** [B]: minor versions; `min_format`/`max_format`; `pack_format` optional; `supported_formats` and overlay `formats` removed (see section 1).

**Commands**
- [B] `/summon` now fails when summoning monsters on Peaceful.
- [N] `/fetchprofile name <name>` / `id <uuid>`: async profile resolution; output includes clickable "summon mannequin" text. (`entity` mode added 26.1.)
- [N] `/spawnpoint` and `/setworldspawn` accept optional pitch; `/setworldspawn` works in any dimension; server commands run from the respawn dimension by default, not always the overworld.

**Data pack**
- [B] Player NBT `respawn`: `angle` -> `yaw` (required), new required `pitch`, `dimension` required.
- [B] Noise settings: `initial_density_without_jaggedness` replaced by `preliminary_surface_level` (2D density function giving the surface y-level). New density functions `find_top_surface`, `invert`.
- [N] Structure `max_distance_from_center` can be `{horizontal,vertical}`.
- [N] Loot table types `entity_interact`, `block_interact`; loot tables `charged_creeper/root`, `brush/armadillo`, `gameplay/turtle_grow`, `harvest/beehive`, `harvest/cave_vine`, `harvest/sweet_berry_bush`, `carve/pumpkin`. `copy_name` and `copy_components` loot functions' `source` accepts new context values (`direct_attacker`, `target_entity`, `interacting_entity`, `this`, `attacker`, `attacking_player`, `tool`). Entity targets in loot (`entity_properties.entity`) can address `target_entity`/`interacting_entity`.
- [B] `profile` component no longer resolves profiles on its own: static when it has `properties` or both/neither of `name`/`id`; dynamic when only one of `name`/`id`. Extra override fields `texture`, `cape`, `elytra`, `model`. `skull` block entity `profile` changed to match.
- [N] `mannequin` entity (summon only): `profile`, `hidden_layers`, `main_hand`, `pose`, `immovable`, `description`, `hide_description`.
- [N] Text component `object` (`atlas`/`player` object types, `sprite`, `hat`) - see Text section.
- [B] World border is dimension-specific (each dimension has its own border).
- [B] `chain` renamed `iron_chain` (block/item ID and sprites). Copper age content (copper golem, copper tools/armor, shelves, chests) with tags `#copper_chests`, `#copper`, `#chains`, `#lanterns`, `#bars`, ...
- [B] `flash` particle requires `color`. `copper_golem` NBT `weather_state` is a string id (`unaffected`...`oxidized`).
- [N] `blocks_attacks`: 0 reduced damage can no longer disable the item.
- [B] Shader version 330 required (OpenGL 3.3).
- [N] Server management protocol (JSON-RPC over WebSocket), `management-server-*` properties.

**Gamerules** (all three are boolean; names later renamed in 1.21.11)
- Removed `spawnChunkRadius` (25w31a).
- Added `pvp`, `allowEnteringNetherUsingPortals`, `spawnMonsters`, `enableCommandBlocks` (renamed `commandBlocksEnabled` in 25w37a), `spawnerBlocksEnabled` (25w37a).
- [B] `server.properties` `allow-nether`, `spawn-monsters`, `enable-command-block`, `pvp` removed.

### 2.5 Java 1.21.11 (data pack 94.1; resource pack 75.0)
Sources: https://github.com/misode/technical-changes/tree/main/1.21.11, https://minecraft.wiki/w/Java_Edition_1.21.11, https://minecraft.wiki/w/Game_rule, https://minecraft.wiki/w/Commands/execute

**Gamerules** [B]: registry + snake_case IDs (25w44a). Full rename table (old -> new, value inversion marked):
`announceAdvancements`->`show_advancement_messages`, `commandBlocksEnabled`->`command_blocks_work`, `commandModificationBlockLimit`->`max_block_modifications` (min 1), `disableElytraMovementCheck`->`elytra_movement_check` (**inverted**), `disablePlayerMovementCheck`->`player_movement_check` (**inverted**), `disableRaids`->`raids` (**inverted**), `doDaylightCycle`->`advance_time`, `doEntityDrops`->`entity_drops`, `doImmediateRespawn`->`immediate_respawn`, `doInsomnia`->`spawn_phantoms`, `doLimitedCrafting`->`limited_crafting`, `doMobLoot`->`mob_drops`, `doMobSpawning`->`spawn_mobs`, `doPatrolSpawning`->`spawn_patrols`, `doTileDrops`->`block_drops`, `doTraderSpawning`->`spawn_wandering_traders`, `doVinesSpread`->`spread_vines`, `doWardenSpawning`->`spawn_wardens`, `doWeatherCycle`->`advance_weather`, `maxCommandChainLength`->`max_command_sequence_length` (min 0), `maxCommandForkCount`->`max_command_forks` (min 1), `naturalRegeneration`->`natural_health_regeneration`, `snowAccumulationHeight`->`max_snow_accumulation_height` (0-8), `spawnRadius`->`respawn_radius` (min 0), `spawnerBlocksEnabled`->`spawner_blocks_work`.
Un-renamed-in-list rules that nonetheless became snake_case (camelCase -> snake_case mechanical, per wiki): `keepInventory`->`keep_inventory`, `mobGriefing`->`mob_griefing`, `fallDamage`->`fall_damage`, `fireDamage`->`fire_damage`, `drowningDamage`->`drowning_damage`, `freezeDamage`->`freeze_damage`, `reducedDebugInfo`->`reduced_debug_info`, `randomTickSpeed`->`random_tick_speed`, `pvp`, `spawnMonsters`->`spawn_monsters`, `allowEnteringNetherUsingPortals`->`allow_entering_nether_using_portals`, `commandBlockOutput`->`command_block_output`, `logAdminCommands`->`log_admin_commands`, `sendCommandFeedback`->`send_command_feedback`, `showDeathMessages`->`show_death_messages`, `universalAnger`->`universal_anger`, `forgiveDeadPlayers`->`forgive_dead_players`, `maxEntityCramming`->`max_entity_cramming`, `playersSleepingPercentage`->`players_sleeping_percentage`, `playersNetherPortalDefaultDelay`->`players_nether_portal_default_delay`, `playersNetherPortalCreativeDelay`->`players_nether_portal_creative_delay`, `spectatorsGenerateChunks`->`spectators_generate_chunks`, `lavaSourceConversion`->`lava_source_conversion`, `waterSourceConversion`->`water_source_conversion`, `locatorBar`->`locator_bar`, `tntExplodes`->`tnt_explodes`, `globalSoundEvents`->`global_sound_events`, `projectilesCanBreakBlocks`->`projectiles_can_break_blocks`, `enderPearlsVanishOnDeath`->`ender_pearls_vanish_on_death`, blockExplosionDropDecay/mobExplosionDropDecay/tntExplosionDropDecay -> `block_explosion_drop_decay`/`mob_explosion_drop_decay`/`tnt_explosion_drop_decay`, `minecartMaxSpeed`->`max_minecart_speed` (Minecart Improvements experiment only).
- [B] `doFireTick` and `allowFireTicksAwayFromPlayer` removed; replaced by integer `fire_spread_radius_around_player` (0 disables fire spread, -1 allows spread with no player nearby; wiki default 128 - **UNVERIFIED**).
- Value ranges now enforced: `max_block_modifications>=1`, `max_command_forks>=1`, `max_command_sequence_length>=0`, `max_entity_cramming>=0` (loosened from 1 in 25w45a), `max_snow_accumulation_height` 0-8, `players_nether_portal_*_delay>=0`, `players_sleeping_percentage>=0`, `random_tick_speed>=0`, `respawn_radius>=0`.
- [B] GameTest `game_rules` environment: `bool_rules`/`int_rules` lists replaced by a single `rules` map `{ "<gamerule id>": value }`.
- Server management protocol bumped to 2.0.0: game rule `value` is boolean/integer keyed by game rule ID.

**Commands**
- [N] `/stopwatch` (real-time, not tick-based; only runs while the world runs). Final syntax (25w42a changed order): `/stopwatch create <id>`, `/stopwatch query <id> [<scale>]` (returns value scaled and truncated; default scale 1 gives seconds, `20` gives ticks-equivalent), `/stopwatch restart <id>`, `/stopwatch remove <id>`. Old early-snapshot form `/stopwatch foo:bar create` is invalid.
- [N] `/execute if|unless stopwatch <id> <range>` (seconds, ms accuracy).
- [B] `/worldborder`: all time arguments are now ticks, with optional `s` (seconds) or `d` (in-game days) suffix. Interpolation follows game ticks (pauses with game/`/tick`). Old: `/worldborder set 100 10` = 10 s; new: `/worldborder set 100 10` = 10 ticks, write `10s` for old behavior.
- [N] Resource-location suggestions show all namespaces, not just `minecraft`.
- [N] Item predicates in command form accept empty predicates to test for component presence: `*[instrument]` == `*[instrument~{}]`; component predicates exist for every component (`{<type>:{}}`).

**Data pack**
- [N] Environment attributes system (dimension and biome `attributes` fields; biome overrides dimension). Key attributes (all `namespace:category/name`): `visual/fog_color`, `visual/sky_color`, `visual/water_fog_color`, `visual/fog_start_distance`, `visual/fog_end_distance`, `visual/water_fog_start_distance`, `visual/water_fog_end_distance`, `visual/sky_fog_end_distance`, `visual/cloud_fog_end_distance`, `visual/cloud_color`, `visual/cloud_height`, `visual/default_dripstone_particle`, `visual/ambient_particles`, `visual/sunrise_sunset_color`, `visual/sun_angle`, `visual/moon_angle`, `visual/star_angle`, `visual/moon_phase`, `visual/star_brightness`, `visual/sky_light_color`, `visual/sky_light_factor`, `audio/background_music`, `audio/music_volume`, `audio/ambient_sounds`, `audio/firefly_bush_sounds`, `gameplay/can_start_raid`, `gameplay/water_evaporates`, `gameplay/bed_rule`, `gameplay/respawn_anchor_works`, `gameplay/nether_portal_spawns_piglin`, `gameplay/fast_lava`, `gameplay/increased_fire_burnout`, `gameplay/piglins_zombify`, `gameplay/snow_golem_melts`, `gameplay/sky_light_level`, `gameplay/eyeblossom_open`, `gameplay/turtle_egg_hatch_chance`, `gameplay/creaking_active`, `gameplay/surface_slime_spawn_chance`, `gameplay/cat_waking_up_gift_chance`, `gameplay/bees_stay_in_hive`, `gameplay/monsters_burn`, `gameplay/can_pillager_patrol_spawn`, `gameplay/villager_activity`, `gameplay/baby_villager_activity`. (`visual/extra_fog`, `visual/cloud_opacity`, `visual/water_fog_radius` existed only in snapshots.)
- [B] Dimension type: removed/migrated `ultrawarm`, `bed_works`, `respawn_anchor_works`, `cloud_height`, `piglin_safe`, `has_raids`, `natural`, `effects` (replaced by `skybox: none|overworld|end` and `cardinal_light: default|nether`), `fixed_time` (replaced by boolean `has_fixed_time`). New optional `timelines` field.
- [B] Biome `effects` fields `fog_color`, `water_fog_color`, `sky_color`, `particle`, `ambient_sound`, `mood_sound`, `additions_sound`, `music`, `music_volume` migrated to attributes. Biome effect colors accept `"#rrggbb"` or `[r,g,b]` floats.
- [B] Removed biome tags `#snow_golem_melts`, `#increased_fire_burnout`, `#plays_underwater_music`, `#has_closer_water_fog`, `#without_patrol_spawns`.
- [N] Timelines (`data/<ns>/timeline/*.json`; fields `period_ticks`, `tracks`); timeline tags `#universal`, `#in_overworld`, `#in_nether`, `#in_end`. (Gained a required `clock` and optional `time_markers` in 26.1.) Mob `activity` registry. `blend_to_gray` colour modifier.
- [N] Slot sources: `empty`, `group`, `slot_range`, `contents`, `filtered`, `limit_slots`; loot pool entry type `slots`.
- [B] Loot `filtered` function: `modifier` -> `on_pass` and `on_fail`. New `discard` loot function.
- [N] Enchantment effects: `post_piercing_attack`, `apply_impulse`, `apply_exhaustion`, `exponent` value type; `play_sound` accepts list per level.
- [N] Components: `use_effects`, `minimum_attack_charge`, `damage_type`, `kinetic_weapon`, `piercing_weapon`, `swing_animation`, `attack_range` (fields moved out of the two weapon components: `min_reach`/`max_reach`/`hitbox_margin` -> `attack_range{min_range,max_range,hitbox_margin,mob_factor,min_creative_reach,max_creative_reach}`). [B] `consumable.animation` `spear` -> `trident` (new `spear`).
- [N] Predicates: entity `flags` gains `is_in_water`, `is_fall_flying`. Advancement trigger `spear_mobs`.
- [B] NBT: `AngryAt` -> `angry_at`; `AngerTime` -> `anger_end_time` (absolute game tick).
- [B] Tag `#enchantable/sword` -> `#enchantable/sweeping`. New tags `#spears`, `#enchantable/melee_weapon`, nautilus tags, `#burn_in_daylight`, ...
- [B] Resource pack atlas: item textures split out of `blocks` into new `items` atlas (no mipmaps); `celestials` atlas. Commands/`atlas` references to item textures must use `items`.
- [N] Zombie nautilus variants (`zombie_nautilus_variant`), spawn-condition predicates.

### 2.6 Java 26.1 (26.1.1, 26.1.2 same pack formats) (data pack 101.1; resource pack 84.0)
Sources: https://github.com/misode/technical-changes/tree/main/26.1, https://minecraft.wiki/w/Java_Edition_26.1, https://minecraft.wiki/w/Commands/time, https://minecraft.wiki/w/Text_component_format
(26.1.1 and 26.1.2 are bug-fix releases with the same formats; no technical changes recorded.)

**Time** [B] (the big one for Slab)
- New registry `world_clock` (`data/<ns>/world_clock/<id>.json`): an internal time that increases every tick and can be paused/resumed, with a rate multiplier. Vanilla clocks include `minecraft:overworld`, `minecraft:the_end` (End sky flashes).
- Timelines now **require** `clock` (use `minecraft:overworld` to preserve previous behaviour) and may carry `time_markers: { "<ns:id>": <int ticks> | {ticks, show_in_commands?} }`. Built-in markers: `wake_up_from_sleep` (time to jump to when everyone wakes), `roll_village_siege`; overworld naming markers `day`, `noon`, `night`, `midnight` (wiki values from the Bedrock section: day 1000, noon 6000, night 13000, midnight 18000 - Java values **UNVERIFIED**).
- Dimension type `default_clock` (optional) selects the clock for `/time`. Per wiki, only Overworld and The End have defaults; elsewhere omitting `of <clock>` errors.
- `/time` (permission level 2):
  ```
  /time [of <clock>] set <time>|<timemarker>
  /time [of <clock>] add <time>
  /time [of <clock>] pause | resume
  /time [of <clock>] rate <rate>          (pre-1; float >0 up to 1000; 1 = normal; clock/timeline speed only, unlike /tick rate)
  /time [of <clock>] query time
  /time [of <clock>] query <timeline> [repetition]
  /time query gametime
  ```
  `<time>` = non-negative float with unit `d` (24000), `s` (20), `t` (default). `set <timemarker>` per wiki 26.1 page advances to the **next occurrence** of that marker; the Commands/time page's examples still say `time set day` sets daytime to 1000 directly - the two disagree, **UNVERIFIED**.
  Old -> new: `/time set day` still works syntactically; `/time query daytime` and `/time query day` **no longer appear** in the 26.x syntax list (use `query time` or `query <timeline>`). Return values of `set`/`add` changed (clock total elapsed ticks).
- [B] `time_check` loot predicate has `clock` field; GameTest environment `time_of_day` replaced by `clock_time` (+ required `clock`); `timeline_attributes` test environment; game rule `advance_time` (formerly `doDaylightCycle`) - interaction with clocks is **UNVERIFIED**.
- Environment attribute-based number provider `environment_attribute` and predicate `environment_attribute_check`.

**Commands**
- [N] `/swing [<entities>] [<mainhand|offhand>]` (both optional; selector defaults `@s`, hand `mainhand`; returns number of entities).
- [N] `/fetchprofile entity <entity>`.

**Villagers / trades** [N]: `villager_trade` and `trade_set` registries (data-driven); loot table type `villager_trade`; transient `additional_trade_cost` component; loot functions `set_random_dyes`, `set_random_potion`; `enchant_with_levels`/`enchant_randomly` get `include_additional_cost_component`. [B] Removed enchantment tags `trades/*_special`.

**Recipes** [B]: `crafting_special_*` types converted/removed: `crafting_special_mapcloning` removed (use `crafting_transmute`), `crafting_special_armordye` -> `crafting_dye`, `crafting_special_tippedarrow` -> `crafting_imbue`; other special recipes changed semantics (`bookcloning`, `bannerduplicate`, `decorated_pot`, `firework_*`, `mapextending`, `shielddecoration`). Recipe `result` accepts short form `"minecraft:foo"` == `{"id":"minecraft:foo","count":1}` in all recipe types. `show_notification` field (default true). Unused `group` removed from stonecutting/smithing recipes. [B] Stonecutter recipes and their advancements renamed. New `minecraft:dye` component; item tag `#dyeable` removed; new `#dyes`, `#loom_dyes`, ...

**Tags** [B]: large vegetation-support rename (examples: `#dry_vegetation_may_place_on` -> `#supports_dry_vegetation`, `#bamboo_plantable_on` -> `#supports_bamboo`, `#small_dripleaf_placeable` -> `#supports_small_dripleaf`, `#mushroom_grow_block` -> `#overrides_mushroom_light_requirement`, `#snow_layer_can_survive_on` -> `#support_override_snow_layer`); `#dirt` split into `#dirt`, `#mud`, `#moss_blocks`, `#grass_blocks`. New fluid tags.

**NBT** [B]: villager and piglin inventory slots only via `mob.inventory.*` (`villager.*`/`piglin.*` removed). Baby mobs `AgeLocked`. Fields `current_explosion_impact_pos`, `current_impulse_context_reset_grace_time` moved to all mobs/armor stands; `ignore_fall_damage_from_current_explosion` removed.

**Text components** [B] (final 26.1, snapshot 5/8, pre-1/2):
- `selector` and `entity` (in `nbt`) fields no longer accept trailing data after the selector; invalid `nbt`/`block` fields are no longer silently ignored.
- `nbt` with `interpret:false` pretty-prints the tag (instead of flattening); new `plain:true` strips styling (cannot combine with `interpret`).
- `object`: optional `fallback` component; `player` objects forbidden in server-list MOTD; MOTD components nested > 16 deep are replaced by an ellipsis.

**Worldgen** [B]: `forest_rock` -> `block_blob`, `ice_spike` -> `spike`; tree `force_dirt`/`dirt_provider` -> `below_trunk_provider`; `flower`, `flower_no_bonemeal`, `random_patch` removed; configured feature tag `#can_spawn_from_bone_meal`; `count` placement modifier max 4096.

**Component ID/ref flexibility** [N]: `provides_banner_patterns`, `blocks_attacks.bypassed_by`, `damage_resistant.types`, `set_instrument.options` accept an ID or list of IDs in addition to a `#tag`.

**World storage / save layout** [B] (matters only for external tooling): `dimensions/<ns>/<path>/` per dimension (overworld `dimensions/minecraft/overworld`, nether `.../the_nether`, end `.../the_end`); `players/{advancements,data,stats}`; world resource pack `resourcepacks/resources.zip`; `data/` namespaced (`data/minecraft/scoreboard.dat`, `data/minecraft/game_rules.dat`, `weather.dat`, `world_clocks.dat`, `scheduled_events.dat`, `custom_boss_events.dat`, `wandering_trader.dat`, `world_gen_settings.dat`, `raids.dat`, `maps/<n>.dat`, `last_id.dat`); command storage is stored in per-namespace subfolders (was `command_storage_<ns>.dat`); `chunks.dat` -> `chunk_tickets.dat`; saved structure blocks `generated/<ns>/structure`; `level.dat`: `Player` -> `singleplayer_uuid`, `difficulty_settings{difficulty (string), locked}`; ender dragon fight data moved to the end dimension's data folder, dimension type `has_ender_dragon_fight`.

**Misc**: Java 25 required; default RAM 4 GB, ZGC; toast when server disk space is low; game rules screen reachable from World Options screen in-game.

### 2.7 Java 26.2 (data pack 107.1; resource pack 88.0)
Sources: https://github.com/misode/technical-changes/tree/main/26.2, https://minecraft.wiki/w/Java_Edition_26.2

**Entity predicates** [B] (the key change for Slab's predicate emitter):
- Format changed from "structure of optional fields" to a map of component-like keys, each key an identifier (no namespace == `minecraft:`).
  - old: `{"effects":{...}}` -> new: `{"minecraft:effects":{...}}` (bare `effects` also accepted).
  - old: `{"type":"minecraft:zombie"}` -> new: `{"minecraft:entity_type":"minecraft:zombie"}` (`type` renamed to `entity_type`).
  - old: `{"type_specific":{"type":"minecraft:player","looking_at":{"type":"minecraft:ender_dragon"}}}` -> new: `{"minecraft:type_specific/player":{"looking_at":{"minecraft:entity_type":"minecraft:ender_dragon"}}}`.
  - type-specific keys: `type_specific/lightning`, `type_specific/fishing_hook`, `type_specific/player`, `type_specific/raider`, `type_specific/sheep`, `type_specific/cube_mob` (was `slime`, now also matches sulfur cubes).
- Unknown keys are now rejected (previously ignored). New sub-predicate `minecraft:entity_tags` (matches `/tag` tags with `any_of`, `all_of`, `none_of`).
- Both direct and `entity_properties` loot-condition uses are affected.

**Commands**
- [B] Team and waypoint colour arguments only accept lowercase names with underscores (`dark_purple`, not `darkpurple`/`DarkPurple`), matching text-component colour names.
- [N] `/unpublish`; `/publish` gets a second boolean (open to online multiplayer vs local only) - then loses `gamemode` in 26.3. 
- [N] `/advancement grant|revoke` output counts changed advancements and affected players; `/execute on owner` treats vex as owned by its evoker.
- [N] Scoreboard: a player's score is not shown in the tab overlay if the player lacks the objective.

**Data pack**
- [N] New `sulfur_cube_archetype` registry (items, buoyant, explosion, contact_damage, knockback_modifiers, attribute_modifiers, sound_settings), `sulfur_cube_content` component, `bounce` game event, `sulfur_cube_hot` damage type.
- [N] Attributes `bounciness`, `friction_modifier`, `air_drag_modifier`, `below_name_distance`, `name_tag_distance` (renamed from `nameplate_distance`); [B] `knockback_resistance` min -2.0.
- [B] Removed `bed` block entity (beds are plain blocks); removed `weird_scaled_sampler` density fn (use `interval_select`); `noise_gradient` surface rule added then removed in-cycle; feature renames `pointed_dripstone` -> `speleothem`, `dripstone_cluster` -> `speleothem_cluster`.
- [B] NBT: `HurtByTimestamp` removed from living entities (use `ticks_since_last_hurt_by_mob`).
- [N] Many fields accept IDs/lists instead of only tags (`infiniburn`, geode blocks, `root_replaceable`, `protected_blocks`, ...).
- [B] Tag `#concrete_powder` -> `#concrete_powders`.
- Misc: `chat-spam-threshold-seconds`, `command-spam-threshold-seconds` server properties (default 10; 0 disables); management protocol 3.0.0; `signs`/`beds` atlases removed; `flat_all_dimensions` world preset (hidden).

### 2.8 Java 26.3 "Wilderness Bound" (data pack 121.0; resource pack 97.1)
Sources: https://github.com/misode/technical-changes/tree/main/26.3 (13 snapshot/pre-release files), https://minecraft.wiki/w/Java_Edition_26.3 (read to char 400000/472815), https://minecraft.wiki/w/Commands/execute

This release is by far the most breaking for data pack JSON. Group by area.

**Commands**
- [N] `/compute <target> integer <context_int_provider>` and `/compute <target> float <context_float_provider> [<scale>]`; `<target>` = `default` | `block <pos>` | `entity <selector>`; scale default 1.0, result floored. Fails (instead of returning 0) on Infinity/NaN/error. Loot context types `command_compute_default`, `command_compute_position`, `command_compute_entity`. `/data modify <target> <path> set compute <target> integer|float <provider>` (new source).
- [B] `/item replace` semantics: surplus destination slots / surplus source items are ignored (`replace [12345] with (ABC) -> [ABC45]`, `replace [123] with (ABCDE) -> [ABC]`). New: `/item fill` (`[12345] <- (ABC)` -> `[ABCAB]`) and `/item override` (`-> [ABC__]`, clears remaining). `/item ... from entity <targets>` accepts multiple entities (slots concatenated). Slot arguments in `/item` and `/execute if|unless items` accept a slot-source ID or inline slot source (loot context `command_slot_source`).
- [N] `/execute if|unless slots block <pos> <slot_source>` and `... slots entity <targets> <slot_source>` (counts slots).
- [B] `/publish` loses its `gamemode` argument (use `/defaultgamemode` and `/gamemode`).
- [B] `/recipe` no longer supports hardcoded recipes (type starts `minecraft:crafting_special_`); they are always unlocked when `limited_crafting` is true.
- [N] `/posteffect add|remove <player> <posteffect>`, `/posteffect clear <player>`, `/posteffect list <target>` (client shader post effects).
- [N] `/swing [<entities>] [<hand>] [<animation: none|whack|stab>] [<duration, default 6t>]`; no longer resets a player's attack strength.
- [N] `/place feature` accepts inline feature definitions. `/spreadplayers` safe blocks governed by `#entities_can_teleport_to`. `/test locate` searches all dimensions.
- Error behaviour: `/give` over item limit, `/tick step`, `/tick step stop`, `/tick sprint stop` now signal real command errors (instead of printing a message and returning 0). `/team join|leave` results count exact entities. `/tellraw` `nbt` components resolve at most 64k times. `/execute as` no longer re-evaluates entity names every use (perf). `/clear <t> <item> 0` no longer syncs inventory.

**Registry/folder moves and file-or-inline** [B]
- `worldgen/configured_feature` -> `worldgen/feature` (config fields inline at the root; no more `config` object); `worldgen/configured_carver` -> `worldgen/carver` (same).
- New data pack folders: `slot_source`, `number_provider` (split into `context_int_provider`/`context_float_provider` folders by pre-1; **folder name for the split is UNVERIFIED**, check at emit time), `worldgen/material_rule`, `worldgen/material_condition`, `worldgen/block_state_provider`, `block_transformer`.
- Loot tables, predicates, item modifiers, number providers, slot sources, recipes, advancement `rewards.loot` etc. accept a namespaced ID or an inline value; lists accept IDs, inline values, mixes, or `#tags` (e.g. `[{type:"set_count",...}, "example:some_modifier"]`). Only top-level files may define inline `group` slot sources and `sequence` item modifiers (snapshot 9).

**Loot / predicate / item modifier schema** [B]
- Predicates (loot conditions): `"condition"` -> `"type"`. Example: old `{"condition":"minecraft:inverted","term":{...}}` -> new `{"type":"minecraft:inverted","term":{...}}`. Bare-list shorthand removed; use explicit `{"type":"minecraft:all_of","terms":[...]}`. `reference` condition removed (use the predicate ID string wherever a predicate is accepted). `block_state_property` removed -> `match_block` (fields of a block predicate). `value_check` removed -> `int_value_check` / `float_value_check` (`value`, `test`).
- Item modifiers (loot functions): `"function"` -> `"type"`; `"conditions"` (list) -> `"condition"` (single inline or ID; `all_of` explicit); `reference` modifier removed; `set_loot_table`: `name`/`map` -> `loot_table_id`, field `type` removed; `exploration_map`: `destination` required (ID, list, or tag), `map_color` removed, no longer converts `map` -> `filled_map` (specify `filled_map` in the entry/trade; if no structure found the map has no `map_id`, filter with `filtered` + `discard`).
- Loot pool: `conditions` -> `condition`, `functions` -> `modifier` (also on `alternatives`, `sequence`, `group` entries); entry `tag`: `name` -> `items`; entry `loot_table`: `value` accepts list/tag, new `expand`; `rolls`/`bonus_rolls` accept number provider references. Loot table type `container_process` added; `advancement_location` gains block entities in context.
- Number providers: `type` always required (previously defaulted to `uniform`). Split into `context_int_provider` and `context_float_provider`:
  - both: `constant`, `uniform`, `storage`, `add` (was `sum`, field `summands` -> `operands`), `mul` (was `product`), `min`, `max`, `avg`, `weighted_list`, `conditional`, `number_dispatcher`, `environment_attribute`, plus arithmetic `sub`, `div`, `mod`, `pow`, `negate`, `abs`;
  - int only: `binomial`, `score`, `floor_mod`, `floor_div`, `from_float`;
  - float only: `enchantment_level`, `ceil`, `floor`, `round`, `truncate`, `sqrt`, `sin`, `cos`, `length`, `from_int`.
  Float `mod` uses standard modulus; `pow(0,0)` errors.
- Predicate `potion_contents` component predicate: `{potions?, effects?}`; `tool/can_silk_touch`, `tool/can_shear` vanilla predicates; entity predicate `food` and loot `sum` provider (26.1).
- Advancement triggers [B]: entity-predicate shorthand and bare condition lists in trigger fields removed (must wrap in `entity_properties` / `all_of`; derived example, exact serialized form **UNVERIFIED**): old `"player":{"equipment":{...}}`-style shorthand -> new `"player":{"type":"minecraft:entity_properties","entity":"this","predicate":{...}}`. Renames: `bee_nest_destroyed.block` -> `blocks` (+ `state`), `enter_block.block` / `slide_down_block.block` -> `blocks`, `player_generates_container_loot.loot_table` -> `loot_tables`, `recipe_crafted`/`crafter_recipe_crafted.recipe_id` -> `recipes`, `recipe_unlocked.recipe` -> `recipes`; `brewed_potion.potion` accepts `potion_contents`. Rewards `loot` accepts inline/list/tag. Visible root advancements must declare `display.background`, and only roots may.
- Recipes: new `brewing` recipe type (`input`, `reagent`, `output`; ingredients with optional `potion_contents`); `cookingtime` required in all cooking recipes (smoker/blast furnace values should be 2x the old value; speed-up is now via `cooking_fuel`); `crafting_special_*` hardcoded recipes unlockable only via limited-crafting logic.

**Components** [B unless noted]
- `pot_decorations`: list of 4 IDs -> `{back,left,right,front}` (all optional). `provides_pottery_pattern` [N]. `trim_material` component matches new definition format (palette ID, no `override_armor_assets`; overrides now in RP equipment asset via `trim_overrides`).
- `swing_animation` removed -> `attack_animation` and `interact_animation` `{type: none|whack|stab, duration: int>=0}`. `map_color` removed. `potion_contents` component predicate reshaped.
- New: `block_transformer` (now a registry/file: component refers by ID; folder `block_transformer`), `compostable{layers}`, `cooking_fuel{burn_time,speed_multiplier}`, `brewing_fuel{uses,speed_multiplier}` (replaces item tag `#brewing_fuel`), `sign_text_front`, `sign_text_back`, `waxed`, `cushion/color`, `villager_food{nutrition}`, `mob_visibility{targeting_entity,visibility}`. `teleport_randomly` consume effect `directional_particles`. `instrument`: optional `durability_damage`, `use_duration` may be 0. New `cushion` entity.
- Block-state data: `{"Name":"minecraft:stone","Properties":{...}}` -> `{"id":"minecraft:stone","properties":{...}}`; default state may be written as just `"minecraft:stone"` (wiki's inline-state note says the shorthand is not allowed in some inline provider contexts - **UNVERIFIED** where exactly).
- Block entities [B]: `sign`/`hanging_sign` no longer run click-event commands and no longer resolve text components unless `allow_op_features:true`; `brewing_stand` `BrewTime` short->int, `Fuel` byte->int, new `total_brew_time`, `total_fuel`, `speed_multiplier`; furnace-family `cooking_time_spent`, `cooking_total_time`, `lit_time_remaining`, `lit_total_time` short->int, new `speed_multiplier`. Entity NBT: villager `AssignProfessionWhenSpawned` removed; `invulnerable_time` added.

**Environment attributes**: `gameplay/bed_rule` field `explodes` -> `destroy_on_use`, new optional `destroy_on_leave`; new `gameplay/straw_bed_rule`, `gameplay/natural_mob_spawns` (biome `spawners`, `spawn_costs`, `creature_spawn_probability` removed - [B]), `gameplay/creature_world_gen_spawn_probability`; `visual/ambient_particles` interpolates and has `append` modifier.

**Data / tags** [B]: item tag `#brewing_fuel` removed; `#overworld_carver_replaceables`/`#nether_carver_replaceables` removed; structure tags renamed (`#on_woodland_explorer_maps` -> `#on_woodland_mansion_maps`, `#on_ocean_explorer_maps` -> `#on_ocean_monument_maps`, `#on_jungle_explorer_maps` -> `#on_jungle_pyramid_maps`, `#on_swamp_explorer_maps` -> `#on_swamp_hut_maps`, `#on_trial_explorer_map` -> `#on_buried_trial_chambers_map`); `#dowses_campfires` renamed `#douses_campfires` in-cycle. Villager trade field `given_item_modifiers` -> `given_item_modifier` (does not accept IDs). `trim_material` file: `asset_name` -> `palette`. GameTest `dimension` moved to each test instance; `flat_all_dimensions` preset used.

**Worldgen** [B]: all the noise/density/carver/feature renames listed in the misode 26.3 files, e.g. noise settings `surface_rule` -> `material_rule`, `default_block` removed (always air), `final_density` no longer implicitly adds `beardifier`, `aquifers_enabled`/`ore_veins_enabled` replaced by objects/rules, noise router fields removed; density functions `argument`->`input`, `argument1/2`->`left/right`, `invert`->`reciprocal`, `y_clamped_gradient`->`gradient`, `cache_once`->`cache` (several caches removed), `shifted_noise` folded into `noise`; `worldgen/noise` `firstOctave`->`base_octave`, `amplitudes`->`amplitude_modifiers`, new `octave_count`/`normalize`; block state provider type renames (`simple_state_provider`->`simple`, `weighted_state_provider`->`weighted`, `noise_provider`->`noise`, `rule_based_state_provider`->`rule_based`, `rotated_block_provider`->`rotated`, ...); density/noise now evaluated in single-precision float (world seeds differ); features renamed (`basalt_columns`->`stepped_column_cluster`, `basalt_pillar`->`single_block_pillar`, `glowstone_blob`->`random_neighbor_spread`, `random_offset`->`offset`; several removed); `noise`/`surface`/`carvers` chunk statuses merged to `terrain`; placed features must stay within a 3x3 chunk region. Details: see the misode 26.3 pages; Slab is unlikely to emit these.

**Options / networking / misc**
- `white-list` server property defaults to `true` [B for fresh server setups]; new properties `allowed-connection-ids`, `status-contact-details`, `enable-legacy-status` landed in 26.4 snapshot 1 (not 26.3). Game no longer hardcodes API endpoints (discovery service). Debug properties `MC_DEBUG_ORE_VEINS`, `MC_DEBUG_AQUIFERS`, `MC_DEBUG_DISABLE_ORE_VEINS` removed (26.4 s1).
- Gamerules: no new rule recorded in 26.2/26.3 changelogs. 26.3 wiki note: adventure-mode projectiles cannot break decorated pots/chorus flowers even with `projectiles_can_break_blocks=true` unless the projectile has `can_break`.

### 2.9 26.4 (not released at time of research; informational)
26.4 snapshots 1-5 exist (pack format 122.0 -> 125.0). Snapshot 1: network packet `mod_list`, `intention` host field changed, `transfer` gets `properties`; server.properties `allowed-connection-ids`, `status-contact-details`, `enable-legacy-status`; [B] `/fillbiome` now fills with block accuracy (misode lists it under 26.4 snapshot 1, not 26.3); worldgen noise and `straight_trunk_placer.trunk_width`. Not part of the 26.3 target.

---

## 3. Cross-cutting topics

### 3.1 Gamerules (summary for a compiler)
- Always emit snake_case IDs listed in 2.5. `/gamerule <name> [<value>]` takes boolean or integer per rule (no float). Values are type-checked; unknown rules error (custom dummy rules exist only as game rule registry entries now, i.e. **no custom gamerules** - unverified whether datapack-defined rules exist).
- Inversions: `elytra_movement_check`, `player_movement_check`, `raids` are now "true = checks/raids on".
- Command: `/execute store result ... run gamerule <name>` yields the current value (wiki).
- Mappings for removed server.properties: use `pvp`, `allow_entering_nether_using_portals`, `spawn_monsters`, `command_blocks_work`.

### 3.2 Permissions
- Command tree permissions are now named levels (`gamemasters`/`admins`/`owners` = 2/3/4; per-command table in 02-commands-core.md section 1.1, from the real 26.3 `commands.json`). Otherwise no change to level semantics found in changelogs from 1.21.5 to 26.3. `ops.json` fields: `uuid`, `name`, `level` (0-4), `bypassesPlayerLimit`. `op-permission-level` default 4 (0-4). `function-permission-level` default 2 (1-4), added 1.14.4-pre4, no later changes recorded. In singleplayer/LAN, functions run at level 2.
- Changes touching permissions: 1.21.6 command report has `required_level` per node; `run_command` click events that need > 0 or fail to parse need user confirmation; dialogs with `command_template` submit show confirmation if permission > 0; 26.3 sign `allow_op_features`; game rules screen available to operators only.
- 26.1: Operators can Ctrl+Pick on players/mannequins to see profile results. `/time` needs level 2.
- If you remember a "permission sets" rework, it was not found in these sources: **UNVERIFIED / likely nonexistent in this target**. (Internally 1.21.11 introduced `Permission` classes, but ops.json and server.properties formats did not change - also **UNVERIFIED**.)

### 3.3 `/execute`, `/data`, scoreboard
- `/execute` syntax list (26.3): `align`, `anchored`, `as`, `at`, `facing`, `in`, `on`, `positioned [as|over]`, `rotated [as]`, `summon`; conditions `biome`, `block`, `blocks`, `data`, `dimension`, `entity`, `function`, `items`, `loaded`, `predicate`, `score`, `stopwatch` (1.21.11), `slots` (26.3); `store (result|success) block|bossbar|entity|score|storage`; `run`. No removals since 1.21.4.
- `/data`: heterogeneous lists, no empty-key traversal (1.21.5); `compute` source (26.3). `/data modify ... set value <snbt>` accepts expanded SNBT.
- Scoreboard: no command-syntax changes found except SNBT styles in `numberformat styled` (1.21.5), text arg resolution in `@s` context, team colour lowercase (26.2), tab list score hidden if objective missing (26.2), `below_name` slot fixed for non-player entities (26.1, MC-99647). MC-254370: `entity_scores` predicate cannot check all scores (open as of 26.3).

### 3.4 Functions & macros
- No changes recorded for macro syntax (`$(key)`, `$` lines), `#minecraft:load|tick`, `/function <id> [<args>|with <source> [<path>]]`, `/return`, `execute if function` between 1.21.4 and 26.3. Directory `data/<ns>/function/` and `tags/function/`. New macro-expansion consumer: dialog `command_template`/`dynamic/run_command` (1.21.6).
- 1.21.5 `/setblock`-style rule changes and `execute` `strict` do not require function syntax changes. Fresh `schedule clear` requires a namespace (known bug).

### 3.5 Text components (current 26.3 shapes)
`{text:"..."}`, `{translate:"key",fallback:"...",with:[...]}`, `{score:{name:"*"|<sel>,objective:"o"}}`, `{selector:"@a",separator:{text:", "}}`, `{keybind:"key.inventory"}`, `{nbt:"path",entity:"@s"|block:"x y z"|storage:"ns:id",interpret:false,plain:false,separator:{...}}`, `{object:"atlas",atlas:"minecraft:blocks",sprite:"...",fallback:{...}}` / `{object:"player",player:{name:"..."},hat:true}`. Style fields: `color`, `bold`, `italic`, `underlined`, `strikethrough`, `obfuscated`, `font`, `shadow_color`, `insertion`, `click_event`, `hover_event`. Written as SNBT in commands (quotes optional on keys, strings `'x'` or `"x"`). In JSON data files (e.g. loot `set_name`, advancement `display.title`) components are plain JSON objects/strings (no stringified JSON). (`shadow_color`, `font` etc. are older and unchanged; not re-verified for 26.3, **UNVERIFIED**.)

### 3.6 Item components
Item syntax in commands: `give @s stick[custom_name={text:'x'},enchantments={sharpness:2}]`; component removal `[!custom_name]`; predicates `*[custom_data~{a:1}]` and since 1.21.11 existence-only `*[instrument]`. 1.21.5 simplified encodings (above) apply both to commands and to JSON files. 26.3 adds many new/changed components (section 2.8).

### 3.7 Time-related summary
- 26.1 world clocks/timelines/time markers (section 2.6); 1.21.11 timelines (`period_ticks`, `tracks`); `default_clock`; `/time rate`; game test `clock_time`; `time_check` predicate `clock`; End skylight flashes controlled by clock `minecraft:the_end`.
- Stopwatch (1.21.11): real-time, independent of ticks (use for wall-clock timers; not for gametime).
- `/tick` command unchanged except error semantics (26.3). `/worldborder` time args ticks (1.21.11).
- `daylight cycle`: game rule `advance_time`; `advance_weather`. Slab should avoid assuming `time set day` == 1000 ticks (see UNVERIFIED in 2.6).

---

## 4. Open uncertainties (summary)
1. Exact comparison semantics of `max_format` with minors; recommended single-target form `min_format=max_format=121`.
2. (Resolved: namespaced gamerule ids are accepted.)
3. `/time set <marker>` semantics (next occurrence vs absolute) and Java marker tick values.
4. Whether any permission-level rework exists (none found).
5. Release dates before 26.1 (from memory).
6. Exact JSON for advancement trigger entity-predicate wrapping in 26.3 (derived).
7. Number-provider folder names after the int/float split.
8. `equippable.equip_on_interact` vs `equip_on_interaction`.
9. 26.3 wiki text beyond char 400000 was not read; `/data` / `/execute` details come from misode.

## 5. Source index
- misode technical changes (repo): https://github.com/misode/technical-changes (folders `1.21.5`, `1.21.6`, `1.21.7`, `1.21.9`, `1.21.11`, `26.1`, `26.2`, `26.3`, `26.4`; tag list in `tags.json`); rendered: https://misode.github.io/changelog/
- https://minecraft.wiki/w/Pack_format
- https://minecraft.wiki/w/Data_pack
- https://minecraft.wiki/w/Java_Edition_1.21.5 , /1.21.6 , /1.21.9 , /1.21.11 , /26.1 , /26.2 , /26.3
- https://minecraft.wiki/w/Game_rule , https://minecraft.wiki/w/Commands/gamerule
- https://minecraft.wiki/w/Commands/time , https://minecraft.wiki/w/Commands/execute
- https://minecraft.wiki/w/Text_component_format
- https://minecraft.wiki/w/Function_(Java_Edition)
- https://minecraft.wiki/w/Server.properties , https://minecraft.wiki/w/Permission_level , https://minecraft.wiki/w/Ops.json_format
- https://minecraft.wiki/w/Java_Edition_version_history
