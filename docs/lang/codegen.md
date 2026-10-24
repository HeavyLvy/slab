# HIR -> IR translation (`src/codegen.rs`)

After checking, `codegen::generate` turns the typed HIR into the IR (`docs/lang/ir.md`), which `lower.rs` turns into commands. Everything here has already been type checked; it only chooses how constructs map onto the runtime model. Errors from this phase are *backend limits* (valid slab the backend cannot express yet), reported with source positions.

## Runtime representation

| slab | IR kind | lives in |
|---|---|---|
| `int`, `bool` | `Int`, `Bool` | scoreboard (objective = namespace) |
| `str`, `list<T>`, structs | `Data` | command storage `<ns>:vars` / `<ns>:globals` |
| `objective[holder]` | `PlayerScore` / `SetScore` | a real scoreboard objective |

An `int` stored *inside* a list or struct is an int tag, so values are converted at every storage boundary (`ToData` when storing, `ToInt` when reading). A struct is a compound keyed by field name; `p.x` is the path `p.x`.

## Translations

* **Names**: `[a-z0-9_]` only; module `game::arena` + function `start` -> `game__arena__start`; locals keep their name, shadowed or colliding ones get `_2`, `_3` (`docs/lang/modules.md`).
* **`break` / `continue`**: the IR has neither. A loop that uses them gets flag variables (`brk`, `cont`). `break` sets `brk` and every statement after one that may jump, in its block, is wrapped in `if brk == 0 && cont == 0`. `continue`'s flag is cleared at the top of each iteration, and a `for` counter's increment sits outside the guard so `continue` still advances.
* **`while`**: `while c { body }`. If `c` needs statements (a temporary), `c` is recomputed into a flag at the end of each iteration; if the loop also uses `break`, the condition is evaluated *inside* the body so a call in it is not re-run after a break. `loop` is `while true`.
* **`for i in a..b`**: `let i = a; let end = b; while i < end { body; i += 1 }` (`b` is evaluated once; `..=` uses `<=`).
* **`for x in list`**: a copy of the list is consumed from the front (`x = tmp[0]; remove tmp[0]`), because indexing with a variable would need macros.
* **List/struct literals**: constant ones become one NBT literal; otherwise a temporary is built with `Append`/`SetField` before the statement that uses it.
* **Context blocks**: `as @a at @s { .. }` -> `execute as @a at @s run function <block>`; the body is its own function. `return`, `break` and `continue` cannot cross it (checked earlier).
* **`#[every(N)]`**: a counter global `<fn>__every` plus a tick function `<fn>__every_tick` that increments it and calls the function when it reaches `N`, then resets it. `#[every(1)]` is just `#[tick]`.
* **Builtins** become typed `datapack::Command`s: `say`, `kill`, `tag.*`, `effect.give` (ticks are rounded up to whole seconds), `effect.clear`, `tp`, `setblock`, `summon`. `print` is `tellraw @a`. `exists(sel)` / `tag.has` become `execute if entity`.
* **Cleanup** (`docs/lang/memory.md`): `summon` tags the entity `<ns>.owned`, `tag.add` names are remembered, and both feed `_uninstall`.
* **Globals** are initialised once (`_init` only sets one that is unset), so `/reload` keeps their value.

## Variables inside commands: macros

A command argument that is only known at run time cannot be written into the command text. Minecraft's mechanism is the **macro function**: a function with `$` lines, called with `function <f> with storage <id>`, which substitutes `$(key)` from that storage. slab uses it implicitly, so the programmer never writes `$`:

| slab | becomes |
|---|---|
| `tp(@s, x, 64, z)`, `setblock(x, y, z, ..)`, `summon(id, x, y, z)` | a coordinate that is not a literal is a `Hole` in the typed command |
| `effect.give(@s, "speed", secs * 20, level)` | duration (ticks, rounded up to seconds) and amplifier may be holes |
| `positioned x 64 z { .. }`, `facing`, `rotated` | the whole `execute` command is the macro line |
| `xs[i]`, `xs[i] = v`, `xs[i] += 1`, `ps[j].a += 10` | `ReadAt` / `WriteAt` with a hole in the NBT path (`xs[$(h0)]`); `xs[-1]` works too |

Mechanism (`codegen.rs` collects `Holes`, `lower.rs::call_macro` emits):

1. each hole gets a key `h0`, `h1`, ... and the expression that supplies it (bound to a temporary unless it is a literal or variable, since a compound assignment reads and writes the same index);
2. the command, containing `$(h0)` etc., becomes the only line of a helper function `<fn>/m<N>` (rendered with the `$` prefix because the text contains a placeholder);
3. all argument expressions are evaluated **first** (they may call functions that use the scratch storage themselves), then stored in `<ns>:macro` (`execute store result storage <ns>:macro h0 int 1 run scoreboard players get ..`);
4. `function <ns>:<fn>/m<N> with storage <ns>:macro`;
5. the keys are removed again, so nothing is left in `<ns>:macro`.

Constant arguments never use this: `tp(@s, 1, 64, 2)` is the plain command `teleport @s 1 64 2`. A macro call costs about `2n + 3` commands for `n` variables. Values are int tags, which substitute as plain decimals (including negatives). A hole's range can only be checked at run time: an out-of-range duration or a bad index makes that one command fail silently.

String values are *not* substituted into commands (a `"` or `\` in a string would break the line); only numbers are. Text with variables goes through **f-strings** instead.

## F-strings

`print(f"{name} has {n} points")` is one `tellraw @a {text:"",extra:[...]}`: literal text, a `{score:{name,objective}}` component for each number and an `{nbt,storage}` component for each string, list or struct. Components are read when the message is sent, so there is no escaping problem and no macro. Booleans print as 0/1. All temporaries stay alive until the `tellraw` is emitted.

## Backend limits (reported as errors)

| Limit | Why |
|---|---|
| `push` onto a list selected by a variable index (`xs[i].push(v)`) | would modify a copy |
| a score holder must be a selector or string literal (`pts[name]` with a variable `name`) | strings are not substituted into commands |
| string `==`, string concatenation, storing an f-string in a variable | would need string macros |
| a loop runs at most ~21,000 iterations per tick | `max_command_sequence_length` = 65536 commands; the game stops the whole call chain (docs/mc/05-verified-by-slab.md) |
| no floats | `/compute` is not used yet |

## Missed optimisations (known, correct but not minimal)

* storing an int into a field takes two commands (convert to a temporary tag, then copy) instead of one `execute store result storage <field>`;
* every `if` branch is a separate function, even for a single command;
* arguments of recursive calls are copied through temporaries.
