# slab semantics: name resolution, types and checks

Implemented in `src/sema/` (`check.rs`, `hir.rs`, `ty.rs`, `builtins.rs`, `selector.rs`), tests in `src/sema/tests.rs`. Input: the syntax tree (`docs/lang/syntax.md`). Output: a typed **HIR** (`sema::hir`) that the next phase lowers to the IR (`docs/lang/ir.md`). `slab check file.slab` runs it.

slab is statically typed with local inference: every expression has a type known at compile time, there are no implicit conversions, and nothing is checked at runtime that could have been checked here.

## Types

| Type | Meaning | Stored in |
|---|---|---|
| `int` | 32-bit signed, wrapping at runtime, `/` and `%` floored | scoreboard |
| `bool` | `true`/`false` (0/1) | scoreboard |
| `str` | text; `+`/`+=` join, f-strings build one | command storage |
| `list<T>` | list of any storable `T` | command storage |
| `Name` | a `struct`, a compound of storable fields | command storage |
| `()` | no value (statements, functions without `->`) | - |
| entity | a reference to one entity (`docs/lang/entities.md`) | scoreboard (an int id) |
| selector | `@a[...]` literal | **not storable** (expression only) |
| coordinate | `~`, `~1`, `^2` | **not storable** (expression only) |

Selectors and coordinates exist only where the language expects them: builtin arguments, context-block operands, `objective[holder]`. They cannot be bound to a variable, passed to a user function, or returned, because the backend has no runtime representation for them yet. Floats do not exist yet (`1.5` is rejected with a clear message).

`int` literals must fit in 32 bits (`-2147483648` is valid). Time literals (`10s`) are ordinary ints counting **ticks**.

## Scoping and names

Semantic analysis runs over the whole project at once: it first registers every item of every module, then resolves imports, then types, then bodies - so modules may refer to each other in any order (and cyclically). Visibility, qualified names and import rules are in `docs/lang/modules.md`.

* Value namespace: locals (block scoped, shadowing allowed, the initialiser of `let x = ...` sees the *outer* `x`), then top-level functions, `global`s, `const`s, `objective`s. Type namespace: `int bool str list` and `struct`s. Top-level items may appear in any order.
* Defining a name twice, or a function/global named like a builtin (`print`, `effect`, `tag`, ...), is an error.
* Parameters are the function's own copies and can be assigned. `for` loop variables are immutable. Everything else is immutable unless declared `let mut`.
* Assignable places: a variable, a global, a field/index of a mutable place, `objective[holder]`. Constants are inlined and cannot be assigned.

## Compile-time constants

`const` values, `global` initialisers, `#[every(N)]` arguments and the "constant" parameters of builtins (selector tags in `tag.has`, `#[every]` periods) are evaluated by the compiler: literals, other consts, arithmetic (checked: overflow and division by zero are errors, `/` `%` floored), comparisons, `&& || !`, lists and struct literals. A global initialiser cannot mention another global or call a function. A `const` that depends on itself is an error.

## Statements and flow

* `if`/`while` conditions must be `bool`. `for i in a..b` / `a..=b` needs two `int`s; `for x in list` walks a list; nothing else can be iterated.
* `break`/`continue` need an enclosing loop in the same context block.
* **Context blocks** (`as @a at @s { }`): `as`/`at` take selectors, `in` a constant dimension id, `positioned`/`facing`/`rotated` take `int` or `~`/`^` operands, `align` a subset of `xyz`, `anchored` `eyes`|`feet`. Because a block may run once per entity, `return` is an error inside one, and `break`/`continue` cannot cross its boundary. Locals of the enclosing function are visible and mutable inside.
* **Definite return**: a function with a return type must return on every path. `if`/`else` where both sides return, `loop` without a `break`, nested blocks and bare `return` count; `while`, an `if` without `else`, or falling off the end do not.
* Calls are checked for arity and argument types (with the parameter type as the expected type, which is how `[]` and `{ ... }` get their types). `return` is checked against the declared return type.

## Functions, entry points

`#[load]` runs at load/`/reload`, `#[tick]` every tick, `#[every(N)]` every `N` ticks (`N >= 1`, an int or a time literal like `1s`). At most one per function; such functions take no parameters and return nothing. Any other attribute is an error, and attributes are only valid on functions.

## Builtins (`sema/builtins.rs`)

Builtins that act on the world return `bool`: **did the command work?** `if !tp(boss, 0, 70, 0) { ... }` catches a missing target. Ignoring the result is free (the plain command is emitted); using it costs one `execute store success`. "Worked" is Minecraft's own notion: it targeted something and changed something, so `tag.add` fails if the tag is already there, `setblock` fails if the block is unchanged, `kill`/`tp`/`effect.give` fail when nothing matches, and a run-time value out of range fails that one command.

| Call | Returns | Notes |
|---|---|---|
| `print(v)` | `()` | any storable value, or an f-string |
| `say(text)` | `()` | any `str`; a computed one is passed at run time |
| `kill(sel)` | `bool` | |
| `exists(sel)` | `bool` | any entity matches |
| `tag.add(sel, name)` `tag.remove` | `bool` | any `str`; tag name is `[A-Za-z0-9_.+-]+` |
| `tag.has(sel, "t")` | `bool` | the tag must be a compile-time constant |
| `effect.give(sel, id, ticks [, amplifier])` | `bool` | duration in **ticks** (`10s` = ten seconds), amplifier 0..=255; both may be variables |
| `effect.clear(sel)` | `bool` | |
| `tp(sel, x, y, z)` | `bool` | each coordinate is any `int` expression or `~`/`^` |
| `setblock(x, y, z, id)` | `bool` | `id` is any `str` |
| `summon(id [, x, y, z])` | entity | `id` is any `str` |

Methods: `list.push(x)` (needs a mutable place), `list.len()`, `str.len()`. Whether the *backend* can lower each builtin yet is separate from this table; a builtin the backend lacks is reported by the lowering, not here.

## Selectors

`@e[type=zombie, distance=..10, tag=!boss, scores={kills=1..}, limit=1]` is converted to the datapack module's typed `Selector` during checking, so unknown keys, bad numbers/ranges/ids/names and repeated single-use options are compile errors with the span of the offending argument. Supported keys: `x y z dx dy dz distance x_rotation y_rotation level limit sort tag team name gamemode type predicate scores`. `nbt=` and `advancements=` are not supported yet.

## Diagnostics

* Errors are collected for the whole program and reported in source order. An expression that already failed gets the internal `{error}` type, which is compatible with everything, so one mistake produces one error (`let y = x + 1;` with an unknown `x` reports only `x`).
* Warnings (do not stop compilation): `unreachable statement`, `unused variable` (names starting with `_` are exempt; parameters never warn), `this expression has no effect`.
* Errors carry hints where there is an obvious fix.

## What the checker does not decide

Backend limits (the 65536-commands-per-tick loop ceiling, dynamic list indexing, variable coordinates, comparing strings) are reported by `codegen`, not here (`docs/lang/codegen.md`). Names are checked case-sensitively; mangling to the IR's `[a-z0-9_]+` names (and avoiding collisions after lowercasing) happens in the HIR -> IR translation.
