# Memory, cleanup and `_uninstall`

A data pack has no process that "ends": it runs for as long as it is enabled, and whatever it writes into the world (scoreboard scores, command storage, entities, tags) stays in the save. This page says what slab cleans up automatically, what it deliberately leaves, and how to remove everything.

## Why there are no lifetimes, ownership or GC

slab values are **copied**: assigning, passing an argument and returning all copy into the destination slot (`scoreboard players operation`, `data modify ... set from`). There are no references or pointers, so nothing can dangle or alias, which is exactly what Rust's lifetimes and borrow checker prevent. A tracing garbage collector would need a runtime, and a data pack has none. What is left is a much simpler need: **deterministic cleanup at the end of a scope**, the part of Rust that is `Drop`. The compiler inserts it; the programmer writes nothing.

If slab ever gets *entity handles* (a value that refers to a summoned entity), ownership rules become worth designing then, because an entity is a real resource that can leak. Today entities are only reached through selectors and are not first-class values.

## What counts as a leak

State that **grows without bound** or **outlives its use**. Fixed-size state that is reused is not a leak.

| State | Handling |
|---|---|
| `str` / `list` / struct **locals and parameters** (command storage, unbounded size) | removed when the function exits, on every `return` path and when it falls off the end |
| `str` / `list` / struct **temporaries** the compiler creates (a call's result, a list literal being built, a copied argument) | removed as soon as they have been used |
| the shared **return slot** `<ns>:ret` | cleared as soon as the caller has copied the value out (or ignored it) |
| the **recursion stack** `<ns>:stack` | balanced by construction (every push has a pop); empty after a normal run |
| `int` / `bool` locals and temporaries (scoreboard) | **not reset**: a fixed set of slots reused by every call. Resetting them would add commands to every call, and commands per tick are limited. They are removed by `_uninstall` |
| **globals** | persist on purpose (that is what they are for, also across `/reload`) |
| **entities** you `summon` | tagged `<ns>.owned`; killed by `_uninstall` |
| **tags** you add with `tag.add` | removed from every entity by `_uninstall` |
| scores of fake players / entities on your `objective`s | removed with the objective by `_uninstall` |

Locals are dropped at the *function* boundary, not at the end of every `{ }` block: a block's locals share the function's storage slots, so a function that returns early still cleans up everything it declared. A long-running loop inside one function keeps its locals until it finishes.

Cleanup is skipped if Minecraft aborts the call chain mid-function (the 65536-commands-per-tick limit): nothing after that point runs. The slots are simply overwritten the next time the function runs.

## `_uninstall`

Every pack gets `<ns>:_uninstall`, which removes everything the program created:

```
/function mypack:_uninstall
/datapack disable "file/mypack"
```

In order: kill the entities tagged `<ns>.owned`; remove the tags the program added; remove all the program's scoreboard objectives (including the compiler's own `<ns>` objective, with every score on it); remove the compiler's storage (`<ns>:vars`, `<ns>:globals`, `<ns>:ret`, `<ns>:stack`). Entities that the program did not summon survive; they only lose the tags the program added.

A data pack cannot detect that it has been removed, so the call has to be made by hand. Disable the pack afterwards: while it is enabled its `#tick`/`#load` functions keep running and will fail harmlessly on the removed objective, and `/reload` would run `#load` again and recreate the state.

## Implementation notes

* `lower.rs`: `FnState::drop` holds the Data locals of the function (parameters and every `Let` of kind `Data`, found by a scan of the body before lowering). `emit_drop` is called before each `return 1` (after the return value has been assigned) and at the end of the function body. One local is removed inline; two or more share a generated `<fn>/_drop` function. `release_data` removes a Data temporary when it is released; call-argument temporaries are removed only after being copied into the callee's parameters.
* Recursion: a callee's exit cleanup removes the *same* slots the caller is using, which is safe because the caller restores its saved slots from the stack right after the call returns.
* `codegen.rs`: `summon` adds `{Tags:["<ns>.owned"]}` (an NBT argument needs a position, so `~ ~ ~` is used when none is given); `tag.add` names are collected. Both end up in `ir::Program::uninstall`, which `lower.rs` writes into `_uninstall` ahead of the state removal.
* Tests: `src/lower.rs` and `src/compile.rs` check the generated commands; `examples/projects/e2e/checks.json` checks on a real server that locals are gone after the functions ran and that `_uninstall` removes objectives, storage, owned entities and tags while an unrelated entity survives.
