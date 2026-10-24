# slab IR and runtime model

Pipeline: source files -> parser -> checker (HIR) -> `codegen` (HIR -> **IR**, `src/ir.rs`) -> **lowering** (`src/lower.rs`) -> typed commands (`src/datapack`) -> pack on disk. `codegen` is `docs/lang/codegen.md`; this file is the IR's runtime model.

The IR is structured (blocks, `if`, `while`, calls), not a flat CFG, because mcfunction control flow is function calls anyway. The frontend only needs to produce `ir::Program`; it never touches command text.

## Runtime model: scoreboard + command storage

| IR kind | Lives in | Notes |
|---|---|---|
| `Int`, `Bool` | scoreboard objective named after the pack namespace | i32, wrapping on overflow; `Bool` is 0/1 |
| `Data` | command storage `<ns>:vars` / `<ns>:globals` | strings, lists, compounds, any NBT |

Slots (all derived from function and variable names, which are `[a-z0-9_]+`):

| What | Scoreboard holder | Storage |
|---|---|---|
| local / param `x` of `f` | `#f.v_x` | `<ns>:vars` path `f.v_x` |
| temp `n` of `f` | `#f.t_n` | `<ns>:vars` path `f.t_n` |
| global `g` | `#g.g` | `<ns>:globals` path `g` |
| return value | `#ret` | `<ns>:ret` path `v` |
| constant `n` (for `*`, `/`, `%`) | `#c_n` / `#c_m<n>` | |
| recursion stack | | `<ns>:stack` lists `i` (ints) and `d` (data) |
| macro arguments | | `<ns>:macro` keys `h0`, `h1`, ... (set right before a macro call, removed right after) |

Everything is created by `<ns>:_init`, which is on `#minecraft:load`: the objective, the constants, each global (only if unset, so `/reload` keeps state), then the `Entry::Load` functions. `Entry::Tick` functions go straight into `#minecraft:tick`.

Semantics worth knowing (all verified on a 26.3 server, see `docs/mc/05-verified-by-slab.md`):

* Integer arithmetic wraps at i32. `/` and `%` are **floored** (`-7 / 2 == -4`, `-7 % 2 == 1`), because that is what the scoreboard does and it avoids extra code. Constants are folded with the same rules.
* No floats yet (`/compute` in 26.3 is the way to add them later).
* `&&` and `||` evaluate both sides (no short circuit), so the lowering rejects calls inside them; hoist calls into locals.
* A `Let` without an initial value does not clear the slot; it keeps whatever the slot held last. Give locals an initial value.

## Control flow

An mcfunction has no blocks, so every `if` branch and every loop body becomes its own function `<ns>:<fn>/b<N>`.

* `if c { A }` -> `execute <tests> run function A`.
* `if c { A } else { B }` -> the test is first stored in a temp (`execute store success score tmp if <tests>`), then `execute if tmp ... run function A` and `execute unless tmp ... run function B`. It must not re-evaluate `c` for the else, because `A` may have changed what `c` reads.
* `while c { body }` -> one function that starts with `execute unless <c> run return 0`, runs the body, then ends with `return run function <itself>` (a tail call).
* Conditions on constants are resolved at compile time and dead branches are dropped.

### `return`

A source-level `return v` stores `v` in `#ret` (or `<ns>:ret v`) and runs `return 1`. A block that may return is called as
`execute <tests> if function <blk> run return 1`, so the non-zero result climbs out through every enclosing block. A block that falls through returns nothing (or `0`), which `if function` treats as "keep going". Blocks that cannot return are called with a plain `function`.

### Limits of loops

Loops are tail-recursive, and the game stops a tick's command chain at `max_command_sequence_length` (gamerule, default 65536). Verified failure mode: `Command execution stopped due to limit (executed 65536 commands)`, and **nothing after the loop runs**. A simple counting loop costs 3 commands per iteration, so about 21,000 iterations is the ceiling per tick; 5000 iterations works. Long computations have to be split over ticks (for example with `schedule`), which the frontend/runtime does not do yet. Raising the gamerule is possible but not done automatically.

## Calls and recursion

Parameters and locals are static slots per function, so non-recursive code is plain scoreboard operations. For a call to `g` from `f`, if `g` can reach `f` again (including `g == f`; computed from the call graph, so mutual recursion works), `f` must protect its slots:

1. evaluate the arguments into temps (left to right; a later call in the argument list forces earlier arguments into temps);
2. **save** every declared variable and live temp of `f` to the `<ns>:stack` lists (`i` for scoreboard slots, `d` for data slots);
3. copy the arguments into `g`'s parameter slots (for a self call all arguments are temps first, otherwise `gcd(b, a % b)` would overwrite `a` before reading it);
4. `function <ns>:g`;
5. **restore** the saved slots in reverse order, then copy `#ret` into the destination.

Return values travel in `#ret` / `<ns>:ret v`, which is never saved. Calls to functions that cannot re-enter the caller emit no stack traffic at all.

## Cleanup

A function's Data locals are removed from storage whenever it exits, Data temporaries as soon as they are used, and the shared `<ns>:ret` once its value has been taken; `<ns>:_uninstall` removes everything the program created, and `Program::uninstall` carries extra commands for it (kill summoned entities, remove added tags). Scoreboard slots are not reset while running. Details and reasoning: `docs/lang/memory.md`.

## Conditions stored as flags

`execute store success score F if A if B` writes `F` only if the chain reaches its last condition: when `A` fails nothing is stored and `F` keeps its old value (verified on 26.3). The lowering therefore clears `F` first whenever it stores more than one test (`Lower::store_success`); a single trailing test stores its own 0.

## Other constructs

`MacroCommand { command, args }` runs a typed command that contains `Hole`s: the values of `args` are stored in `<ns>:macro` and the command, which is the only line of a generated helper function, is called with them (`docs/lang/codegen.md`). `ReadAt` / `WriteAt` do the same for an NBT path with an index hole (`xs[$(h0)]`); `Context` carries `args` for holes in its `execute` clauses; `PrintParts` is a `tellraw` of literal text, `score` and `nbt` components.

`PlayerScore`/`SetScore` read and write a *user* objective (created in `_init` from `Program::objectives`); `EntityExists` is `execute if entity`; `Context` runs a body under `/execute` modifiers as its own function (it may run once per entity, so it cannot `Return`); `Remove` is `data remove`.

## What the lowering checks

Unknown functions/variables/globals, argument count and Int-vs-Data class of every operation, duplicate declarations, invalid names, constant division by zero, entry functions with parameters, calls inside `&&`/`||`. The rest (valid commands, references between generated functions) is `DataPack::validate()`.

## Not done yet

* `break` / `continue` (desugar with a flag variable for now), `for`, short-circuit with calls.
* Floats (`/compute`), string concatenation and formatting (needs macros), entities/selectors as values, `schedule`-based long loops.
* Optimisations: inlining single-command blocks, reusing `#ret` copies, skipping saves of dead slots, `execute store` fusion.
* Typed macro arguments in the datapack module.
