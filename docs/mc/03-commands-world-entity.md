# 03 - World / entity / item / player commands (Minecraft Java 26.3, data pack format 121.0)

Scope: advancement, attribute, ban/pardon/op/deop/kick/whitelist (+ ban-ip, pardon-ip, banlist, setidletimeout, stop/save-*, list, help), clear, clone, damage, defaultgamemode, effect, enchant, experience/xp, fill, fillbiome, gamemode, give, item, kill, locate, loot, particle, place, playsound, publish, recipe, ride, rotate, setblock, spawnpoint, spectate, spreadplayers, stopsound, summon, swing, teleport/tp, transfer, weather, worldborder, waypoint, stopwatch, fetchprofile.
Not covered here (other docs): execute, scoreboard, data, function, return, schedule, tag, trigger, tellraw/title/say/me/msg, bossbar, team, random, tick, forceload, time, gamerule, difficulty, setworldspawn, seed, datapack, reload, debug, compute, posteffect, dialog, version, test.

## 0. Source quality and method (READ FIRST)

The syntax trees in this file were NOT written from memory or from the (often stale) wiki. They were generated mechanically from the **Brigadier command tree dump** that the misode/mcmeta project extracts from the game's `--reports` output (`commands.json`), pinned to the exact release commit:

- 26.3 release tree: `https://raw.githubusercontent.com/misode/mcmeta/d96c75fec2/commands/data.json` (commit "Update summary for 26.3", 2026-09-15). Registries: `.../d96c75fec2/registries/data.json`.
- Compared against tree dumps for 1.21.4 (`1b221f1ccc`), every release/snapshot from 1.21.5 onward (all via `https://raw.githubusercontent.com/misode/mcmeta/<sha>/commands/data.json`; commit list from `https://api.github.com/repos/misode/mcmeta/commits?sha=summary`).
- The `summary` branch HEAD at research time is 26.4-snapshot-5 (2026-10-20); the trees for every command in this file were diffed between 26.3 and 26.4-snapshot-5 and are **identical** (no 26.4 snapshot changed any of these command trees; only `/fillbiome` has a documented upcoming behavior change, see its section).

Legend used in the trees below:
- `[X]` = node is executable (the command may end here).
- `<name>` = argument node; `: parser{props}` is the Brigadier parser id (props = parser properties from the dump). `mc:` = `minecraft:`, `b:` = `brigadier:`.
- Permission level shown as `L=` using the 26.x names: `all` (no restriction, level 0), `moderators` (1, unused by any command here), `gamemasters` (2), `admins` (3), `owners` (4). In the 26.3 dump the command node carries `permissions: {type: "minecraft:require", permission: {type: "minecraft:command_level", level: "<name>"}}`. (Wiki permission page `https://minecraft.wiki/w/Permission_level` is stale and does not document the names; the name-to-number mapping is inferred from the old 2/3/4 levels each command had in 1.21.4 - UNVERIFIED but consistent: every command that was level 2 is `gamemasters`, level 3 `admins`, level 4 `owners`.)
- Result/success semantics ("store result" = what `execute store result` captures, "success" = what `execute store success` captures / whether the command counts as passed) come from the per-command wiki pages (`https://minecraft.wiki/w/Commands/<name>`), which are mostly accurate for output values but have **no history entries after 1.21.x for many commands**; the tree dumps are authoritative for syntax.

Common command-result rule: a command is **unparseable** (syntax error, nothing runs), **failed** (runs, result 0, success 0) or **successful**. Some failures are **errors** (a thrown CommandSyntaxException shown in red); per the wiki's `/place` table, an *error* leaves the `execute store` target unchanged while a *fail* writes 0 - UNVERIFIED for commands other than /place.

---

## 1. Changes since 1.21.4 (compiler-relevant)

Derived from tree diffs 1.21.4 -> 26.3 (authoritative for syntax) plus wiki version pages.

### Syntax / tree changes (by version where the change first appears in the dump)
| Version | Command | Change |
|---|---|---|
| 1.21.5 (25w02a) | `fill`, `setblock`, `clone`, `place template` | New `strict` flag: place blocks as-is, no block updates and no shape updates. `fill ... replace <filter>` is no longer terminal: `replace <filter>` can now be followed by `destroy|hollow|outline|strict`. `clone` takes `strict` BEFORE the mask mode: `clone ... [strict] [replace\|masked\|filtered <f>] [force\|move\|normal]`. Omitting `{nbt}` on the block now PRESERVES the existing block entity data (previous behavior wiped it). Source: `https://minecraft.wiki/w/Java_Edition_1.21.5` |
| 1.21.5 | NBT/SNBT | Text components in NBT/item components are now SNBT/NBT-native (not JSON strings), item components replaced NBT (already 1.20.5). Affects `summon` `{CustomName:...}`, `give` `custom_name`/`lore`. (Cross-check with the data doc; wiki confirms only the 1.20.5 component change on `/give`.) |
| 1.21.5 | slot names | `horse.saddle` renamed to `saddle` (works on any entity that can equip a saddle). `https://minecraft.wiki/w/Java_Edition_1.21.5`. `horse.armor` is not in the current slot list (use `armor.body`) - the removal is UNVERIFIED as to exact version. |
| 1.21.6 | `waypoint` | NEW command (25w15a experimental, 25w17a normal gameplay, `fade` args removed, `style` args added). |
| 1.21.6 | `playsound`, `stopsound` | New sound source category `ui`. |
| 1.21.9 | `spawnpoint` | Last arg changed from `<angle>` (`mc:angle`, yaw only) to `<rotation>` (`mc:rotation`, yaw+pitch). Wiki says "optional pitch added" but the tree has ONE `<rotation>` node, which (Rotation argument) takes yaw AND pitch together - UNVERIFIED whether a lone yaw is still accepted; the tree says it is not. |
| 1.21.9 (25w31a) | `summon` | Now fails when summoning monsters (hostile mobs) while difficulty is Peaceful. |
| 1.21.9 (25w36a) | `worldborder` | Affects only the border of the dimension the command runs in. |
| 1.21.11 (25w41a) | `stopwatch` | NEW command (25w42a: query returns the value, `scale` arg added). |
| 1.21.11 (25w43a/44a) | `worldborder add|set|warning time` | `<time>` changed from integer SECONDS to `mc:time` (float + optional `d`/`s`/`t` suffix, DEFAULT UNIT = TICKS). `worldborder set 100 10` used to mean 10 s, now means 10 ticks. |
| 1.21.11 (25w44a) | gamerule | `commandModificationBlockLimit` renamed to `max_block_modifications` (limit for fill/clone/fillbiome volume). Default value UNVERIFIED (was 32768). |
| 1.21.11 | permissions | Command tree now carries permission requirement objects with named levels (`gamemasters`, `admins`, `owners`); see section 0. |
| 26.1 (snapshot 1) | `swing` | NEW command. snapshot 3: all args optional. |
| 26.1 | `fetchprofile` | NEW command: `fetchprofile entity|id|name ...`. |
| 26.1 (snapshot 4) | slots | `villager.*` and `piglin.*` slot names REMOVED; use `mob.inventory.*`. `https://minecraft.wiki/w/Java_Edition_26.1_Snapshot_4` |
| 26.1 | output/behavior | Game mode changes (F3+F4 or commands) change the world default game mode (LAN/host context); advancement grant/revoke reports how many advancements changed state across all players / number of players. (From `https://minecraft.wiki/w/Java_Edition_26.2` command bullets; page scope is partly ambiguous - UNVERIFIED which of 26.1/26.2.) |
| 26.2 | `waypoint modify ... color` | Color arg parser changed `mc:color` -> `mc:team_color`: accepts ONLY lowercase names with underscores (`dark_purple` yes; `darkpurple`, `DarkPurple` no). Same change for `/team modify <t> color`. |
| 26.2 | attributes | New attributes: `air_drag_modifier`, `below_name_distance`, `bounciness`, `friction_modifier`, `name_tag_distance`; min `knockback_resistance` is now -2.0. (`waypoint_receive_range`/`waypoint_transmit_range`, `camera_distance`, `tempt_range`, `movement_efficiency`, `step_height`... exist since earlier.) |
| 26.2 | `publish` | snapshot 8 briefly added a leading `<online>` bool; REVERTED in 26.2-pre-1. Not in release. |
| 26.2 | particles | New: `geyser`, `geyser_base`, `geyser_plume`, `geyser_poof`, `sulfur_cube_goo`, `noxious_gas`, `noxious_gas_cloud`, `sulfur_bubbles`, `pause_mob_growth`, `reset_mob_growth` etc. (see registry list in section 19). |
| 26.2 | `/execute on owner` | vexes count as owned by their evoker (execute doc). |
| 26.3 (snapshot 1) | `item` | MAJOR: slot arguments become `slot_source` (`mc:slot_source`) instead of `item_slot`; NEW subcommands `item fill` and `item override`; `from entity` source may now be a multi-entity selector. `item modify|replace` arg names change (`<pos>`->`<target>`). |
| 26.3 (snapshot 1) | `give` | Over-limit count now signals an ERROR instead of printing a failure message and returning 0. |
| 26.3 (snapshot 1-3) | `place feature` | Argument changed `resource_key<worldgen/configured_feature>` -> `resource_key<worldgen/feature>` (snap 1; the registry `configured_feature` was renamed `worldgen/feature`) -> `mc:feature` parser (snap 3): accepts an inline SNBT feature definition OR a resource location. Also affects `tag/worldgen/feature`. |
| 26.3 (snapshot 4) | `spreadplayers` | Safe landing blocks controlled by block tag `#minecraft:entities_can_teleport_to`. |
| 26.3 (snapshot 6) | `publish` | `<gamemode>` argument REMOVED: now `publish [<allowCommands>] [<port>]`. |
| 26.3 (snapshot 7) | `swing` | Added `<animation>` (`whack`/`stab`/`none`) and `<duration>` (`mc:time`, min 1 tick, default `6t`); using it on a player no longer resets their attack strength. |
| 26.3 (snapshot 10) | `locate` | Performance improvement for structures (no syntax change). |
| 26.3 | misc | `/team join|leave` counts only changed entities; success-message overcounting fixed for `/tag`, `/scoreboard players enable`, `/recipe`, `/enchant`, `/effect` (MC-308008..308013): the **result value** of these commands now counts only entities/players actually changed. `/give`, `/unpublish`, `/tick` signal errors properly. |
| 26.4 snapshot 1 (upcoming, not 26.3) | `fillbiome` | "Biome filling works with block accuracy" (per wiki history) - not in 26.3. |

### Unchanged since 1.21.4 (verified identical tree modulo permission metadata)
advancement, attribute (base reset arrived 1.21.4 / 24w44a), ban family, clear, damage, defaultgamemode, effect, enchant, experience/xp, fillbiome, gamemode, give (tree), kill, loot, particle, recipe, ride, rotate, spectate, summon, teleport/tp, transfer, weather (tree; the argument type was already `time`), whitelist, setidletimeout, kick, stopsound/playsound apart from `ui`.

Sources for this section: tree diffs described in section 0; `https://minecraft.wiki/w/Java_Edition_26.1`, `.../26.2`, `.../26.3`, `.../26.3-snapshot-1`, `.../1.21.5`; per-command history tables cited below.

---

## 2. Shared argument types and syntax (compiler cheat-sheet)

Source: `https://minecraft.wiki/w/Argument_types` + parser ids from the dump.

| Parser | Syntax |
|---|---|
| `mc:entity{type,amount}` | selector `@s @p @a @e @r @n`, player name, or UUID. `amount: single|multiple`, `type: entities|players`. "players" requires player-type selector (a name/UUID/`@a`/`@p`/`@r`/`@s`-as-player; `@e` is rejected at parse time unless `type=player`... UNVERIFIED nuance). |
| `mc:game_profile` | player name (even offline) or player selector. Used by ban/pardon/op/deop/whitelist. |
| `mc:block_pos` | three ints, each `N`, `~`, `~N`, or `^N` (local; all three must then be `^`). |
| `mc:vec3` | three doubles (`x y z`), `~` and `^` allowed. If written as integers w/o `~`, x/z are centered (+0.5). |
| `mc:vec2` | two doubles (x z), `~` allowed (no `^`). |
| `mc:rotation` | two doubles `yaw pitch`, `~` relative to execution rotation. Yaw: -180=north, 0=south, 90=west. Pitch -90 up .. 90 down. |
| `mc:angle` | (removed from spawnpoint in 1.21.9) single float yaw. |
| `mc:time` | float + optional unit: `t` (ticks, default) `s` (20 ticks) `d` (24000 ticks). Rounded to nearest tick. `{min}` property enforces minimum ticks. e.g. `10s`, `0.5d` = 12000. |
| `mc:block_state` | `block_id[prop=val,...]{nbt}` - `[]` and `{}` optional, no tags. |
| `mc:block_predicate` | same as block_state but `#tag` allowed (`#minecraft:logs[axis=y]{...}`); properties/NBT are partial matches. |
| `mc:item_stack` | `item_id[component=value,!component,...]` (SNBT values; `!id` removes a default component). Namespace optional. |
| `mc:item_predicate` | `<item_id\|#tag\|*>[tests]`; tests (comma = AND, `\|` = OR, `\|` binds tighter than `,`): `component` (exists), `component=snbt` (exact), `predicate~snbt` (sub-predicate, e.g. `damage~{durability:{min:3}}`), `count`, `count=N`, `count~{min:..,max:..}`, `!test` negation. |
| `mc:item_slot` | `<slot_type>` or `<slot_type>.<n>` (see slot table, section 5). |
| `mc:item_slots` | same plus `*` wildcards (registered; `/item` now uses `slot_source` instead). |
| `mc:slot_source` | SNBT slot-source definition, or resource location in the `slot_source` registry/datapack folder; plain slot names (incl. wildcards) are shorthand for `slot_range`. See section 5. |
| `mc:loot_modifier` | SNBT item-modifier (loot function or list of) or resource location of an `item_modifier` datapack file. |
| `mc:loot_table` | resource location OR inline SNBT loot table (since 1.20.5). |
| `mc:feature` | resource location of a worldgen feature OR inline SNBT definition (26.3+). |
| `mc:particle` | `particle_id` or `particle_id{snbt options}`. |
| `mc:nbt_compound_tag` | SNBT compound. |
| `mc:resource{registry}` | existing registered id (namespace optional); invalid ids fail at parse time. |
| `mc:resource_key{registry}` | id that is resolved at execution (does not need to exist at parse time; missing -> error/fail at run). |
| `mc:resource_or_tag{registry}` / `resource_or_tag_key` | id or `#tag`. |
| `mc:resource_location` | arbitrary id for unregistered content (modifier ids, stopwatch ids, sound ids, waypoint style, template ids). |
| `mc:dimension` | dimension id. |
| `mc:entity_anchor` | `eyes` or `feet`. |
| `mc:team_color` | lowercase underscore text color name (not hex). |
| `mc:hex_color` | 3 or 6 hex digits (`F0A`, `FF00AA`); 3-digit digits are x17. |
| `mc:swing_animation` | `whack`, `stab`, `none`. |
| `mc:template_rotation` | `none`, `clockwise_90`, `counterclockwise_90`, `180`. |
| `mc:template_mirror` | `none`, `front_back`, `left_right`. |
| `mc:gamemode` | `survival`, `creative`, `adventure`, `spectator`. |
| `mc:message` | greedy plain text; `@s`-style selectors inside are replaced with entity names. NOT a JSON/SNBT component. |
| `mc:uuid` | UUID. |
| `b:string{type}` | `word`, `phrase` (quoted ok), `greedy`. |

### Which of these commands accept text components
Only indirectly. None of the commands in this file take a `mc:component` argument. `kick`/`ban`/`ban-ip` take `mc:message` (plain text; selectors replaced, no formatting). Text components enter through: `give`/`item replace ... with`/`loot` item components (`custom_name`, `lore`, `item_name`, written_book_content...) and `summon`/`place`-fed NBT (`CustomName`, etc.) - in 1.21.5+ these are written as SNBT components (a bare string `"Hi"` or compound `{text:"Hi",color:"red"}` or list), not JSON-in-a-string. `item modify` functions `set_name`, `set_lore` take components in the loot-function JSON/SNBT. `waypoint` takes colors only.

### Block-state syntax notes
- `minecraft:stone_stairs[facing=east,half=top]{...}`: properties are validated against the block at parse time; unknown prop/value is a parse error. `{...}` only meaningful for block-entity blocks (chests, signs...) and merges into block entity data on placement. In 1.21.5+, if `{}` is omitted and the old block had block entity data of the same type, that data is preserved (setblock/fill replace of same BE type).
- Tags are only allowed in predicates (`replace <filter>`, `clone filtered`, `fill replace`), not as the block to place.
- `fill`/`setblock` fail when the new state is identical to the old one (see each section).

---

## 3. advancement  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/advancement`

```
advancement (grant|revoke) <targets: entity players multiple>
    everything                                   [X]
    only <advancement: resource_key advancement> [X] [<criterion: greedy string>] [X]
    from <advancement>                           [X]
    through <advancement>                        [X]
    until <advancement>                          [X]
```
- `from` = advancement + all descendants; `until` = advancement + all ancestors to root; `through` = ancestors + advancement + descendants; `only` = just that advancement or (with `<criterion>`) one criterion. `everything` = all loaded.
- `<advancement>` is a `resource_key`, so a non-existent id is a runtime failure, not a parse error. `<criterion>` is a greedy string (takes rest of line).
- Result: fails if targets don't resolve to online players or nothing changes. Success count 1. `execute store result` = total number of advancements (or criteria) granted/revoked across players (per page: "total number of advancements (or criterions) added to (or removed from) each player"). As of 26.1/26.2 the chat output reports how many advancements changed and for how many players (not changing store values; UNVERIFIED).
- History: 1.12 added; 1.13 removed `test`; 1.20.3 no longer throws unhandled exceptions. No syntax changes through 26.3.

## 4. attribute  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/attribute`, `https://minecraft.wiki/w/Attribute`

```
attribute <target: entity entities single> <attribute: resource attribute>
    get [X]            [<scale: double>] [X]
    base get [X]       [<scale: double>] [X]
    base set <value: double>       [X]
    base reset                     [X]                      (added 1.21.4, 24w44a)
    modifier add <id: resource_location> <value: double> (add_value|add_multiplied_base|add_multiplied_total) [X]
    modifier remove <id: resource_location> [X]
    modifier value get <id: resource_location> [X] [<scale: double>] [X]
```
- Modifier operation names (renamed 24w09a / 1.20.5): `add_value`, `add_multiplied_base`, `add_multiplied_total`. Modifier ids are RESOURCE LOCATIONS (1.21, 24w21b; replaced `uuid`+`name`); namespace defaults to `minecraft:`; re-using an id that already exists on that attribute FAILS (must `remove` first). Modifier ids are per-attribute-instance; the same id can exist on different attributes.
- Formula order: all `add_value` summed onto base; then `add_multiplied_base` sums multiply once `base_result*(1+sum)`; then each `add_multiplied_total` multiplies `(1+x)` successively; result clamped to attribute min/max.
- Target: players, armor stands, mobs (living entities with attribute maps). Non-living -> unparseable/fail. Attribute missing on that entity (e.g. `flying_speed` on a player) -> fail.
- Result: `get`/`base get`/`modifier value get`: store result = value * `<scale>` cast to int (default scale 1); success 1. `base set`, `modifier add`, `modifier remove`, `base reset`: success 1, result 1. Failure -> 0/0.
- Attribute ids in 26.3 (registry `minecraft:attribute`, 40): air_drag_modifier, armor, armor_toughness, attack_damage, attack_knockback, attack_speed, below_name_distance, block_break_speed, block_interaction_range, bounciness, burning_time, camera_distance, entity_interaction_range, explosion_knockback_resistance, fall_damage_multiplier, flying_speed, follow_range, friction_modifier, gravity, jump_strength, knockback_resistance, luck, max_absorption, max_health, mining_efficiency, movement_efficiency, movement_speed, name_tag_distance, oxygen_bonus, safe_fall_distance, scale, sneaking_speed, spawn_reinforcements, step_height, submerged_mining_speed, sweeping_damage_ratio, tempt_range, water_movement_efficiency, waypoint_receive_range, waypoint_transmit_range. (Names are the registry ids WITHOUT the old `generic.`/`player.`/`zombie.` prefixes, which were dropped in 1.21.2.) Defaults/min/max table: wiki `Attribute` page (e.g. max_health 20 [1,1024], scale 1 [0.0625,16], gravity 0.08 [-1,1], knockback_resistance 0 [-2,1], waypoint_transmit_range 0 [0,60000000]).

## 5. item  (L=gamemasters)  -- 26.3 rewrite
Sources: `https://minecraft.wiki/w/Commands/item` (flagged "needs update"), `https://minecraft.wiki/w/Java_Edition_26.3-snapshot-1`, `https://minecraft.wiki/w/Slot_source`, `https://minecraft.wiki/w/Slot`, tree dump.

```
item (replace|override|fill) (block <target: block_pos>|entity <target: entity entities multiple>) <slots: slot_source>
        with <item: item_stack> [<count: int 1..99>] [X]
        from (block <source: block_pos>|entity <source: entity entities multiple>) <sourceSlots: slot_source> [X] [<modifier: loot_modifier>] [X]
item modify (block <target: block_pos>|entity <target: entity entities multiple>) <slots: slot_source> <modifier: loot_modifier> [X]
```
(`replace`, `override`, `fill` all have the identical `with`/`from` shape in the dump; `modify` has only the modifier.)

Semantics (26.3 snapshot 1 notes, `.../26.3-snapshot-1`):
- `replace`: source items are matched to destination slots one by one in order; surplus destination slots untouched (`replace [12345] with (ABC) -> [ABC45]`), surplus source items ignored (`replace [123] with (ABCDE) -> [ABC]`). `with <item> <count>` supplies a single stack, so on a multi-slot destination only the first slot is written.
- `fill`: repeats the source items to fill every destination slot (`fill [12345] with (ABC) -> [ABCAB]`).
- `override`: writes matching source items and CLEARS remaining destination slots (`override [12345] with (ABC) -> [ABC__]`).
- `from entity` source may be multi-entity; slots from each entity are concatenated in selector order.
- `modify` applies the item modifier to each slot in the slot source.
- Old forms still valid: `item replace entity @s weapon.mainhand with diamond_sword`, `item replace block ~ ~ ~ container.0 from entity @p hotbar.0`, `item modify entity @s armor.head my_pack:enchant`.
- `<count>` 1..99 in the tree (and must not exceed the item's max stack size, else fail).
- Result/store: `block` targets: success 1, result 1; `entity` targets: result = number of entities whose items changed. For `fill`/`override` the exact result value is **UNVERIFIED** (wiki page not updated).
- Fails: slot missing on container/entity, block not a container, modifier invalid, count > stack limit, source unresolved.
- Note the wiki's `slot_source error in Java 26.3` banner on the item page indicates the page is incomplete.

### slot_source values
A `<slots>` argument is one of:
1. A slot name `slot_type` or `slot_type.N` or wildcard `slot_type.*` - shorthand for `{type:"minecraft:slot_range",slots:"<name>"}` (e.g. `hotbar.4` == `{type:"minecraft:slot_range",slots:"hotbar.4"}`).
2. An inline SNBT slot source (types: `minecraft:slot_range`, `minecraft:contents`, `minecraft:filtered`, `minecraft:group`, `minecraft:limit_slots`, `minecraft:empty`; registry `slot_source_type` = contents, empty, filtered, group, limit_slots, slot_range).
3. A resource location of a datapack slot source file (`data/<ns>/slot_source/<name>.json`, new in 26.3 snapshot 1). A string NBT tag is parsed as a resource location. How the parser disambiguates a bare word like `my_pack:foo` vs slot names is UNVERIFIED (names have no `:`; ids normally do).

Slot source definitions (JSON/SNBT) per `https://minecraft.wiki/w/Slot_source`:
- `slot_range`: `{type, source?: block_entity|this|attacking_entity|last_damage_player|direct_attacker|target_entity|interacting_entity|container (default container), slots: "<slot name incl. *>"}`. In command context only `container` (and contextual `this`) is meaningful: `command_slot_source` loot context supplies `container` (the targeted block/entity inventory), `origin`, optional `this_entity` (`@s`).
- `contents`: `{type, component: minecraft:bundle_contents|minecraft:charged_projectiles|minecraft:container, slot_source}`; selects the slots (now INCLUDING empty ones since 26.3-snap1) of the item container component of the items picked by the nested slot source.
- `filtered`: `{type, item_filter: <item predicate JSON>, slot_source}`.
- `group`: `{type, terms:[slot sources...]}` (duplicates repeated; 26.3-snap9: `group` may only be an inline definition in a top-level file).
- `limit_slots`: `{type, limit:int, slot_source}`.
- `empty`: `{type}`.
- 26.3-snap1 added then snap4 REMOVED `minecraft:reference`; since snap4 any field that took a slot source accepts a namespaced-id string (reference) and list fields accept tags/lists. Snap 9: slot source definition files may be entirely a string reference (MC-310318).
- `execute if|unless slots block|entity ...` (counts slots present) and `execute if|unless items block|entity ... <slot source> <item predicate>` (26.3 changes; see execute doc).

### Slot names (command `item_slot` / slot_range `slots`)
Source: `https://minecraft.wiki/w/Slot` (page flagged outdated) corrected by version pages. **Treat entries marked UNVERIFIED with care.**

| Name | N range | Notes |
|---|---|---|
| `contents` | - | single-slot containers (item entities, item frames, item displays...) |
| `container.N` | 0-53 | any container block / entity inventory (chest 0-26/53, furnace 0-2, brewing stand 0-4, hopper 0-4, dispenser 0-8, shulker 0-26, player-as-container 0-35...) |
| `hotbar.N` | 0-8 | players |
| `inventory.N` | 0-26 | player main inventory (internal 9-35) |
| `enderchest.N` | 0-26 | players |
| `weapon` / `weapon.mainhand` | - | mainhand (internal 98) |
| `weapon.offhand` | - | offhand (99) |
| `armor.head` `armor.chest` `armor.legs` `armor.feet` | - | 103/102/101/100 |
| `armor.body` | - | body slot: wolf armor, horse armor, llama carpet, happy ghast harness etc. (105) |
| `saddle` | - | saddle slot (was `horse.saddle` until 1.21.5) |
| `horse.N` | 0-14 | chested horse/donkey/mule/llama chest slots (500-514) |
| `horse.chest` | - | the chest item slot of chested equines (499) |
| `player.cursor` | - | item held by cursor in GUI (not creative) (499) |
| `player.crafting.N` | 0-3 | 2x2 crafting grid |
| `mob.inventory.N` | UNVERIFIED (0-7 expected: villager 300-307) | villager/wandering trader/pillager/piglin inventories; REPLACES `villager.N` and `piglin.N` (removed 26.1 snapshot 4) |

Wildcards (`item fill/override/replace`, `execute if items`, slot_range): `container.*` (0-53), `hotbar.*` (0-8), `inventory.*` (0-26), `enderchest.*` (0-26), `horse.*` (0-14), `weapon.*` (mainhand, offhand), `armor.*` (head, chest, legs, feet, body), `player.crafting.*` (0-3); presumably `mob.inventory.*` (UNVERIFIED).
`loot replace` still takes a single `mc:item_slot` (no sources/wildcards) in 26.3.
Known quirk (MC-270664, won't fix): `player.cursor` and `horse.chest` share internal index 499, and `player.crafting.*` overlaps `horse.*` 500+.

## 6. loot  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/loot`

```
loot (give <players: entity players multiple> | insert <targetPos: block_pos> | spawn <targetPos: vec3>
      | replace block <targetPos: block_pos> <slot: item_slot> [<count: int >=0>]
      | replace entity <entities: entity entities multiple> <slot: item_slot> [<count: int >=0>] )
    <source>
<source> =
    fish <loot_table> <pos: block_pos> [mainhand|offhand|<tool: item_stack>]  [X]
    loot <loot_table>                                                          [X]
    kill <target: entity single>                                               [X]
    mine <pos: block_pos> [mainhand|offhand|<tool: item_stack>]               [X]
```
- `loot_table`: resource location OR inline SNBT table (since 1.20.5). `fish`: `loot_table` should be a fishing-type table; origin block center, tool default none (empty hand). `kill`: damage source `magic`, this=target. `mine`: block state + BE data at pos, tool.
- Targets: `give` distributes to players' inventories (drops what doesn't fit); `insert` into a container block; `spawn` drops item entities at a position (doubles); `replace` fills consecutive slots starting at `<slot>` (`<count>` slots cleared first; extra stacks ignored).
- Store result: `spawn` = stacks spawned, `give` = stacks given, `insert` = stacks inserted, `replace` = stacks placed; success 1. Failure/error -> 0. Fails on unloaded block, non-container, missing slot, invalid table, `mainhand` with non-living executor.
- Unchanged since 1.21.4.

## 7. clear  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/clear`
```
clear [X]
clear <targets: entity players multiple> [X]
clear <targets> <item: item_predicate> [X]
clear <targets> <item> <maxCount: int >=0> [X]
```
- `maxCount` 0 = count matches without removing. Omitted -> clear all matches. Targets default to the executing player (required to be a player).
- Fails if targets don't resolve, or no matching items. Success 1; store result = number of items removed (or matched with count 0).
- Component predicates via `item_predicate` since 1.20.5 (`clear @s *[custom_data~{x:1}]`). Also clears crafting grid and cursor slots(not creative).

## 8. clone  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/clone`

```
clone [from <sourceDimension: dimension>] <begin: block_pos> <end: block_pos> [to <targetDimension: dimension>] <destination: block_pos>   [X]
      [strict] [replace|masked|filtered <filter: block_predicate>] [force|move|normal]
```
Full tree (all `[X]` at every optional boundary): after `<destination>`: optionally `strict`, then optionally one of `replace`, `masked`, `filtered <filter>`, then optionally `force|move|normal`. `strict` only precedes the mask mode, it does not follow it. The dimension prefix/infix forms: `from <dim>` before begin; `to <dim>` between end and destination. (Both can appear.)
- `<destination>` = lowest-corner position of the copy.
- Modes: `replace` (default): copy all; `masked`: copy only non-air; `filtered`: only blocks matching the predicate (tags ok). `normal` (default): fail if regions overlap; `force`: allow overlap; `move`: after cloning, source replaced with air (for `filtered`, only cloned blocks), overlapping not allowed unless... (`replace move` fails on overlap per wiki).
- Limit: volume must be <= gamerule `max_block_modifications` (renamed from `commandModificationBlockLimit` in 1.21.11; default UNVERIFIED, was 32768).
- Fails: unloaded/out-of-world region, volume over limit, overlap in `normal`, no blocks to copy (`replace` with only barrier... per wiki), `filtered` with a barrier.
- Success 1; store result = number of blocks cloned. History: 1.19.4 (23w03a) dimension args and gamerule limit; 1.21.5 `strict`.

## 9. damage  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/damage`
```
damage <target: entity single> <amount: float >=0> [X]
    [<damageType: resource damage_type>] [X]
        at <location: vec3> [X]
        by <entity: entity single> [X]  [from <cause: entity single> [X]]
```
- Default damage type `minecraft:generic`. 26.3 damage types (registry): arrow, bad_respawn_point, cactus, campfire, cramming, dragon_breath, drown, dry_out, ender_pearl, explosion, fall, falling_anvil, falling_block, falling_stalactite, fireball, fireworks, fly_into_wall, freeze, generic, generic_kill, hot_floor, in_fire, in_wall, indirect_magic, lava, lightning_bolt, mace_smash, magic, mob_attack, mob_attack_no_aggro, mob_projectile, on_fire, out_of_world, outside_border, player_attack, player_explosion, sonic_boom, spear, spit, stalagmite, starve, sting, sulfur_cube_hot, sweet_berry_bush, thorns, thrown, trident, unattributed_fireball, wind_charge, wither, wither_skull (+ datapack-defined).
- `by` = direct source (e.g. the arrow), `from` = causing entity (e.g. the skeleton). `at` = position-only source.
- Success only if damage was actually dealt (fails for invulnerable/immune, amount 0, etc.). Success 1, result 1. Fails if target isn't damageable. `damage ... generic_kill` ignores everything (like /kill).
- Unchanged since 1.19.4 (new `spear`, `sulfur_cube_hot` damage types exist in registry).

## 10. defaultgamemode / gamemode  (L=gamemasters)
Sources: `https://minecraft.wiki/w/Commands/defaultgamemode`, `.../gamemode`
```
defaultgamemode <gamemode: gamemode> [X]
gamemode <gamemode> [X]  [<target: entity players multiple>] [X]
```
- `gamemode` without target needs a player executor. Success 1; store result = number of players whose mode changed. `defaultgamemode`: only changes online players if `force-gamemode` is true or the world is LAN-published (then count = players changed); otherwise success with count 0. In 26.1/26.2 changing game mode via commands/F3+F4 also changes the world default game mode (see section 1; scope UNVERIFIED); MC-308694 (26.2) fixed `/defaultgamemode` in LAN worlds changing everyone's mode to "Settings for Other Players".

## 11. effect  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/effect`
```
effect clear [X]
effect clear <targets: entity entities multiple> [X] [<effect: resource mob_effect>] [X]
effect give <targets> <effect> [X]
effect give <targets> <effect> <seconds: int 1..1000000> [X]  [<amplifier: int 0..255> [X]  [<hideParticles: bool>] [X]]
effect give <targets> <effect> infinite [X]  [<amplifier> [X] [<hideParticles>] [X]]
```
- NOTE: the tree gives `seconds` min **1** (wiki says 0..1000000; trust tree). Default duration 30 s; for `instant_damage`/`instant_health`/`saturation` the number is ticks and default 1. Default amplifier 0, hideParticles false. `clear` without targets = executor.
- Effect ids (26.3 registry, 40): absorption, bad_omen, blindness, breath_of_the_nautilus, conduit_power, darkness, dolphins_grace, fire_resistance, glowing, haste, health_boost, hero_of_the_village, hunger, infested, instant_damage, instant_health, invisibility, jump_boost, levitation, luck, mining_fatigue, nausea, night_vision, oozing, poison, raid_omen, regeneration, resistance, saturation, slow_falling, slowness, speed, strength, trial_omen, unluck, water_breathing, weakness, weaving, wind_charged, wither.
- Result: `give`/`clear` success 1 if at least one entity changed; store result = number of entities affected (since 26.3-snapshot-1 the count only includes entities actually changed, MC-308013). Fails if immune or the new effect is not stronger/longer with same hideParticles.

## 12. enchant  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/enchant`
```
enchant <targets: entity entities multiple> <enchantment: resource enchantment> [X] [<level: int >=0>] [X]
```
Default level 1. Applies to the item in the target's MAINHAND only. Fails: no item, unsupported/conflicting/already present, level above enchantment max. Result: store result/success count = number of entities enchanted (26.3: counts only actual changes). `level` 0 is parseable (removes? UNVERIFIED).

## 13. experience / xp  (L=gamemasters; `xp` redirects to `experience`)
Source: `https://minecraft.wiki/w/Commands/experience`
```
experience add <target: entity players multiple> <amount: int> [X] [points|levels] [X]
experience set <target: entity players multiple> <amount: int >=0> [X] [points|levels] [X]
experience query <target: entity players single> (points|levels) [X]
```
Default unit for add/set = points. `add` accepts negatives. `set points` fails for a player when amount >= remaining capacity of the current level bar. Store result: `query` = value; `add`/`set` = number of players affected; success 1.

## 14. fill  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/fill`
```
fill <from: block_pos> <to: block_pos> <block: block_state> [X]
    destroy | hollow | keep | outline | strict                       [X each]
    replace [X]                                                      (bare replace is a leaf: nothing may follow it except <filter>)
    replace <filter: block_predicate> [X]  [destroy|hollow|outline|strict] [X]
```
- Exact dump shape: after `<block>` the literals are `destroy`, `hollow`, `keep`, `outline`, `replace`, `strict` (all executable leaves except `replace`, which has the single child `<filter>`); `<filter>` is executable and its only children are `destroy`, `hollow`, `outline`, `strict` (1.21.5+; `keep` is not allowed after a filter). `strict` therefore cannot be combined with `destroy|hollow|outline|keep` except via `replace <filter> strict`.
- Modes: `replace` (default; optional filter), `destroy` (drops as if mined, all blocks), `keep` (air only), `hollow` (edge placed, interior air; acts like replace if any dimension <3), `outline` (edge only), `strict` (no block/shape updates).
- Volume <= `max_block_modifications`. Block entity data: omitted `{}` preserves existing BE data (1.21.5). Fails if region unloaded/out of world, volume too large, or nothing changed. Success 1; store result = blocks changed.

## 15. fillbiome  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/fillbiome`
```
fillbiome <from: block_pos> <to: block_pos> <biome: resource worldgen/biome> [X] [replace <filter: resource_or_tag worldgen/biome> [X]]
```
Biomes are stored per 4x4x4 cell; store result = number of cells changed. Volume limit = `max_block_modifications`. Fails if unloaded/outside world/over limit (MC-271297: also fixed to fail when nothing changed). Upcoming in 26.4 snapshot 1: block-accurate biome fill (NOT in 26.3).

## 16. give  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/give`
```
give <targets: entity players multiple> <item: item_stack> [X] [<count: int >=1>] [X]
```
Max total 100 stacks worth of items; 26.3: exceeding signals an ERROR (previously message + return 0). Store result = number of players given; success 1. Items that don't fit are dropped. Item components in `[...]`.

## 17. kill  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/kill`
```
kill [X] [<targets: entity entities multiple>] [X]
```
Defaults to `@s`. Uses `generic_kill` damage, bypasses totems/armor/creative(?). Success 1; result = number of entities killed. Fails if targets resolve to none. (MC-307002 fixed in 26.2: killing an end crystal no longer aborts the dragon respawn.)

## 18. locate  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/locate`
```
locate structure <structure: resource_or_tag_key worldgen/structure> [X]
locate biome <biome: resource_or_tag worldgen/biome> [X]
locate poi <poi: resource_or_tag point_of_interest_type> [X]
```
Output text is a clickable `/tp` link (not shown when run from functions). Store result = horizontal distance (structure/poi) or distance (biome); success 1. Fails if not found within search radius (structure: 201x201 spacing cells; biome: 12801x12801 blocks; poi: 256-block sphere). 26.3 snapshot 10: structure search perf improved. MC-269520 (froze with no structures) fixed 26.2.

## 19. particle  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/particle`, `https://minecraft.wiki/w/Particle_format`
```
particle <name: particle> [X]
particle <name> <pos: vec3> [X]
particle <name> <pos> <delta: vec3> <speed: float >=0> <count: int >=0> [X] [force|normal] [X] [<viewers: entity players multiple>] [X]
```
- `normal` visible within 32 blocks (not at "Minimal" particles setting), `force` within 512 blocks. count 0: single particle with velocity = delta*speed; count>0: spawns count particles gaussian-distributed by delta.
- Particle options are SNBT (1.20.5+): `dust{color:[1.0f,0.0f,0.0f],scale:1.5f}`, `dust_color_transition{from_color:[..],to_color:[..],scale:1f}`, `block{block_state:"minecraft:stone"}` (also `block_marker`, `falling_dust`, `block_crumble`, `dust_pillar`), `item{item:"minecraft:diamond"}`, `entity_effect{color:[r,g,b,a]}`, `effect{power:1f,color:[r,g,b]}`/`instant_effect` (25w36a), `flash{color:[r,g,b,a]}` (color required), `trail{target:[x,y,z],color:int,duration:int}`, `vibration{destination:{type:"block",pos:[x,y,z]},arrival_in_ticks:int}`, `sculk_charge{roll:f}`, `shriek{delay:int}`, `dragon_breath{power:f}`, `tinted_leaves{color:...}`. Colors may also be packed ints.
- Store result = number of players who can see it; success 1; fails if no viewer in range.
- Particle ids in 26.3 (128): angry_villager, ash, block, block_crumble, block_marker, bubble, bubble_column_up, bubble_pop, campfire_cosy_smoke, campfire_signal_smoke, cherry_leaves, cloud, composter, copper_fire_flame, crimson_spore, crit, current_down, damage_indicator, dolphin, dragon_breath, dripping_dripstone_lava, dripping_dripstone_water, dripping_honey, dripping_lava, dripping_obsidian_tear, dripping_water, dust, dust_color_transition, dust_pillar, dust_plume, effect, egg_crack, elder_guardian, electric_spark, enchant, enchanted_hit, end_rod, entity_effect, explosion, explosion_emitter, falling_dripstone_lava, falling_dripstone_water, falling_dust, falling_honey, falling_lava, falling_nectar, falling_obsidian_tear, falling_spore_blossom, falling_water, firefly, firework, fishing, flame, flash, geyser, geyser_base, geyser_plume, geyser_poof, glow, glow_squid_ink, gust, gust_emitter_large, gust_emitter_small, happy_villager, heart, infested, instant_effect, item, item_cobweb, item_slime, item_snowball, landing_honey, landing_lava, landing_obsidian_tear, large_smoke, lava, mycelium, nautilus, note, noxious_gas, noxious_gas_cloud, ominous_spawning, orange_poplar_leaves, pale_oak_leaves, pause_mob_growth, poof, portal, raid_omen, rain, red_poplar_leaves, reset_mob_growth, reverse_portal, scrape, sculk_charge, sculk_charge_pop, sculk_soul, shriek, small_flame, small_gust, smoke, sneeze, snowflake, sonic_boom, soul, soul_fire_flame, spit, splash, spore_blossom_air, squid_ink, sulfur_bubbles, sulfur_cube_goo, sweep_attack, tinted_leaves, totem_of_undying, trail, trial_omen, trial_spawner_detection, trial_spawner_detection_ominous, underwater, vault_connection, vibration, warped_spore, wax_off, wax_on, white_ash, white_smoke, witch, yellow_poplar_leaves. (`rain`/weather particles cannot be spawned by command per wiki.)

## 20. place  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/place`
```
place feature <feature: mc:feature> [X] [<pos: block_pos>] [X]        (26.3: id OR inline SNBT feature)
place jigsaw <pool: resource_key worldgen/template_pool> <target: resource_location> <max_depth: int 1..20> [X] [<position: block_pos>] [X]
place structure <structure: resource_key worldgen/structure> [X] [<pos>] [X]
place template <template: resource_location> [X] [<pos>] [X] [<rotation: template_rotation>] [X] [<mirror: template_mirror>] [X] [<integrity: float 0..1>] [X] [<seed: int>] [X] [strict] [X]
```
Each optional arg is positional (you must supply earlier ones to supply later ones). Defaults: pos = execution pos, rotation none, mirror none, integrity 1.0, seed 0. Fails: unknown id, chunk unloaded/outside world, empty template, no valid position. Success 1, result 1. Error (not fail) when chunks unloaded for `jigsaw/structure` (store unchanged). `feature` registry renamed `worldgen/configured_feature` -> `worldgen/feature` in 26.3.

## 21. playsound / stopsound  (L=gamemasters)
Sources: `https://minecraft.wiki/w/Commands/playsound`, `.../stopsound`
```
playsound <sound: resource_location> [X]
    (master|music|record|weather|block|hostile|neutral|player|ambient|voice|ui) [X]
        <targets: entity players multiple> [X]
            <pos: vec3> [X]
                <volume: float >=0> [X]  <pitch: float 0..2> [X]  <minVolume: float 0..1> [X]
stopsound <targets: entity players multiple> [X]
    (*|master|music|record|weather|block|hostile|neutral|player|ambient|voice|ui) [X]  [<sound: resource_location>] [X]
```
- The `ui` source category is new since 1.21.6. In the tree, `playsound <sound> <source>` and `<targets>` each are executable (so source/targets/pos defaults: source `master`, targets `@s`, pos = execution pos, volume 1, pitch 1, minVolume 0).
- Volume >1 extends audible range (16*volume blocks). minVolume > 0 lets distant targets hear it at that volume 2 blocks away. Pitch clamps below 0.5.
- Fails if no targets or nobody can hear (minVolume 0 and all outside range). Store result = number of players who hear it. `stopsound` result = number of targeted players; `*` = all sources with named sound. Sound ids are sound EVENT names from sounds.json (e.g. `entity.pig.ambient`), not asset paths.

## 22. publish  (L=owners)
Source: `https://minecraft.wiki/w/Commands/publish`
```
publish [X] [<allowCommands: bool>] [X] [<port: int 0..65535>] [X]
```
Singleplayer-only (not on dedicated servers). 26.3: no `<gamemode>`. Result = port; fails if already published or port busy. `unpublish` (26.1+, `unpublish` command, owners) closes the LAN server.

## 23. recipe  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/recipe`
```
recipe (give|take) <targets: entity players multiple> (* | <recipe: resource_key recipe>) [X]
```
Fails if nothing newly unlocked/locked. Store result = total recipes given/taken (26.3: counts only changed players). `<recipe>` is a `resource_key` (runtime check).

## 24. ride  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/ride`
```
ride <target: entity single> mount <vehicle: entity single> [X]
ride <target> dismount [X]
```
Fails: target already riding, vehicle is marker/player/fishing bobber/leash knot/lightning bolt, loop, different dimensions, dismount with no vehicle. Result 1/1 (0 on fail).

## 25. rotate  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/rotate`
```
rotate <target: entity single> <rotation: rotation> [X]
rotate <target> facing <facingLocation: vec3> [X]
rotate <target> facing entity <facingEntity: entity single> [<facingAnchor: eyes|feet>] [X]
```
Result 1/1, fails if entity unresolved. (Added 1.21.2; permission level raised 0->2.)

## 26. setblock  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/setblock`
```
setblock <pos: block_pos> <block: block_state> [X] [destroy|keep|replace|strict] [X]
```
`replace` default (no drops/sound), `destroy` (drops, break sound/particles), `keep` (only if air), `strict` (no updates). Fails: unloaded/outside world, `keep` on non-air, identical block (ignoring BE data), unstable redstone replacement. Result 1/1.

## 27. spawnpoint  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/spawnpoint`
```
spawnpoint [X] [<targets: entity players multiple>] [X] [<pos: block_pos>] [X] [<rotation: rotation>] [X]
```
Defaults: targets = executor, pos = executor pos, rotation = executor's. Works in any dimension (use `execute in`). Result: success 1/result 1 (UNVERIFIED whether result = number of players; wiki says 1/0). Coordinate limits x,z in [-30000000,30000000), y in [-20000000,20000000). `<rotation>` is yaw+pitch since 1.21.9 (was single `<angle>` yaw).

## 28. spectate  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/spectate`
```
spectate [X] [<target: entity single> [X] [<player: entity players single>] [X]]
```
No-arg form stops spectating. Fails if target==player, player not in Spectator, non-player executor with no `<player>`. Cross-dimension OK since 1.19.4. Result 1/1.

## 29. spreadplayers  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/spreadplayers`
```
spreadplayers <center: vec2> <spreadDistance: float >=0> <maxRange: float >=1> [under <maxHeight: int>] <respectTeams: bool> <targets: entity entities multiple> [X]
```
Spreads into the execution dimension. Never places on lava/water/bubble column/fire; 26.3: tag `#minecraft:entities_can_teleport_to` controls safe landing blocks. Fails if no valid positions or too many targets for `spreadDistance`. Success 1; result = number of teams (if `respectTeams true`) else number of entities.

## 30. summon  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/summon`
```
summon <entity: resource entity_type> [X] [<pos: vec3>] [X] [<nbt: nbt_compound_tag>] [X]
```
Fails: player/fishing bobber, unloaded, out-of-range pos (x,z [-30000000,30000000), y [-20000000,20000000)), hostile monster in Peaceful (1.21.9+), duplicate UUID. Result 1/1. NBT is the entity's SNBT; `Passengers`, `Pos`, `Rotation:[yaw,pitch]`, `CustomName` (SNBT text component since 1.21.5), `equipment` (1.21.5 replaced `HandItems/ArmorItems` with `equipment:{mainhand,offhand,head,chest,legs,feet,body,saddle}` - UNVERIFIED, confirm in data doc), `active_effects`, etc.

## 31. swing  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/swing`
```
swing [X] [<targets: entity entities multiple>] [X]
   (mainhand|offhand) [X] [<animation: whack|stab|none>] [X] [<duration: time >=1t>] [X]
```
Defaults: targets `@s`, hand `mainhand`, animation `whack`, duration `6t`. Result: success 1; store result = number of entities targeted (in 26.1-26.3 wiki: "number of targeted entities"). Not all entities animate on client. Players: attack strength no longer reset (26.3).  `none` as animation value is per wiki/argument page - UNVERIFIED in actual behavior.

## 32. teleport / tp  (L=gamemasters; `tp` redirects to `teleport`)
Source: `https://minecraft.wiki/w/Commands/teleport`
```
teleport <destination: entity single> [X]                  (executor -> entity)
teleport <location: vec3> [X]                              (executor -> position)
teleport <targets: entity entities multiple> <destination: entity single> [X]
teleport <targets> <location: vec3> [X]
teleport <targets> <location> <rotation: rotation> [X]
teleport <targets> <location> facing <facingLocation: vec3> [X]
teleport <targets> <location> facing entity <facingEntity: entity single> [<facingAnchor>] [X]
```
Limits x,z [-30000000,30000000), y [-20000000,20000000). Without rotation: keeps rotation (unless destination is an entity: copies its rotation). `~`/`^` relative to the execution position/rotation. Can load chunks. Cross-dimension teleport to entity works. Success 1; store result = number of teleported entities.

## 33. transfer  (L=admins)
Source: `https://minecraft.wiki/w/Commands/transfer`
```
transfer <hostname: string phrase> [X] [<port: int 1..65535>] [X] [<players: entity players multiple>] [X]
```
Default port 25565, default players = executor. Result = number of players transferred.

## 34. weather  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/weather`
```
weather (clear|rain|thunder) [X] [<duration: time >=1t>] [X]
```
Default random durations: clear 12000-180000 ticks, rain 12000-24000, thunder 3600-15600. Store result = duration (-1 if omitted). Works in any dimension (1.20.5).

## 35. worldborder  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/worldborder`
```
worldborder add <distance: double -59999968..59999968> [X] [<time: time >=0>] [X]
worldborder set <distance: double -59999968..59999968> [X] [<time: time >=0>] [X]
worldborder center <pos: vec2> [X]
worldborder damage amount <damagePerBlock: float >=0> [X]
worldborder damage buffer <distance: float >=0> [X]
worldborder get [X]
worldborder warning distance <distance: int >=0> [X]
worldborder warning time <time: time >=0> [X]
```
Time default unit = TICKS since 1.21.11 (was integer seconds). Per-dimension border since 1.21.9. Center x,z within +-29999984. Size must end in [1.0, 59999968]. Fails if unchanged. Store result: `add` = distance arg; `set` = change in width; `get` = size rounded; `damage amount/buffer` = value; `warning distance/time` = value; `center` = 0. Initial border: diameter 59999968, damage 0.2/block, buffer 5, warning distance 5, warning time 15 s.

## 36. waypoint  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/waypoint`
```
waypoint list [X]
waypoint modify <waypoint: entity single> color <color: team_color> [X]
waypoint modify <waypoint> color hex <color: hex_color> [X]
waypoint modify <waypoint> color reset [X]
waypoint modify <waypoint> style set <style: resource_location> [X]
waypoint modify <waypoint> style reset [X]
```
Locator-bar waypoints. Entities that transmit: living entities with attribute `waypoint_transmit_range` > 0, or whose color was explicitly set this session. `list` store result = number of tracked waypoints. Failure: target not a transmitter. Style default `minecraft:default`; the game does not verify the style exists (resource-pack `waypoint_style` definition). 26.2: color arg is `team_color` (lowercase names only).

## 37. stopwatch  (L=gamemasters)
Source: `https://minecraft.wiki/w/Commands/stopwatch`
```
stopwatch create <id: resource_location> [X]
stopwatch query <id> [X] [<scale: double>] [X]
stopwatch restart <id> [X]
stopwatch remove <id> [X]
```
Real-time (wall clock, ms precision; unaffected by /tick or lag). `query` result = elapsed seconds * scale, floored and clamped to int. Fails on duplicate create / missing id. Check in `execute if|unless stopwatch`. Persistence across restarts: UNVERIFIED.

## 38. fetchprofile  (L=gamemasters) (new 26.1)
Source: tree dump; `https://minecraft.wiki/w/Java_Edition_26.1`
```
fetchprofile entity <entity: entity single> [X]
fetchprofile id <id: uuid> [X]
fetchprofile name <name: greedy string> [X]
```
Prints profile info (players/mannequins). `entity` form fails if entity has no profile. `id`/`name` forms not described on the wiki - semantics UNVERIFIED.

## 39. Server administration (brief)
Tree dump (26.3). Levels: ban, ban-ip, banlist, pardon, pardon-ip, op, deop, kick, whitelist, setidletimeout, transfer = `admins`; stop, save-all, save-off, save-on, publish = `owners`; list, help, me, msg/tell/w, say, teammsg/tm, trigger = `all`.
```
ban <targets: game_profile> [X] [<reason: message>] [X]
ban-ip <target: word> [X] [<reason: message>] [X]      (target = IP or player name)
banlist [X] (ips|players) [X]
pardon <targets: game_profile> [X]
pardon-ip <target: word> [X]
op <targets: game_profile> [X]
deop <targets: game_profile> [X]
kick <targets: entity players multiple> [X] [<reason: message>] [X]
whitelist (on|off|list|reload) [X]   |   whitelist (add|remove) <targets: game_profile> [X]
setidletimeout <minutes: int >=0> [X]
list [X] [uuids] [X]
save-all [X] [flush] [X] ; save-off [X] ; save-on [X] ; stop [X]
```
Dedicated-server oriented; `/kick` can't kick the server owner (`commands.kick.owner.failed`). MC-307513 fixed 26.2 (`/deop` multi-target messages).

---

## 40. Open uncertainties
- `moderators` level name is inferred (no command uses it); exact numeric mapping of the 26.1+ permission model unverified.
- `mob.inventory.N` range and wildcard support; `horse.armor` removal version; exact result values of `item fill/override`.
- Whether `spawnpoint` still accepts yaw-only; `enchant ... 0`; `swing none`; `fetchprofile id|name` semantics; stopwatch persistence; default of `max_block_modifications`.
- Slot-source string disambiguation in the command parser.
- The wiki command pages have no history entries for 26.x on many commands; syntax truth here is from the mcmeta tree dump, semantics from the (possibly stale) wiki pages.
