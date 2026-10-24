# 04 - Data formats used inside data packs (Minecraft Java 26.3)

Target: Java Edition **26.3 "Wilderness Bound"** (released 2026-09-15, data pack format **121.0**, resource pack 97.1).
Research date: 2026-10-22.

## Source policy / confidence legend

Primary sources (fetched live, wikitext via `?action=raw`):
- https://minecraft.wiki/w/Java_Edition_26.3 , /26.2 , /26.1 (release changelogs - the authoritative description of 26.x changes)
- https://minecraft.wiki/w/Predicate , /w/Data_component_format (+ subpages), /w/Data_component_predicate, /w/Text_component_format,
  /w/NBT_format, /w/Advancement_definition, /w/Loot_table, /w/Item_modifier, /w/Recipe_(Java_Edition), /w/Dialog,
  /w/Number_provider, /w/Block_predicate, /w/Entity_format (+ /Entity, /Mob), /w/Display, /w/Armor_Stand, /w/Marker, /w/Interaction,
  /w/Commands/compute, /w/Commands/data, /w/Argument_types
- **Ground truth for 26.3 JSON shapes**: the vanilla 26.3 data pack and registries as dumped by misode/mcmeta at the "Update data/registries for 26.3" commits
  (`data` @ 538b2b167248c648b2198f2c0d56eced10dfc0cf, `registries` @ 2240df2376509bfaf12becbb36e156e29f8ecb4d):
  https://github.com/misode/mcmeta . Wherever the wiki prose and the vanilla data disagree, **vanilla data wins** and the
  disagreement is noted. Registry lists below (loot condition types, loot function types, trigger types, data components, ...) come from these dumps.

**IMPORTANT staleness warning:** several wiki pages were NOT yet updated for the 26.3 renames (Advancement_definition, Item_modifier, Argument_types,
Loot_table tag entry, Recipe cookingtime defaults, Number_provider `conditions`). Never copy examples from those pages without applying the
"26.3 breaking changes" table below.

Markers: `**UNVERIFIED**` = could not confirm in 26.3 data/changelog; `(wiki-stale)` = old wiki text known to be wrong for 26.3.

---------------------------------------------------------------------

## 0. 26.3 breaking changes cheat-sheet (what a compiler must emit NOW)

These are all in the 26.3 changelog (https://minecraft.wiki/w/Java_Edition_26.3, snapshots 26.3-snapshot-3 .. pre-1) and confirmed against vanilla 26.3 data unless noted.

| Area | Old (<= 26.2) | New (26.3) |
|---|---|---|
| Predicate type key | `"condition": "minecraft:foo"` | `"type": "minecraft:foo"` (**mandatory**; no longer defaults to `entity_properties`) |
| Loot function type key | `"function": "minecraft:set_count"` | `"type": "minecraft:set_count"` |
| Condition list on functions/entries/pools | `"conditions": [ ... ]` (list = implicit all_of) | `"condition": <one predicate or predicate-id>`; **a bare list is no longer accepted** - wrap in `{"type":"minecraft:all_of","terms":[...]}` |
| Functions on entries/pools/tables | `"functions": [ ... ]` | `"modifier": <function | id | list>` (field now exists on ALL entry types incl. alternatives/sequence/group). Single object OR list accepted |
| `minecraft:reference` (predicate / item modifier / slot source) | existed | **removed**: anywhere a predicate/modifier/slot_source is expected you may now write an ID string `"ns:name"` (or a `#tag`, or a list of ids/inline mixed, depending on the field) |
| `block_state_property` condition | existed | **removed** -> use `match_block` (`blocks` + `state`) |
| `value_check` condition | existed | **removed** -> `int_value_check` / `float_value_check` |
| Number providers | one registry `number_provider`; `{min,max}` shorthand = uniform; `type` defaulted to uniform | two registries `context_int_provider` / `context_float_provider`; inline provider **always needs explicit `type`** (plain number = constant still OK) |
| `sum`/`product`/`minimum`/`maximum`/`average` | names; `summands` | `add`/`mul`/`min`/`max`/`avg`; field `inputs` |
| Cooking recipes | `cookingtime` optional (defaults 200/100) | **`cookingtime` mandatory** in smelting/blasting/smoking/campfire_cooking. Smoker/blast recipes should now use 2x their old value (200 for the vanilla-equivalent) because speed-up is done by fuel component |
| `tag` loot entry | `name` | `items` (single id, list, or `#tag`) - verified in vanilla (`creeper.json`) |
| `set_loot_table` fn | `name`, `type` | `loot_table_id`; `type` removed |
| `exploration_map` fn | `map_color`, optional destination, turns `map`->`filled_map` | `map_color` removed; `destination` mandatory; does not change item type (loot entry must already be `filled_map`); use `filtered`+`discard` if no structure found |
| Advancement triggers | `block`, `recipe_id`/`recipe`, `loot_table` | `bee_nest_destroyed`/`enter_block`/`slide_down_block`: `blocks`; `recipe_crafted`/`crafter_recipe_crafted`/`recipe_unlocked`: `recipes`; `player_generates_container_loot`: `loot_tables` |
| Advancement `player` field & entity fields | compound or list of predicates | single inline predicate **with `type`** (e.g. `{"type":"minecraft:entity_properties","entity":"this","predicate":{...}}`) or predicate id |
| Advancement `display.background` | any | must be present on visible root advancements, and ONLY allowed on roots |
| Block state object (falling_block, block_display, ...) | `{Name:"...", Properties:{...}}` | `{id:"...", properties:{...}}`; a plain string `"minecraft:stone"` = default state |
| `pot_decorations` component, `decorated_pot.sherds` | list of 4 item ids | object `{back,left,right,front}` of optional item stacks |
| `swing_animation` component | existed | removed -> `attack_animation` + `interact_animation` |
| `map_color` component | existed | removed |
| `minecraft:trim_material` inline def | `asset_name`, `override_armor_assets` | `palette_id`; overrides now live in resource-pack equipment asset |
| Signs | click events in sign text always ran | block entity field `allow_op_features` (default false) must be true for click events/commands in signs to work; new items/blocks need it set explicitly |
| `brewing_stand`/furnace BE fields | shorts/bytes | ints (`BrewTime`, `Fuel`, `cooking_time_spent`, `cooking_total_time`, `lit_time_remaining`, `lit_total_time`), new `speed_multiplier`, `total_brew_time`, `total_fuel` |
| `#brewing_fuel` item tag | defined fuels | removed; fuels are `brewing_fuel` / `cooking_fuel` components |
| `tellraw` | - | `nbt` text components are resolved at most 64k times |
| Data pack lists/tags | single-form fields | "element lists/tags" codec now allows mixing inline values and references, e.g. `[{type:set_count,...}, "minecraft:some_modifier"]` |

Also new in 26.3: `/compute`, `data modify ... compute`, `execute if|unless slots`, `item fill|override`, slot-source files (`slot_source/`), `context_int_provider/` & `context_float_provider/` data pack folders, `block_transformer/` folder, `predicate/` registry now populated in vanilla (`tool/can_silk_touch`, `tool/can_shear`, `block/fast_cooking`), `height_range` block predicate, brewing recipe type, `invulnerable_time` entity tag.

---------------------------------------------------------------------

## 1. NBT and SNBT

Source: https://minecraft.wiki/w/NBT_format (SNBT section + History)

### Tag types
byte(8) `b`/`B`, short(16) `s`/`S`, int(32) none/`i`/`I`, long(64) `l`/`L`, float `f`/`F`, double (no suffix if has decimal point, or `d`/`D`), string, list, compound, byte array `[B;...]`, int array `[I;...]`, long array `[L;...]`.

### SNBT syntax
- **Compound**: `{key:value,key2:value2}`. Keys unquoted if only `[a-zA-Z0-9_\-.+]`, else quoted (`"` or `'`). Trailing commas allowed (1.21.5+).
- **List**: `[a,b,c]`. **Since 1.21.5 (25w04a/25w09a) SNBT lists may be heterogeneous** (`[1,"abc"]`); when written to binary NBT, non-compound entries of a mixed list are wrapped as `{"":value}` compounds. (Before, mixed lists errored. 1.20.5 already did the wrap for `custom_data` written from JSON.) Compiler consequence: never rely on list element-type homogeneity checks, but do not emit mixed lists into real NBT paths that vanilla codecs read (they expect typed lists).
- **Typed arrays**: `[B;1b,2b,3b]`, `[I;1,2,3]`, `[L;1l,2l]`. `[1b,2b]` is a *list*, not a byte array. Byte array elements must be byte tags; **booleans are not allowed** in `[B;...]`. Int array accepts byte/short/int tags; long array accepts byte/short/int/long.
- **Booleans**: `true` / `false` are byte `1b` / `0b` (NBT has no bool). Many fields marked boolean accept either.
- **Strings**: `"..."` or `'...'`; unquoted allowed iff string matches `[0-9A-Za-z_\-.+]+` and does not start with `0-9`, `-`, `.`, `+`. Escapes (1.21.5, 25w09a) - 13 sequences: `\b \f \n \r \s \t \\ \' \" \xHH \uHHHH \UHHHHHHHH \N{name}` (`\s` = space).
- **Numbers (25w09a+)**: freeform: `.1`, `1.`, `2e3`, `1.2e3`, `123_456` (underscores between digits), `0xCAFE` hex, `0b1001` binary; type suffix as above. Signedness suffixes `s`/`u` *before* type suffix on integers only: `240ub` == `-16b`, `15sS` == short 15 (`240uB`, `-16sb`). Invalid: `82u`, `-87uI`, `30bu`.
- **Operations**: `bool(x)` (number or bool -> byte 0/1; strings error), `uuid("f81d4fae-7dec-11d0-a765-00a0c91e6bf6")` -> `[I;...]` int array. Added 25w10a.
- Number typing matters: `{Item:{count:1}}` int vs `1b`/`1d` mismatches fail NBT matches.
- **Partial-match semantics** (`execute if data`, `@e[nbt=]`, `custom_data` predicate): compound subset match; **list matching is also subset-style and order/length-insensitive** (`Pos:[2d]` matches); arrays `[I;]` are exact. Value type must match exactly (`1` int != `1d`); IDs need namespaces (`"minecraft:stone"`).
- JSON <-> NBT: JSON numbers pick the narrowest of byte/short/int/long/float/double; JSON arrays with all-same converted type become typed arrays (byte/int/long) else list. Used by JSON data-pack files that embed NBT (e.g. `custom_data` in loot functions). Strings of SNBT are accepted where a "compound or SNBT string" is documented (e.g. `custom_data`, `nbt` in entity/block predicates).

### What changed in 1.21.5+ that affects a compiler
1. **Text components are NBT/SNBT, not JSON strings**, everywhere they are stored or passed to commands (see section 2).
2. Item stacks use `{id, count, components}` (see section 4); `count` int; NBT-form items in containers add `Slot` (byte) in block/entity inventories only.
3. SNBT accepts everything JSON object/array/string syntax accepts (quoted keys, `true/false`), so existing JSON-like command arguments still parse, but numbers have SNBT typing.
4. Hex/binary/underscore numbers, escapes, heterogeneous lists, trailing commas (above).
5. 26.3: block state NBT renamed `Name`/`Properties` -> `id`/`properties` (also plain-string shorthand).

---------------------------------------------------------------------

## 2. Text components

Sources: https://minecraft.wiki/w/Text_component_format (+ History), https://minecraft.wiki/w/Java_Edition_26.1 (nbt/object/selector changes), 26.3 changelog (`tellraw` 64k).

### Form
A component is a **string** (= `{text:"..."}`), a **list** (first element is parent; the remaining ones are appended to its `extra`, so style inherits: `["A","B"]` == `{text:"A",extra:["B"]}`), or a **compound**. Written as SNBT in commands (`/tellraw @a {text:"hi",color:"red"}`; JSON-style `{"text":"hi"}` is still valid SNBT) and as NBT (not JSON strings) in data components, entity NBT (`CustomName`, text_display `text`), signs, books. In JSON data-pack files it is ordinary JSON (string/array/object). A JSON-encoded string containing JSON text (`'{"text":"x"}'`) is **no longer parsed** - it would be literal text.

### Content (determined by `type` or autodetected from keys in this order: `text`, `translate`, `score`, `selector`, `keybind`, `nbt`; `object` keys)
`type` values: `text`, `translatable`, `score`, `selector`, `keybind`, `nbt`, `object`.

| type | fields |
|---|---|
| `text` | `text` (string) |
| `translatable` | `translate` (key), `fallback` (string, optional), `with` (list of components; `%s`, `%1$s` slots) |
| `score` | `score:{name, objective}` - `name` is a score holder, selector (must be single entity), or `"*"` (reader's own score). Resolved server side |
| `selector` | `selector` (string), `separator` (component, default `{color:"gray",text:", "}`) - no trailing data allowed after selector (26.1) |
| `keybind` | `keybind` (e.g. `key.inventory`), client-side |
| `nbt` | `nbt` (path), source = one of `block` (coords), `entity` (selector), `storage` (id) with optional explicit `source: "block"/"entity"/"storage"`; `interpret` (bool, default false: treat NBT value as a text component), `plain` (bool, default false, 26.1: no syntax coloring of pretty-printed SNBT; cannot be combined with `interpret:true`), `separator` (default `{text:", "}`). 26.1: non-interpreted output is pretty-printed; parse failures of `nbt`/`block` no longer silently ignored. **26.3: resolved at most 64k times per `tellraw`** |
| `object` (1.21.9+) | sprite as 8x8 char. `object:"atlas"` (default): `atlas` (default `minecraft:blocks`), `sprite` (e.g. `block/emerald_block`). `object:"player"`: `player` (string name or profile compound like the `profile` component), `hat` (bool, default true). 26.1 pre1: optional `fallback` (component used where objects can't render, e.g. logs/narration). Bold/italic ignored on sprites. |

Server-side resolution ("advanced") components: `score`, `selector`, `nbt` are resolved by the server in `tellraw`/`title`/`bossbar`/`scoreboard`/`team`, signs, text_display `text`, `set_name`/`set_lore` (only when their `entity` is set), and bossbar/objective/numberformat commands run as an entity. They are **NOT resolved** in: direct writes to `custom_name`/`item_name`/`lore`/`written_book_content` components, `CustomName` NBT, **dialogs** (no nbt/score/selector in dialogs), most data-pack JSON (advancement titles, chat types). Resolution is a snapshot, not live.

### Style fields (inherited by `extra` children unless overridden)
- `color`: named (`black dark_blue dark_green dark_aqua dark_red dark_purple gold gray dark_gray blue green aqua red light_purple yellow white`) or `"#RRGGBB"`.
- `bold italic underlined strikethrough obfuscated`: booleans. (`italic:false` to defeat the default italic of custom names.)
- `font`: resource location (default `minecraft:default`).
- `shadow_color` (1.21.4+): int ARGB (e.g. `-16777216`) **or** list of 4 floats `[r,g,b,a]` 0..1.
- `insertion`: string inserted into chat input on shift-click.
- `click_event` / `hover_event` (renamed from camelCase `clickEvent`/`hoverEvent` in 1.21.5).
- Structure: `extra` (list of components), `separator` (selector/nbt only).

### click_event (`action` key)
| action | fields |
|---|---|
| `open_url` | `url` (must be http/https URI) |
| `open_file` | `path` (game-generated only; not usable from servers) |
| `run_command` | `command` (no leading `/` needed; run as the clicking player with their permissions, cannot send chat; on signs run by the server at the sign with the clicker as `@s`, command-block permission - requires sign `allow_op_features:true` in 26.3) |
| `suggest_command` | `command` |
| `change_page` | `page` (positive int; books only) |
| `copy_to_clipboard` | `value` |
| `show_dialog` (1.21.6) | `dialog` (id string or inline dialog) |
| `custom` (1.21.6) | `id` (namespaced), `payload` (optional NBT; **no effect on vanilla servers**) |

### hover_event (`action` key)
| action | fields |
|---|---|
| `show_text` | `value` (component; click/hover inside the tooltip don't work) |
| `show_item` | `id`, `count` (opt), `components` (opt; data component map) |
| `show_entity` | `id` (entity type), `uuid` (hyphenated string or int array/list of 4), `name` (opt component) |

Old (pre-1.21.5) fields `contents`, `value` of click events, `type`/`id` swap in show_entity are gone.

Example: `/tellraw @a [{text:"Score: ",color:"gold"},{score:{name:"@s",objective:"pts"},bold:true},{text:" [click]",click_event:{action:"run_command",command:"trigger menu"},hover_event:{action:"show_text",value:"Open menu"}}]`

---------------------------------------------------------------------

## 3. Predicates and friends

Sources: https://minecraft.wiki/w/Predicate (updated to 26.3), 26.3 changelog "Predicate"/"Number providers", vanilla `data/minecraft/predicate/*.json`, https://minecraft.wiki/w/Advancement/Conditions/{entity,item,location,block,damage_type} (via Template:Nbt inherit/conditions/*), https://minecraft.wiki/w/Data_component_predicate, https://minecraft.wiki/w/Loot_context.

### 3.1 Predicate files and references
- File: `data/<ns>/predicate/<path>.json` -> id `ns:path`. The file root is **one condition object** (or—since 26.3 "element list" codec—what a `minecraft:predicate` element codec accepts). Vanilla 26.3 ships `minecraft:tool/can_silk_touch`, `minecraft:tool/can_shear`, `minecraft:block/fast_cooking`.
- Used by: `/execute if predicate <id | inline SNBT>`, selector `predicate=<id>` (once per candidate entity, at that entity), loot tables/functions/entries (`condition`), advancements (`player` & others), number providers (`conditional`, `number_dispatcher`).
- **26.3: wherever a predicate is expected you can write the id string** (`"condition": "minecraft:tool/can_silk_touch"`) - this replaces `minecraft:reference`. `all_of.terms`/`any_of.terms` accept: inline predicate, one id, list of ids, list of inline, or `#predicate_tag`. A bare JSON array is no longer an implicit `all_of`.
- Command inline form (needs `type` now): `execute if predicate {type:"minecraft:weather_check",raining:true}` (**Argument_types wiki examples still show old `condition:`**).

### 3.2 Condition types (loot_condition_type registry in 26.3)
`all_of, any_of, damage_source_properties, enchantment_active_check, entity_properties, entity_scores, environment_attribute_check, float_value_check, int_value_check, inverted, killed_by_player, location_check, match_block, match_tool, random_chance, random_chance_with_enchanted_bonus, survives_explosion, table_bonus, time_check, weather_check`
(NOT present: `reference`, `value_check`, `block_state_property`, `alternative`, `below_heightmap` (that is 26.4 snap2).)

Every object has `"type": "minecraft:<name>"`.

| type | fields | context needed / notes |
|---|---|---|
| `all_of` | `terms`: predicate(s) (see 3.1) | any |
| `any_of` | `terms` | any |
| `inverted` | `term`: predicate | any |
| `entity_properties` | `entity`: `this`/`attacker`/`direct_attacker`/`attacking_player`/`target_entity`/`interacting_entity`; `predicate`: entity predicate (3.3) | needs that entity in context |
| `entity_scores` | `entity` (same targets); `scores`: `{objective: N}` or `{objective:{min:?,max:?}}`; N/min/max may be an int or a `context_int_provider` id/inline | fails if entity missing |
| `killed_by_player` | (none) | needs `attacking_player` |
| `location_check` | `offsetX`,`offsetY`,`offsetZ` (int, optional); `predicate`: location predicate | needs `origin` |
| `match_block` (new 26.3) | all optional: `blocks` (id / list / `#tag`), `state` (property->value or `{min,max}`), `nbt` (block entity SNBT/compound), `components` (exact), `predicates` (component predicates) | `nbt/components/predicates` fail if no block entity in context. Replaces `block_state_property` |
| `match_tool` | `predicate`: item predicate (3.5) | needs `tool` |
| `damage_source_properties` | `predicate`: damage source predicate (3.7) | needs damage source & origin |
| `survives_explosion` | (none) | probability 1/radius; always true w/o radius |
| `table_bonus` | `enchantment`, `chances` (list indexed by level from 0) | uses `tool` |
| `random_chance` | `chance`: float or `context_float_provider` id/inline (e.g. `{type:"minecraft:uniform",min:0,max:1}`) | any |
| `random_chance_with_enchanted_bonus` | `unenchanted_chance` (float), `enchanted_chance` (level-based value e.g. `{type:"minecraft:linear",base:0.035,per_level_above_first:0.01}`), `enchantment` | level taken from `attacker` |
| `time_check` | `clock` (world clock id: `minecraft:overworld`, `minecraft:the_end`; 26.1+), `value`: int / provider / `{min,max}` (each may be provider), `period` (int, optional: day time mod period first) | any. Compares on that clock |
| `weather_check` | `raining` (bool), `thundering` (bool) | any |
| `int_value_check` | `value`: int or `context_int_provider`; `test`: int/provider or `{min,max}` | any (replaces `value_check`) |
| `float_value_check` | `value`: float or `context_float_provider`; `test` | any |
| `enchantment_active_check` | `active` (bool) | only `enchanted_location` context |
| `environment_attribute_check` (26.1) | `attribute` (environment attribute id), `value` (exact value to match) | needs `origin` if attribute varies by position. e.g. `{type:"environment_attribute_check",attribute:"gameplay/piglins_zombify",value:true}` |

Examples (26.3 shape):
```json
{ "type": "minecraft:entity_properties", "entity": "this",
  "predicate": { "minecraft:equipment": { "mainhand": { "items": "minecraft:diamond_sword" } } } }
{ "type": "minecraft:time_check", "clock": "minecraft:overworld", "value": { "min": 0, "max": 12000 }, "period": 24000 }
{ "type": "minecraft:int_value_check", "value": { "type": "minecraft:score", "target": {"type":"minecraft:fixed","name":"#game"}, "score": "state" }, "test": 3 }
```
**UNVERIFIED**: Whether `time_check.clock` is mandatory in 26.3 (changelog: "added clock field"); wiki lists it without "optional". Safe to always emit it.

### 3.3 Entity predicate (26.2 "component-map" form; used by `entity_properties.predicate` and advancement entity fields; selectors use `@e[...]` args instead)
Since 26.2-snap3 the entity predicate is a **map of sub-predicate id -> value**; ids default to `minecraft:` namespace, so old field names still work, with exceptions below. **Unknown keys are now rejected.** Registry `entity_sub_predicate_type`:

| key (`minecraft:` prefix optional) | value |
|---|---|
| `entity_type` (was `type`!) | entity id / list / `#tag` |
| `components` | exact data component map (variants, `custom_name`, ...) |
| `predicates` | data component predicate map (e.g. `{"minecraft:custom_name":{}}` existence) |
| `distance` | `{absolute|horizontal|x|y|z: {min,max}}` (to the "origin" of the predicate invocation) |
| `effects` | `{ "minecraft:speed": {amplifier:{min,max}|N, duration:{min,max}|N, ambient:bool, visible:bool} }` |
| `entity_tags` (26.2) | `{any_of:[..], all_of:[..], none_of:[..]}` - **scoreboard tags** (`/tag`) |
| `equipment` | `{mainhand|offhand|head|chest|legs|feet|body|saddle: <item predicate>}` |
| `flags` | booleans: `is_baby is_on_fire is_sneaking is_sprinting is_swimming is_on_ground is_flying is_fall_flying` (1.21.11 added more values - **UNVERIFIED**) |
| `location` | location predicate (entity's position) |
| `movement` | `{x,y,z,speed,horizontal_speed,vertical_speed,fall_distance: number|{min,max}}` (blocks/s) |
| `movement_affected_by` | location predicate (block 0.5 below the stepped-on block) |
| `stepping_on` | location predicate (block stood on; true only if on ground) |
| `nbt` | SNBT string or compound (slowest, last) |
| `passenger` | entity predicate (any direct passenger) |
| `vehicle` | entity predicate |
| `targeted_entity` | entity predicate (mob's attack target) |
| `periodic_tick` | int > 0: entity age divisible by it |
| `slots` | `{<slot range e.g. "container.*" | "weapon.mainhand">: <item predicate>}` |
| `team` | team name string |
| `type_specific/player` | `looking_at` (entity predicate), `advancements` (`{id: bool | {criterion: bool}}`), `gamemode` (list), `level` (int|{min,max}), `recipes` (`{id: bool}`), `stats` (list of `{type, stat, value}`), `input` (`forward backward left right jump sneak sprint`), `food` (`{level, saturation}`, 26.1) |
| `type_specific/fishing_hook` | `in_open_water` |
| `type_specific/lightning` | `blocks_set_on_fire`, `entity_struck` (entity predicate) |
| `type_specific/raider` | `is_captain`, `has_raid` |
| `type_specific/sheep` | `sheared` |
| `type_specific/cube_mob` (was `slime`) | `size` (int|{min,max}; 1 = smallest) - slime, magma cube, sulfur cube |

Old->new example: `{"type":"minecraft:zombie","equipment":{...}}` -> `{"minecraft:entity_type":"minecraft:zombie","minecraft:equipment":{...}}`; `{"type_specific":{"type":"minecraft:player",...}}` -> `{"minecraft:type_specific/player":{...}}`.

### 3.4 Location predicate
`biomes` (id/list/#tag), `block` (block predicate 3.6), `dimension`, `fluid` (`{fluids, state}`), `light` (`{light: int|{min,max}}`), `position` (`{x,y,z: number|{min,max}}`), `smokey` (bool), `can_see_sky` (bool), `structures` (id/list/#tag).

### 3.5 Item predicate (condition-style; also `match_tool.predicate`, advancement `item`)
```
{ "items": id | [ids] | "#tag", "count": N | {min,max},
  "components": { "<component id>": <exact value> },       // exact match
  "predicates": { "<component predicate id>": <value> } }  // loose match
```
(`tag`, `nbt`, `potion`, `durability`, `enchantments` top-level fields are gone since 1.20.5/1.21.) Since 1.21.11 `predicates` supports an **existence check for any component type**: `{"predicates":{"minecraft:map_id":{}}}` (seen in vanilla `exploration_map` filters).

Command form (`item_predicate` argument): `item_or_#tag_or_*[ test, test | test ]` where test = `component_id` (exists), `component_id=SNBT` (exact), `predicate_id~SNBT` (component predicate), `count` / `count=N` / `count~{min:,max:}`; `!test` negates; `,` = AND, `|` = OR (**OR binds tighter than AND**). E.g. `*[!damage|damage=0, count=1]`.
Item stack argument: `id[comp=value,!comp_to_remove]`.

**Data component predicate types** (registry `data_component_predicate_type`): `attribute_modifiers, bundle_contents, container, custom_data, damage, enchantments, firework_explosion, fireworks, jukebox_playable, potion_contents, stored_enchantments, trim, villager/variant`, plus `writable_book_content`, `written_book_content` (wiki lists these; the registry dump has them too).
- `damage`: `{damage: N|{min,max}, durability: N|{min,max}}` (durability = max_damage - damage).
- `enchantments` / `stored_enchantments`: list of `{enchantments: id|[ids]|#tag, levels: N|{min,max}}` (all list entries must match).
- `custom_data`: SNBT string or compound (partial match).
- `container`/`bundle_contents`: `{items: <collection predicate of item predicates>}`. Collection predicate: `{contains:[...], count:[{test:..., count:N|{min,max}}], size:N|{min,max}}`.
- `potion_contents` (**changed 26.3**): `{potions: id|[ids]|#tag, effects: <collection predicate of mob_effects maps>}`; mob_effects map: `{"minecraft:speed":{duration,amplifier,ambient,visible}}`.
- `jukebox_playable`: `{song: id|[ids]|#tag}`; `trim`: `{material, pattern}`; `fireworks`: `{explosions: collection, flight_duration}`; `firework_explosion`: `{shape, has_trail, has_twinkle, colors?, fade_colors?}` (**UNVERIFIED** exact fields); `attribute_modifiers`: `{modifiers: collection}`; `written_book_content`: `{pages: collection, author, title, generation, resolved}`; `writable_book_content`: `{pages: collection}`.

### 3.6 Block predicate (condition-style, for location/`match_block`/`can_break`/`can_place_on`)
`{blocks: id|[ids]|#tag, state: {prop: "value" | {min,max}}, nbt: SNBT|compound, components: {...}, predicates: {...}}` (all optional). Note `state` values are strings, e.g. `"level": "0"`.

**Placement-condition "block predicates" (worldgen/enchantments; a different system)** - registry `block_predicate_type`: `all_of, any_of, has_sturdy_face, height_range (NEW 26.3), inside_world_bounds, matching_biomes, matching_block_tag, matching_blocks, matching_fluids, not, replaceable, solid, true, unobstructed, volume_match, would_survive`.
`height_range`: `{"type":"minecraft:height_range","min_inclusive":<vertical anchor>,"max_inclusive":<vertical anchor>}`; vertical anchor = `{"absolute":N}` | `{"above_bottom":N}` | `{"below_top":N}`. Many accept `offset:[x,y,z]` (-16..16).

### 3.7 Damage source predicate
`{ "direct_entity": <entity pred>, "source_entity": <entity pred>, "is_direct": bool, "tags": [ {"id": "#minecraft:is_projectile" | id | [ids], "expected": true} ] }` (`tags.id` accepts one id, list, or `#tag` since 26.3). Advancement "damage" predicate: `{blocked: bool, dealt: N|{min,max}, taken: N|{min,max}, source_entity: <entity pred>, type: <damage source pred>}`.

### 3.8 Ranges
- **int range / double range** in predicates: a bare number (exact) or `{ "min": a, "max": b }` (both optional, inclusive). In 26.3 predicate int ranges in `entity_scores`, `time_check`, `*_value_check` may use providers instead of literals.
- Command args `int_range`/`float_range`: `N`, `a..b`, `a..`, `..b`.

### 3.9 Number providers (26.3: two registries) - also used by `/compute`
Docs: https://minecraft.wiki/w/Number_provider (+ 26.3 changelog). Stored as `data/<ns>/context_int_provider/<name>.json` and `data/<ns>/context_float_provider/<name>.json` (file may be just `2` / `2.0` / a string id / an object). Referenced by id string (`"minecraft:cooking/time_coal"`) or written inline. A plain JSON number is `constant`. **Inline objects must carry `type`.** The type names may omit `minecraft:`.
These are *not* the worldgen `int_provider_type`/`float_provider_type` (context-free).

Integer provider types (`context_int_provider_type`): `abs, add, avg, binomial, conditional, constant, div, environment_attribute, floor_div, floor_mod, from_float, max, min, mod, mul, negate, number_dispatcher, pow, score, storage, sub, uniform, weighted_list`
Float provider types (`context_float_provider_type`): `abs, add, avg, ceil, conditional, constant, cos, div, enchantment_level, environment_attribute, floor, from_int, length, max, min, mod, mul, negate, number_dispatcher, pow, round, sin, sqrt, storage, sub, truncate, uniform, weighted_list`

| type | fields | semantics |
|---|---|---|
| `constant` | `value` | (inline: bare number) |
| `uniform` | `min`,`max` (providers) | int: inclusive both; float: range |
| `binomial` (int only) | `n` (int provider), `p` (float provider 0..1) | successes of n coin flips |
| `add`/`mul`/`min`/`max`/`avg` | `inputs`: one provider, list, id, list of ids, or `#tag` (>=1 required) | int ops use 32-bit ints; overflow cancels the computation; `avg` int rounds toward 0; float `length` = sqrt(sum of squares) |
| `sub`,`div`,`mod`,`pow`,(int) `floor_div`,`floor_mod` | `left`,`right` (`pow`: `base`,`exponent`) | `div`/`mod` truncate toward 0; `floor_*` toward -inf; div/mod by 0 = error (command fails); int `pow`: negative exponent or 0^0 = error; float `mod` is standard fmod (changed 26.3) |
| `abs`,`negate` | `input` | int overflow cancels |
| `ceil`,`floor`,`round`,`truncate`,`sin`,`cos`,`sqrt` (float only) | `input` (radians for trig) | round: ties toward +inf |
| `from_float` (int) | `input` (float provider) | truncates toward 0; NaN/out of range = error |
| `from_int` (float) | `input` (int provider) | |
| `score` (int only) | `target`: `{type:"minecraft:fixed",name:"<holder>"}` or `{type:"minecraft:context",target:"this"|"attacker"|"direct_attacker"|"attacking_player"|"target_entity"|"interacting_entity"}` (a bare context-target string is also accepted); `score`: objective; `fallback` (int provider, default 0) | |
| `storage` | `storage` (id), `path` (NBT path), `fallback` (default 0) | non-number / multiple matches -> fallback. Non-matching numeric NBT type conversion "undefined" |
| `environment_attribute` | `attribute` (id; must be representable as int/float) | needs origin in context |
| `enchantment_level` (float only) | `amount`: level-based value | |
| `conditional` | `condition` (predicate: inline/id), `on_true`, `on_false` (optional, default 0) | **vanilla data uses `condition` (singular)**; changelog prose says `conditions` (wiki-stale) |
| `number_dispatcher` | `cases`: `[{condition: <predicate>, value: <provider>}]`, `default` (optional, default 0) | first matching case wins (vanilla `compostable/*` verified). Early-snapshot prose used `number_provider` for the case value; use `value` |
| `weighted_list` | `distribution`: `[{data: <provider>, weight: positive int}]` | |

Vanilla registered providers (ids under `minecraft:`): int - `brewing/uses_default`, `compostable/{always_add_one,low,low_medium,medium,medium_high}`, `cooking/{fast_burn_time_reduction_factor,normal_burn_time_reduction_factor,time_bamboo,time_blaze_rod,time_boats,time_coal,time_coal_block,time_dried_kelp_block,time_dry_plants,time_hanging_signs,time_lava_bucket,time_roots,time_wood_blocks,time_wood_items_extra_small,time_wood_items_large,time_wood_items_small,time_wood_slabs,time_wool,time_wool_carpets,time_wool_slabs}`; float - `brewing/speed_default`, `cooking/{fast_speed_multiplier,normal_speed_multiplier,speed_default}`.

`/compute` (op level 2): `compute <default | block <pos> | entity <selector>> integer <int_provider>` and `... float <float_provider> [scale]` (result = floor(value*scale), default scale 1.0; integer mode truncates at each step so can differ from float mode; evaluation failure e.g. div by 0 => command fails). Contexts: `command_compute_default` (origin = execution pos, `this` = `@s`), `command_compute_position` (adds block_state, block_entity), `command_compute_entity` (adds `target_entity`). `data modify <target> <path> <set|merge|append|prepend|insert> compute <target> <provider> [integer]` stores the exact int or float tag. Example from changelog: `data modify storage foo bar set compute entity @n[type=minecraft:armor_stand] {type:score,target:{type:context,target:target_entity},score:baz}`. Quirk: in `/compute entity` the executing entity is `this`; the selected entity is `target_entity`.

### 3.10 Level-based value (enchantments, `random_chance_with_enchanted_bonus.enchanted_chance`, `enchantment_level.amount`)
Number = constant, or `{type: clamped|exponent|fraction|levels_squared|linear|lookup, ...}`; `linear`: `{base, per_level_above_first}`.

---------------------------------------------------------------------

## 4. Data components

Sources: https://minecraft.wiki/w/Data_component_format (+ subpages `Data_component_format/<name>`), 26.1/26.2/26.3 changelogs, registry `data_component_type` @26.3 (117 entries), vanilla defaults from mcmeta `item_components` (summary branch; note that branch is now 26.4-snapshot so 26.4-only fields like `equip_sound`/`shearing_sound` may appear - ignore them for 26.3).

### 4.1 Item stack & syntax
- NBT/JSON item stack: `{ "id": "minecraft:stone", "count": 1, "components": { "minecraft:damage": 5, "!minecraft:food": {} } }`. `count` 1..99 (default 1). `id` != `minecraft:air`. In block-entity/entity inventories add `Slot` (byte). Loot-table/recipe result stacks: `{id, count?, components?}`.
- **Removing a default component**: in NBT/JSON/loot form prefix the key with `!` (value ignored, `{}`): `{"components":{"!minecraft:damage":{}}}` (that is the form in `set_components`). In command item_stack argument: `give @s diamond_sword[!attribute_modifiers, custom_name={text:"X"}]`.
- Component ids may omit `minecraft:` in commands. Values in commands are SNBT. Components not listed on a stack default to the item type's defaults.
- Components are only partly used by block entities (`container`->`Items` etc.); entities don't store components (they use NBT) but predicates/loot functions can read them.

### 4.2 Component table (id -> shape). All ids `minecraft:`; `ID` = identifier string; `TC` = text component; `dye` = one of 16 dye color names.
| id | shape |
|---|---|
| `additional_trade_cost` | int (transient; removed after villager trade generated) |
| `attack_animation` (26.3) | `{type: "whack"\|"stab", duration: int>=0}` (both optional; vanilla default `{}`; default type whack, duration 6) |
| `interact_animation` (26.3) | same as attack_animation |
| `attack_range` | `{min_reach,max_reach,min_creative_reach,max_creative_reach: float, hitbox_margin: float, mob_factor: float}` (defaults 0/3/0/5, 0.3, 1.0) |
| `attribute_modifiers` | list of `{id, type (attribute id), slot: any\|hand\|armor\|mainhand\|offhand\|head\|chest\|legs\|feet\|body\|saddle, operation: add_value\|add_multiplied_base\|add_multiplied_total, amount: double, display?: {type: default\|hidden\|override, value?: TC}}` |
| `banner_patterns` | list of `{pattern: ID or inline def, color: dye}` |
| `base_color` | dye (shield base) |
| `bees` | list of `{entity_data: {id,...}, min_ticks_in_hive: int, ticks_in_hive: int}` |
| `block_entity_data` | block entity NBT incl. `id` (excludes x,y,z). Placing needs op |
| `block_state` | map of property -> string value |
| `block_transformer` (26.3) | ID of `block_transformer` registry entry (`minecraft:axe\|hoe\|shovel`); datapack folder `block_transformer/`. Inline components removed. Entry = list of rules `{block_state_provider, sound?, particle?: none\|scrape\|wax_on\|wax_off, disallowed_faces?: [dirs], loot?: ID, drop_strategy?: from_middle\|clicked_face, transform_type?: single_block\|copper_chest, consume_on_use?: bool(true), item_damage_per_use?: int, update_from_neighbors?: bool(true)}` |
| `blocks_attacks` | `{block_delay_seconds, disable_cooldown_scale, damage_reductions:[{type: ID/[ids]/#tag, base, factor, horizontal_blocking_angle}], item_damage:{threshold,base,factor}, block_sound, disabled_sound, bypassed_by: #tag/ID/[ids]}` |
| `break_sound` | sound event ID or inline |
| `brewing_fuel` (26.3) | `{uses: int\|context_int_provider ID, speed_multiplier: float\|context_float_provider ID}` (vanilla: `uses:"minecraft:brewing/uses_default"`, `speed_multiplier:"minecraft:brewing/speed_default"`) |
| `cooking_fuel` (26.3) | `{burn_time: int\|provider ID (ticks), speed_multiplier: float\|provider ID}`; providers run in `container_process` context |
| `compostable` (26.3) | `{layers: int\|context_int_provider ID}` (required) |
| `bucket_entity_data` | `{NoAI,Silent,NoGravity,Glowing,Invulnerable,AgeLocked: bool, Health: float, Age: int, HuntingCooldown: long}` |
| `bundle_contents` | list of item stacks |
| `can_break` / `can_place_on` | block predicate or list of block predicates (`{blocks,state,nbt,...}`) |
| `charged_projectiles` | list of item stacks |
| `consumable` | `{consume_seconds: float(1.6), animation: none\|eat\|drink\|block\|bow\|spear\|crossbow\|spyglass\|toot_horn\|brush\|bundle\|trident, sound, has_consume_particles: bool, on_consume_effects: [consume effect]}`. Consume effect types: `apply_effects{effects:[{id,amplifier,duration,ambient,show_particles,show_icon}],probability}`, `remove_effects{effects}`, `clear_all_effects`, `teleport_randomly{diameter, directional_particles (26.3, default true)}`, `play_sound{sound}` |
| `container` | list of `{slot: int 0..255, item: item stack}` |
| `container_loot` | `{loot_table: ID, seed?: long}` |
| `custom_data` | compound (any keys) - the main user-data hook; accepts SNBT string too |
| `custom_model_data` | `{floats:[f], flags:[bool], strings:[s], colors:[int \| [r,g,b] floats]}` |
| `custom_name` / `item_name` | TC (`custom_name` shown italic by default; `item_name` default name, not anvil-erasable) |
| `lore` | list of TC (max 256) |
| `cushion/color` (26.3) | dye |
| `damage` / `max_damage` / `max_stack_size` / `repair_cost` | int (`damage` = uses consumed; max_stack_size 1..99) |
| `damage_resistant` | `{types: "#tag"/ID/[ids]}` |
| `damage_type` | damage type ID (damage dealt) |
| `death_protection` | `{death_effects:[consume effect]}` |
| `debug_stick_state` | map block ID -> property name |
| `dye` | dye color string |
| `dyed_color` | int RGB or `[r,g,b]` floats 0..1 |
| `enchantable` | `{value: int>0}` |
| `enchantment_glint_override` | bool |
| `enchantments` / `stored_enchantments` | map enchantment ID -> level int (flat map; no `levels` wrapper) |
| `entity_data` | entity NBT with `id` (spawn eggs, armor stands, etc.; excludes id, UUID, Passengers handling) |
| `equippable` | `{slot: head\|chest\|legs\|feet\|body\|mainhand\|offhand\|saddle, equip_sound, asset_id, allowed_entities: ID/[ids]/#tag, dispensable, swappable, damage_on_hurt, equip_on_interact, camera_overlay, can_be_sheared, shearing_sound}` (`equip_sound`/`shearing_sound` are 26.4 additions) |
| `firework_explosion` | `{shape: small_ball\|large_ball\|star\|creeper\|burst, colors:[int], fade_colors:[int], has_trail, has_twinkle}` |
| `fireworks` | `{flight_duration: byte, explosions:[firework_explosion]}` (max 256) |
| `food` | `{nutrition: int, saturation: float, can_always_eat: bool}` |
| `glider`, `intangible_projectile`, `unbreakable`, `waxed` (26.3) | `{}` markers |
| `instrument` | instrument ID or inline (`durability_damage` optional field added 26.3) |
| `item_model` | model definition ID |
| `jukebox_playable` | jukebox song ID |
| `kinetic_weapon` | `{delay_ticks, damage_conditions, dismount_conditions, knockback_conditions, forward_movement, damage_multiplier, sound, hit_sound}` (conditions: kinetic weapon conditions object) |
| `piercing_weapon` | `{deals_knockback: bool(true), dismounts: bool(false), sound, hit_sound}` |
| `weapon` | `{item_damage_per_attack: int(1), disable_blocking_for_seconds: float(0)}` |
| `minimum_attack_charge` | float 0..1 |
| `lock` | item predicate (key) |
| `lodestone_tracker` | `{target?: {pos:[I;x,y,z], dimension}, tracked: bool(true)}` |
| `map_decorations` | map key -> `{type, x: double, z: double, rotation: float}` |
| `map_id` | int |
| `mob_visibility` (26.3) | `{targeting_entity_types: ID/[ids]/#tag, visibility: float 0.0..10.0}` (vanilla data uses `targeting_entity_types`; changelog says `targeting_entity`) |
| `note_block_sound` | sound ID |
| `ominous_bottle_amplifier` | int 0..4 |
| `pot_decorations` (26.3 shape) | `{back,left,right,front: item stack}` each optional |
| `potion_contents` | ID string (=potion) or `{potion?: ID, custom_color?: int, custom_name?: string, custom_effects?: [{id, amplifier(byte), duration(int, -1=inf), ambient, show_particles, show_icon}]}` |
| `potion_duration_scale` | float |
| `profile` | string (name) or `{name?, id?: [I;uuid], properties?: [{name:"textures",value,signature?}], texture?, cape?, elytra?, model?: wide\|slim}` |
| `provides_banner_patterns` | pattern ID/[ids]/#tag |
| `provides_pottery_pattern` (26.3) | `decorated_pot_pattern` ID |
| `provides_trim_material` | trim material ID or inline (`palette_id` in 26.3) |
| `rarity` | `common\|uncommon\|rare\|epic` |
| `recipes` | list of recipe IDs (knowledge book) |
| `repairable` | `{items: ID/[ids]/#tag}` |
| `sign_text_front`, `sign_text_back` (26.3) | `{messages:[TC x4], filtered_messages?:[TC], color?: dye(black), has_glowing_text?: bool}` (messages and filtered must have equal counts) |
| `sulfur_cube_content` (26.2) | item stack (doubles as the cube's body slot) |
| `suspicious_stew_effects` | list of `{id, duration (default 160)}` |
| `tool` | `{default_mining_speed: float(1), damage_per_block: int(1), can_destroy_blocks_in_creative: bool(true), rules:[{blocks: ID/[ids]/#tag, speed?, correct_for_drops?}]}` |
| `tooltip_display` | `{hide_tooltip: bool, hidden_components:[component IDs]}` |
| `tooltip_style` | ID (sprite set) |
| `trim` | `{material: ID/inline, pattern: ID/inline}` |
| `use_cooldown` | `{seconds: float, cooldown_group?: ID}` |
| `use_effects` | `{can_sprint: bool(false), speed_multiplier: 0..1 (0.2), interact_vibrations: bool(true)}` |
| `use_remainder` | item stack |
| `villager_food` (26.3) | `{nutrition: int>0}` |
| `writable_book_content` | `{pages:[string \| {raw, filtered?}]}` |
| `written_book_content` | `{pages:[TC \| {raw:TC, filtered?:TC}], title: string \| {raw,filtered?}, author, generation: 0..3, resolved: bool}` |
| variants (all string IDs/enums) | `axolotl/variant (lucy wild gold cyan blue)`, `cat/collar` (dye), `cat/sound_variant`, `cat/variant`, `chicken/sound_variant`, `chicken/variant`, `cow/sound_variant`, `cow/variant`, `fox/variant (red snow)`, `frog/variant`, `horse/variant`, `llama/variant`, `mooshroom/variant (red brown)`, `painting/variant`, `parrot/variant`, `pig/sound_variant`, `pig/variant`, `rabbit/variant`, `salmon/size`, `sheep/color`, `shulker/color`, `tropical_fish/{base_color,pattern,pattern_color}`, `villager/variant`, `wolf/collar`, `wolf/sound_variant`, `wolf/variant`, `zombie_nautilus/variant` |
| non-encoded | `creative_slot_lock`, `map_post_processing` (internal) |

Removed in 26.3: `swing_animation`, `map_color`. Added 26.1: `dye`, `additional_trade_cost`. Added 26.2: `sulfur_cube_content`.

### 4.3 Item modifier / loot function types (registry `loot_function_type` 26.3)
`apply_bonus, copy_components, copy_custom_data, copy_name, copy_state, discard, enchant_randomly, enchant_with_levels, enchanted_count_increase, exploration_map, explosion_decay, fill_player_head, filtered, furnace_smelt, limit_count, modify_contents, sequence, set_attributes, set_banner_pattern, set_book_cover, set_components, set_contents, set_count, set_custom_data, set_custom_model_data, set_damage, set_enchantments, set_firework_explosion, set_fireworks, set_instrument, set_item, set_loot_table, set_lore, set_name, set_ominous_bottle_amplifier, set_potion, set_random_dyes, set_random_potion, set_stew_effect, set_writable_book_pages, set_written_book_pages, toggle_tooltips`
(there is **no `set_nbt`** (removed in 1.20.5; use `set_custom_data`/`set_components`) and **no `reference`** anymore.)

Common fields on every function (26.3): `"type"`, optional `"condition"` (one predicate or predicate id; **was `conditions` list**). Item modifier file root = one function object, or a list (= sequence). `data/<ns>/item_modifier/<name>.json`. Wherever an item modifier is accepted you may give an id, `#tag`, inline, or a list mixing them (26.3).
Command usage: `item modify entity @s weapon.mainhand ns:mod` or inline SNBT `{type:"minecraft:set_count",count:5,add:true}` (examples in Argument_types still say `function:` - stale). `item replace/fill/override` in 26.3 take **slot sources** instead of single slots (string shorthand like `hotbar.4` / `container.*` still accepted).

Key functions & fields (26.3):
- `set_components`: `{components: {"minecraft:custom_name": {...}, "!minecraft:damage": {}}}` - set or remove (`!`).
- `set_custom_data`: `{tag: <SNBT string or compound>}` - merges into `minecraft:custom_data`.
- `copy_custom_data`: `{source: "this"|"block_entity"|... | {type:"minecraft:context",target:...} | {type:"minecraft:storage",source:"ns:id"}, ops:[{source:"<path>",target:"<path>",op:"replace|append|merge"}]}`.
- `set_count`: `{count: <int provider>, add: bool}`; `set_damage`: `{damage: <float provider>, add}` (1.0 undamaged); `set_item`: `{item}`; `limit_count`: `{limit:{min?,max?}}` (single number no longer accepted); `set_enchantments`: `{enchantments:{id: <int provider>}, add}`; `enchant_with_levels`: `{levels: <int provider>, options?: ID/[ids]/#tag, include_additional_cost_component?}`; `enchant_randomly`: `{options?, only_compatible?(true), include_additional_cost_component?}`; `set_potion`: `{id}`; `set_random_potion`: `{options?}`; `set_stew_effect`: `{effects:[{type: effect id, duration: <int provider>}]}`.
- `set_name`: `{name: TC, entity?: "this"|"attacker"|..., target?: "custom_name"|"item_name"}` (components needing resolution only resolved if `entity` set and present); `set_lore`: `{lore:[TC], mode: append|insert|replace_all|replace_section, offset?, size?, entity?}`; `copy_name`: `{source}`; `set_attributes`: `{modifiers:[{id, attribute, operation, amount: <float provider>, slot: ...}], replace?: bool(true)}`.
- `filtered`: `{item_filter: <item predicate>, on_pass?: modifier(s), on_fail?: modifier(s)}`; `discard`; `sequence`: `{functions: modifier(s), condition?}` (inline only in top-level file); `modify_contents`: `{component: container|bundle_contents|charged_projectiles, modifier: <modifier or id>}`; `set_contents`: `{component, entries:[loot entries]}`; `set_loot_table`: `{loot_table_id, seed?}`; `exploration_map`: `{destination (required): structure ID/[ids]/#tag, decoration, zoom, search_radius, skip_existing_chunks}`; `copy_state`: `{block, properties:[...]}`; `fill_player_head`: `{entity}`; `toggle_tooltips`, `set_book_cover`, `set_written_book_pages`, `set_writable_book_pages`, `set_banner_pattern`, `set_fireworks`, `set_firework_explosion`, `set_custom_model_data` (`floats` may contain providers; `colors` int providers), `set_ominous_bottle_amplifier`, `set_random_dyes`, `set_instrument`, `apply_bonus`, `enchanted_count_increase` (`count` float provider, `limit`, `enchantment`), `explosion_decay`, `furnace_smelt`.
Vanilla 26.3 example shapes: `{"type":"minecraft:set_name","name":{"translate":"filled_map.bamboo_camp_map"},"target":"item_name"}`, `{"type":"minecraft:set_components","components":{"minecraft:trim":{"material":"minecraft:copper","pattern":"minecraft:bolt"}}}`, `{"type":"minecraft:filtered","item_filter":{"predicates":{"minecraft:map_id":{}}},"on_fail":{"type":"minecraft:discard"}}`.

---------------------------------------------------------------------

## 5. Advancements

Sources: https://minecraft.wiki/w/Advancement_definition (**partly 26.3-stale**), 26.3 changelog "Advancement definition", registry `trigger_type`, vanilla advancements (mcmeta data 26.3).

File: `data/<ns>/advancement/<path>.json`.

```json
{
  "parent": "ns:root",                       // absent => root
  "display": { "icon": {"id":"minecraft:stone","count":1,"components":{}},
               "title": TC, "description": TC,
               "frame": "task|goal|challenge", "background": "ns:gui/...|path",
               "show_toast": true, "announce_to_chat": true, "hidden": false },
  "criteria": { "<name>": { "trigger": "minecraft:inventory_changed", "conditions": { "player": <predicate>, ... } } },
  "requirements": [ ["a","b"], ["c"] ],     // AND of ORs; default: all criteria required (each in its own group)
  "rewards": { "experience": 0, "recipes": ["ns:r"], "loot": <loot table(s)>, "function": "ns:fn" },
  "sends_telemetry_event": false
}
```
- No `display` => hidden helper advancement (ideal for hooks). If `display` present: needs `icon`, `title`, `description`. 26.3: visible roots (display but no parent) **must** have `display.background`; non-roots **must not**.
- `rewards.function`: a single function id (**function tags not allowed**), runs once when granted, `@s` = the player, at the player's position. `rewards.loot` (26.3): inline loot table, one id, list, or `#tag`. `rewards.recipes`, `rewards.experience`.
- Each criterion's `conditions.player` (any trigger except `impossible`): **26.3**: one inline predicate (needs `type`) or predicate id (no list shorthand).
- Criteria can be granted/revoked by command: `advancement grant|revoke <targets> only ns:name [criterion]`, `... everything|from|through|until`.

### Hook patterns (event -> function)
```json
// data/ns/advancement/hook/on_hurt.json  (no display; comments are illustrative, JSON has none)
{ "criteria": { "t": { "trigger": "minecraft:player_hurt_entity",
    "conditions": { "player": { "type":"minecraft:entity_properties","entity":"this",
                                "predicate": { "minecraft:equipment": { "mainhand": { "items": "minecraft:stick" } } } } } } },
  "rewards": { "function": "ns:on_hurt" } }
```
and in `ns:on_hurt`: `advancement revoke @s only ns:hook/on_hurt` as the **first** line so it can fire again (an advancement with all criteria met stays granted until revoked). `minecraft:tick` fires every tick for each player (per-player; advancement is granted once, so revoke it in the reward function to loop - but prefer `#minecraft:tick` function tag for pure ticking). `minecraft:location` fires every 20 ticks. Reward functions run with the **function permission level** and the player as `@s`; revocation inside the function re-arms it for the next event. **UNVERIFIED**: exact permission level (commonly stated as level 2; see 01-datapack-structure.md).

### Trigger list (registry `trigger_type`, 26.3; all accept `conditions.player`)
`allay_drop_item_on_block, any_block_use, avoid_vibration, bee_nest_destroyed, bred_animals, brewed_potion, changed_dimension, channeled_lightning, construct_beacon, consume_item, crafter_recipe_crafted, cured_zombie_villager, default_block_use, effects_changed, enchanted_item, enter_block, entity_hurt_player, entity_killed_player, fall_after_explosion, fall_from_height, filled_bucket, fishing_rod_hooked, hero_of_the_village, impossible, inventory_changed, item_durability_changed, item_used_on_block, kill_mob_near_sculk_catalyst, killed_by_arrow, levitation, lightning_strike, location, nether_travel, placed_block, player_generates_container_loot, player_hurt_entity, player_interacted_with_entity, player_killed_entity, player_sheared_equipment, recipe_crafted, recipe_unlocked, ride_entity_in_lava, shot_crossbow, slept_in_bed, slide_down_block, spear_mobs, started_riding, summoned_entity, tame_animal, target_hit, thrown_item_picked_up_by_entity, thrown_item_picked_up_by_player, tick, used_ender_eye, used_totem, using_item, villager_trade, voluntary_exile`

Conditions (besides `player`) for the hook-relevant ones; `entity`-type fields are an inline predicate `{type:"minecraft:entity_properties",entity:"this",predicate:{...}}` or predicate id (26.3):
| trigger | condition fields |
|---|---|
| `tick` | none (every tick per player) |
| `location` | none (every 20 ticks; use `player` predicate with `minecraft:location`) |
| `impossible` | none; grant manually with `advancement grant ... only id criterion` |
| `inventory_changed` | `items`: [item predicate] (each must match an *added* item), `slots`: `{empty,full,occupied: N|{min,max}}` |
| `using_item` | `item`: item predicate (every tick while using) |
| `consume_item` | `item` |
| `item_used_on_block` / `placed_block` / `default_block_use` / `any_block_use` / `allay_drop_item_on_block` | `location`: **predicate** (origin = block position; block state & tool available -> use `location_check`/`match_block`/`match_tool`) |
| `player_hurt_entity` | `damage` (damage predicate), `entity` (predicate) |
| `entity_hurt_player` | `damage` |
| `player_killed_entity` / `kill_mob_near_sculk_catalyst` | `entity`, `killing_blow` (damage source predicate) |
| `entity_killed_player` | `entity`, `killing_blow` |
| `player_interacted_with_entity` | `item`, `entity` |
| `item_durability_changed` | `item`, `durability`, `delta` |
| `recipe_crafted` / `crafter_recipe_crafted` | `recipes` (id/list/#tag; 26.3 rename), `ingredients`: [item predicates] |
| `recipe_unlocked` | `recipes` |
| `enter_block` / `slide_down_block` / `bee_nest_destroyed` | `blocks` (26.3 rename), `state` (+ bee nest: `item`, `num_bees_inside`) |
| `player_generates_container_loot` | `loot_tables` (26.3; id/list/#tag) |
| `changed_dimension` | `from`, `to` |
| `effects_changed` | `effects` map, `source` |
| `summoned_entity`, `tame_animal`, `villager_trade`(`item`,`villager`), `target_hit`(`signal_strength`,`projectile`), `thrown_item_picked_up_by_player/entity`, `fishing_rod_hooked`(`entity`,`item`,`rod`), `player_sheared_equipment`, `bred_animals`(`child`,`parent`,`partner`), `lightning_strike`, `cured_zombie_villager`, `channeled_lightning`(`victims`) | entity predicates / ids |
| `filled_bucket`, `shot_crossbow`, `used_totem`, `enchanted_item`(`item`,`levels`), `brewed_potion`(`potion`: potion_contents predicate in 26.3), `killed_by_arrow`(`fired_from_weapon`,`unique_entity_types`,`victims`), `spear_mobs`(`count`), `fall_from_height`/`nether_travel`/`ride_entity_in_lava`(`start_position`,`distance`), `fall_after_explosion`(`start_position`,`distance`,`source`), `levitation`(`distance`,`duration`), `construct_beacon`(`level`), `used_ender_eye`(`distance`), `avoid_vibration`, `hero_of_the_village`, `slept_in_bed`, `started_riding`, `voluntary_exile` | see wiki |
**UNVERIFIED**: full 26.3 field lists for the rarer triggers (wiki not fully updated); the table above follows the wiki + changelog.

Vanilla 26.3 example (shape check): `{"criteria":{"crafting_table":{"conditions":{"items":[{"items":"minecraft:crafting_table"}]},"trigger":"minecraft:inventory_changed"}}, "requirements":[["crafting_table"]], ...}`.

---------------------------------------------------------------------

## 6. Loot tables and recipes

### 6.1 Loot tables
Sources: https://minecraft.wiki/w/Loot_table, 26.3 changelog, vanilla 26.3 loot tables.
File `data/<ns>/loot_table/<path>.json`:
```json
{ "type": "minecraft:entity",            // context key set (optional, default generic)
  "random_sequence": "minecraft:entities/zombie",
  "modifier": <item modifier(s)>,         // applies to all drops (optional)
  "pools": [ {
      "rolls": 1,                         // int | context_int_provider (id or inline)
      "bonus_rolls": 0.0,                 // float | context_float_provider; per Luck level
      "condition": <predicate>,           // pool gate (single predicate/id; list shorthand removed)
      "modifier": <item modifier(s)>,
      "entries": [ <entry> ... ] } ] }
```
- `type` (context key set): `advancement_entity, advancement_location, advancement_reward, archaeology, barter, block, block_interact, block_use, chest, command, command_compute_default, command_compute_entity, command_compute_position, command_slot_source, container_process, empty, enchanted_damage, enchanted_entity, enchanted_item, enchanted_location, entity, entity_interact, equipment, fishing, generic, gift, hit_block, selector, shearing, vault, villager_trade`. Loot context entity targets for `entity` fields: `this, attacker, direct_attacker, attacking_player, target_entity, interacting_entity` (+ `block_entity`, `tool` for some functions).
- **Entry** common: `type`, `condition` (predicate), `modifier` (functions), and for singleton entries `weight` (default 1), `quality` (default 0; weight = max(floor(weight + quality*luck),0)).
  - singletons: `item` {`name`}, `loot_table` {`value`: id / list / inline / #tag (26.3), `expand` (26.3)}, `tag` {`items`: `#tag`/id/list (26.3; was `name`), `expand` bool}, `dynamic` {`name`: `minecraft:contents|minecraft:sherds`}, `empty`, `slots` {`slot_source`: slot source(s)}.
  - composites: `group`, `alternatives` (first passing child), `sequence` (children until one fails) - all with `children`.
  - registry `loot_pool_entry_type`: `alternatives, dynamic, empty, group, item, loot_table, sequence, slots, tag`.
- Vanilla 26.3 verified examples: `{"type":"minecraft:item","condition":"minecraft:tool/can_silk_touch","name":"minecraft:stone"}`, `{"type":"minecraft:tag","expand":true,"items":"#minecraft:creeper_drop_music_discs"}`, `"rolls":{"type":"minecraft:uniform","min":5,"max":8}`, `"modifier":{"type":"minecraft:set_count","count":{"type":"minecraft:uniform","min":1,"max":4}}`.
- `/loot` command can take an inline SNBT loot table. Slot sources (new files `slot_source/`): types `contents, empty, filtered, group, limit_slots, slot_range (source optional now: container|this|...; slots: "container.*")`.

### 6.2 Recipes
Sources: https://minecraft.wiki/w/Recipe_(Java_Edition) (header says "needs update"), 26.3 changelog, vanilla 26.3 recipes, registry `recipe_serializer`.
File `data/<ns>/recipe/<path>.json`. `result`: `{id, count?, components?}` (smelting-family results have no count). Ingredient = item id string, `#tag`, or list of ids. Common optional: `group`, `category`, `show_notification` (default true).

Serializers (26.3): `crafting_shaped, crafting_shapeless, crafting_transmute, crafting_imbue, crafting_dye, crafting_decorated_pot, crafting_special_{bannerduplicate,bookcloning,firework_rocket,firework_star,firework_star_fade,mapextending,repairitem,shielddecoration}, smelting, blasting, smoking, campfire_cooking, stonecutting, smithing_transform, smithing_trim, brewing`.

| type | fields |
|---|---|
| `crafting_shaped` | `pattern`: ["###","# #"] (<=3x3, equal widths, space = empty), `key`: {"#": ingredient}, `result`, `category` (equipment/building/misc/redstone) |
| `crafting_shapeless` | `ingredients` (1-9; each ingredient or list), `result` |
| `crafting_transmute` | `input`, `material`, `material_count?` (int or {min,max}), `add_material_count_to_result?`, `result` (keeps input's components, can override) |
| `crafting_imbue` | `source` (center), `material` (the 8 around), `result` (keeps source `potion_contents`) - tipped arrows |
| `crafting_dye` | `dye` (ingredient with `minecraft:dye` component), `target`, `result` |
| `smelting` / `blasting` / `smoking` / `campfire_cooking` | `ingredient`, `result`, `cookingtime` (**required in 26.3**; vanilla 26.3 values verified: smelting 200, blasting 200, smoking 200, campfire_cooking 600), `experience` (optional float), `category` (food/blocks/misc), `group` |
| `stonecutting` | `ingredient`, `result` |
| `smithing_transform` | `template`, `base`, `addition` (all optional-ish ingredients), `result` |
| `smithing_trim` | `template`, `base`, `addition` |
| `brewing` (NEW 26.3, data-driven) | `input`: `{item, potion_contents?: <potion_contents component predicate>}`, `reagent`: same shape, `output`: item stack `{id, components?}` - e.g. `{"type":"minecraft:brewing","input":{"item":"minecraft:potion","potion_contents":{"potions":"minecraft:water"}},"reagent":{"item":"minecraft:nether_wart"},"output":{"id":"minecraft:potion","components":{"minecraft:potion_contents":{"potion":"minecraft:awkward"}}}}` (the vanilla file uses `potions` in the predicate; the changelog example used `potion` - use `potions`) |
Recipe unlock: `recipe give|take`, advancement `rewards.recipes`, state stored in player NBT (`recipeBook`). Advancement trigger fields `recipes` accept id/list/`#tag` in 26.3.
`crafting_shaped` in 26.3 vanilla: `{"type":"minecraft:crafting_shaped","key":{"#":"#minecraft:planks"},"pattern":["#","#"],"result":{"count":4,"id":"minecraft:stick"}}`.

---------------------------------------------------------------------

## 7. Dialogs (1.21.6+)

Source: https://minecraft.wiki/w/Dialog ; vanilla 26.3 `data/minecraft/dialog/*.json` (`custom_options`, `quick_actions`, `server_links`); registries `dialog_type`, `dialog_action_type`, `dialog_body_type`, `input_control_type`.
File: `data/<ns>/dialog/<name>.json`; shown with `/dialog show <players> <id|inline SNBT>`, via `show_dialog` click_event, or from dialog tags `#minecraft:pause_screen_additions` / `#minecraft:quick_actions`. Dialog text components do **not** resolve `nbt`/`score`/`selector`.

Common fields: `type`, `title` (TC, required), `external_title` (TC; button label when listed), `body` (element or list), `inputs` (list), `can_close_with_escape` (default true), `pause` (default true; single-player), `after_action`: `close` (default) | `none` (only if `pause:false`) | `wait_for_response`.
Types:
- `minecraft:notice` - `action` button (default OK/no action).
- `minecraft:confirmation` - `yes`, `no` buttons (required).
- `minecraft:multi_action` - `actions` (non-empty list of buttons), `columns` (2), `exit_action`.
- `minecraft:dialog_list` - `dialogs` (id/list/inline/#tag), `columns`, `button_width`, `exit_action`.
- `minecraft:server_links` - `columns`, `button_width`, `exit_action`.
Button: `{label: TC, tooltip?: TC, width?: 1..1024 (150), action?: <action>}`.
Body: `minecraft:plain_message` {`contents` TC, `width` (200)}; `minecraft:item` {`item`: stack, `description`: TC or `{contents,width}`, `show_decoration`, `show_tooltip`, `width`,`height`}.
Inputs (all have `key` [letters, digits, `_`] and `label`): `minecraft:text` {`width`,`label_visible`,`initial`,`max_length`(32),`multiline`:{`max_lines`,`height`}}, `minecraft:boolean` {`initial`,`on_true`("true"),`on_false`("false")}, `minecraft:single_option` {`options`:[{`id`,`display?`,`initial?`}], `width`, `label_visible`}, `minecraft:number_range` {`start`,`end`,`step?`,`initial?`,`label_format?`,`width`}.
Actions (`action` object, `type`): static - same as click events except open_file: `open_url{url}`, `run_command{command}`, `suggest_command{command}`, `change_page{page}`, `copy_to_clipboard{value}`, `show_dialog{dialog}`, `custom{id,payload?}`; dynamic - `dynamic/run_command{template}` (macro `$(key)` substitution of input values; text input values are escaped for SNBT strings) and `dynamic/custom{id, additions?}` (sends all input values as a compound payload `{key: value}`; text -> string, boolean -> `1b/0b`, number_range -> float, option -> its `id`).
Datapack integration: `run_command` runs as the clicking player with **their** permission level (privileged commands only for ops) -> typical pattern is `/trigger <objective> set <n>` (triggers are player-callable) or `dynamic/run_command` with `trigger ... set $(n)`; `custom`/`dynamic/custom` events are **ignored by vanilla servers** (only mods/plugins can receive them). **UNVERIFIED**: whether `/function` from a dialog button works for non-ops (assumed no).

---------------------------------------------------------------------

## 8. Entity and block-entity NBT commonly touched

Sources: https://minecraft.wiki/w/Entity_format (+ /Entity, /Mob), /w/Display (+ Entity_format/display_entity), /w/Armor_Stand, /w/Marker, /w/Interaction, /w/Lingering_Potion (entity data), /w/Item_(entity), /w/Player.dat_format, /w/Commands/data.

### Tags common to all entities
`Pos:[x,y,z]d`, `Motion:[dx,dy,dz]d` (clamped +/-10), `Rotation:[yaw,pitch]f` (yaw 0 = south, increases clockwise; pitch +90 = down), `OnGround`, `fall_distance:d`, `Fire:s`, `Air:s`, `PortalCooldown`, `NoGravity:b`, `Invulnerable:b`, **`invulnerable_time:int` (NEW 26.3: >0 => invulnerable for that many ticks, counts down; independent of `Invulnerable`)**, `Silent:b`, `Glowing:b`, `HasVisualFire:b`, `TicksFrozen`, `CustomName` (TC), `CustomNameVisible:b`, `Tags:[str]` (scoreboard tags; lost if removed), `Passengers:[entities]`, `UUID:[I;4]`, `id` (only in stored/embedded entities), **`data:{}`** (custom data compound for any entity = `minecraft:custom_data`, removed when empty).
Mobs add: `Health:f`, `AbsorptionAmount`, `active_effects:[{id,amplifier(b),duration,ambient,show_particles,show_icon}]`, `attributes:[{id,base,modifiers:[]}]`, `equipment:{head,chest,legs,feet,mainhand,offhand,body,saddle: item stack}`, `drop_chances`, `NoAI:b`, `PersistenceRequired`, `CanPickUpLoot`, `LeftHanded`, `DeathLootTable`/`DeathLootTableSeed`, `Brain`, `leash`.
Block state objects inside entity NBT (falling_block `BlockState`, block_display `block_state`, enderman `carriedBlockState`): `{id:"minecraft:oak_stairs",properties:{facing:"north"}}` or `"minecraft:stone"` (26.3).

### marker
`summon marker ~ ~ ~ {Tags:["x"],data:{any:"thing"}}` - server-only, no tick cost; ideal as a data anchor. `data` holds custom NBT. Cannot carry passengers.

### armor_stand
`Invisible:b`, `Marker:b` (tiny hitbox, no interaction), `NoBasePlate:b`, `ShowArms:b`, `Small:b`, `DisabledSlots:int` (bit field), `Pose:{Head,Body,LeftArm,RightArm,LeftLeg,RightLeg:[x,y,z]f}`, `equipment:{...}`.

### display entities (`block_display`, `item_display`, `text_display`)
Common (Entity_format/display_entity): `transformation` = `{translation:[x,y,z]f, left_rotation:[x,y,z,w]f (quaternion, real last) | {angle,axis:[..]}, scale:[x,y,z]f, right_rotation:[..]}` **or** 16-float row-major matrix list (matrix form accepted on input, decomposed form saved); `interpolation_duration:int` (ticks), `start_interpolation:int` (delay; not saved; set by commands), `teleport_duration:int` (0..59), `billboard:"fixed|vertical|horizontal|center"`, `brightness:{block:0..15,sky:0..15}`, `glow_color_override:int`, `view_range:f` (1.0), `shadow_radius:f`, `shadow_strength:f`, `width:f`, `height:f` (culling box; 0 = none). Interpolated: transformation, shadow_*, text `background`, `text_opacity`. Any change restarts interpolation from current state; changes within one tick count as one.
- `block_display`: `block_state` (block state object).
- `item_display`: `item` (item stack, no Slot), `item_display:"none|thirdperson_lefthand|thirdperson_righthand|firstperson_lefthand|firstperson_righthand|head|gui|ground|fixed"` (default none). Also addressable as slot `contents` by `/item`.
- `text_display`: `text` (TC; resolved with the entity as context), `alignment:"center|left|right"`, `background:int ARGB` (default 0x40000000), `default_background:b`, `line_width:int` (200), `see_through:b`, `shadow:b`, `text_opacity:byte` (-1 = 255; 4..26 discarded), px = 1/40 m.
Displays are invisible for one tick after load; summon-and-set-in-one-command is safe.

### interaction
`width:f`, `height:f`, `response:b`, `attack:{player:[I;4],timestamp:long}`, `interaction:{player:[I;4],timestamp:long}` (last left/right click; compare `timestamp` with gametime; clear with `data remove entity @s attack`/`interaction`). Pair with advancements `player_interacted_with_entity` / `player_hurt_entity` as an alternative.

### area_effect_cloud
`Age, Duration, WaitTime, ReapplicationDelay, DurationOnUse:int`, `Radius, RadiusOnUse, RadiusPerTick:f`, `Color:int`, `custom_particle:{type:"minecraft:...",...}`, `potion_contents:{potion?,custom_effects:[...],custom_color?}` (or string = potion id), `potion_duration_scale:f` (lingering potions use 0.25), `Owner:[I;4]` (not preserved). Effect duration is divided by the scale/applied per `ReapplicationDelay`.

### item (entity)
`Item:{id,count,components}`, `Age:s` (-32768 = never despawn), `PickupDelay:s` (32767 = never pickable), `Health:s`, `Owner`/`Thrower` UUID.

### Player / `data` command caveats
- `data get entity <player> ...` works (read-only); **`data modify`/`data merge` on players fails** ("Unable to modify player data"); `data remove` on a player is not allowed. Use `item`, `attribute`, `effect`, scoreboard, `tag`, `team`, etc. to mutate players.
- Since 25w03a (1.21.5) player armor/offhand are in `equipment` (not `Inventory` slots 100-103/-106); selected item data via `SelectedItem`, hotbar slot via `SelectedItemSlot`. Ender chest: `EnderItems`.
- Player NBT in `<uuid>.dat`; `data get entity @s` returns the live player compound (large); prefer `execute store result score ... run data get entity @s Path 1` with narrow paths; Common keys: `Pos`, `Rotation`, `Health`, `foodLevel`, `XpLevel`, `Dimension`, `playerGameType`, `Inventory`, `equipment`, `SelectedItem`, `SelectedItemSlot`, `EnderItems`; the spawn-point key layout in 26.3 was **not checked** (**UNVERIFIED**).
- `Tags`, `Pos` etc. are readable on any entity; most mob/NBT writes need `data merge entity @e[...]` (not players). Changing `Pos` via merge on non-player entities works but `tp` is preferred; `Motion` on non-players works.

### Block entities (touched often)
- Signs/hanging signs: `front_text`/`back_text`: `{messages:[TC x4], filtered_messages?, color: dye, has_glowing_text: bool}`, `is_waxed: bool`, **26.3 `allow_op_features: bool` (default false; needed for clickable commands)**. Block `decorated_pot`: `sherds:{back,left,right,front: item stack}` (26.3). Brewing stand / furnace numeric fields are now ints (+ `speed_multiplier`).
- Container block entities: `Items:[{Slot:b,id,count,components}]`, `CustomName`, `lock` (item predicate), `LootTable`/`LootTableSeed`.

---------------------------------------------------------------------

## 9. Open uncertainties / things to re-verify before relying on

1. `time_check.clock` mandatory? (**UNVERIFIED**; emit always.)
2. Exact permission level of advancement-reward functions (**UNVERIFIED**).
3. `firework_explosion` data component predicate field list; `flags` entity sub-predicate extra values added 1.21.11; full 26.3 condition field lists of rare advancement triggers (wiki pages unchanged since 26.2).
4. (resolved) vanilla cooking times verified from 26.3 data: smelting/blasting/smoking 200, campfire 600.
5. Whether dialog `run_command` can invoke `/function` for non-op players (assumed not).
6. Wiki 26.3 changelog prose has mid-snapshot leftovers (`conditions` in conditional providers, `number_provider` in dispatcher cases, `targeting_entity`); the vanilla data/registry dump was used to resolve them. Anything not covered by vanilla data (e.g. `from_float`, `floor_mod`, `length` providers, `copy_properties_provider`) is taken from the changelog only.
7. 26.4-only items to ignore: `below_heightmap` predicate, `equip_sound`/`consumable.sound`/`shearing_sound` fields, `matching_fluids.state`, `TicksFrozen` via freezing effect.
