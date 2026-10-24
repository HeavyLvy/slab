# Minecraft Java 26.3 reference (for the slab compiler)

Target: **Java Edition 26.3 "Wilderness Bound"**, released 2026-09-15. Data pack format **121.0**, resource pack format 97.1, protocol 777, data version 5023, requires Java 25.

These docs were compiled from minecraft.wiki, the misode.github.io technical changelog and other web sources. Each section lists its sources. `**UNVERIFIED**` marks claims that weren't confirmed; verify and then fix the doc.

| File | Contents |
|---|---|
| [00-version-deltas-1.21.4-to-26.3.md](00-version-deltas-1.21.4-to-26.3.md) | Per-version breaking/notable changes, "top things to know" cheat sheet |
| [01-datapack-structure.md](01-datapack-structure.md) | pack.mcmeta, folders, `.mcfunction` rules, macros, `#load`/`#tick` tags |
| [02-commands-core.md](02-commands-core.md) | execute, scoreboard, data, function, return, schedule, gamerule, selectors, argument types |
| [03-commands-world-entity.md](03-commands-world-entity.md) | World/entity/item/player commands |
| [04-data-formats.md](04-data-formats.md) | SNBT, text components, predicates, components, advancements, loot tables, recipes, dialogs, entity NBT |
| [05-verified-by-slab.md](05-verified-by-slab.md) | Facts confirmed (or refuted) by running slab's output on a real 26.3 server. Highest authority. |

## Source reliability (trust in this order when docs disagree)

1. **Real 26.3 server** (including `05-verified-by-slab.md`): `commands.json` / `registries.json` from the data generator (`java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports`) and live console tests. Tagged `[G]` / `[V]` in `02-commands-core.md`.
2. **misode/mcmeta** dumps (command tree, vanilla data pack, registries) and **misode/technical-changes** repo.
3. **minecraft.wiki** (often stale for 26.x: several pages still show pre-26.3 shapes).
4. Memory/older guides (do not use).

Known resolved conflicts: permissions are named levels in the command tree (numeric levels in `ops.json` unchanged); `/gamerule` accepts both `advance_time` and `minecraft:advance_time`.

Open items worth testing against a real server before the compiler depends on them: macro limits, leading `/` in `.mcfunction`, `/schedule` across reload, `mob.inventory.N` slot range, `item fill|override` result values, `time_check.clock` requirement.
