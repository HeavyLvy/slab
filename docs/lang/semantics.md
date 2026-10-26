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
| `Name<A, B>` | an `enum` with its type arguments, `Option<int>`, `Result<(), str>` (`docs/lang/enums.md`) | command storage |
| `()` | no value (statements, functions without `->`); also the payload of `Ok(())` | - |
| `game::Entity` | a reference to one entity (`docs/lang/entities.md`); a built-in type | scoreboard (an int id) |
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

`const` values, `global` initialisers, `#[every(N)]` arguments and the "constant" parameters of builtins (selector tags in `tag::has`, `#[every]` periods) are evaluated by the compiler: literals, other consts, arithmetic (checked: overflow and division by zero are errors, `/` `%` floored), comparisons, `&& || !`, lists and struct literals. A global initialiser cannot mention another global or call a function. A `const` that depends on itself is an error.

## Statements and flow

* `if`/`while` conditions must be `bool`. `for i in a..b` / `a..=b` needs two `int`s; `for x in list` walks a list; nothing else can be iterated.
* `break`/`continue` need an enclosing loop in the same context block.
* **Context blocks** (`as @a at @s { }`): `as`/`at` take selectors, `in` a constant dimension id, `positioned`/`facing`/`rotated` take `int` or `~`/`^` operands, `align` a subset of `xyz`, `anchored` `eyes`|`feet`. Because a block may run once per entity, `return` is an error inside one, and `break`/`continue` cannot cross its boundary. Locals of the enclosing function are visible and mutable inside.
* **`match` / `if let`** (`docs/lang/enums.md`, "Taking a value apart"): the value may be an `int`, `bool`, `str`, an enum or an entity. Patterns: `_`, a name (binds a copy of that part; immutable, scoped to the arm), a literal, `Variant`, `Variant(p, ..)` (every payload value must be given), nested patterns, and `A | B` (alternatives cannot bind). An arm may have a guard `if cond`, which sees the bindings. The arms must cover every value (pattern-matrix check; guarded arms do not count): the error names a missing case, `int` and `str` need a `_` or a name arm. An arm that an earlier arm already covers is a warning. Used as a value, every arm has the same type, except arms that `return`, `break` or `continue` (those fit any type); a block arm in a value must end that way. As a statement, arms may be expressions of any type, blocks, or assignments (`x => total += 1`). `if let P = v { A } else { B }` is a match with the arms `P` and `_`.
* **Operators on structs and decimal literals** (`docs/lang/doubles.md`): `a + b`, `a - b`, `a * b`, `a / b`, `a % b`, `a < b`, `a <= b`, `a > b`, `a >= b` and `-a` on a struct call its method `add`, `sub`, `mul`, `div`, `rem`, `lt`, `le`, `gt`, `ge`, `neg` when it has one (`a += b` is `a = a.add(b)`). A literal with a point (`1.5`), or an integer, where a `#[decimals(N)]` struct (`NearDouble`, `FarDouble`) is expected builds that struct; a literal with more than N digits is an error, and a decimal literal with no expected type is an error that names the fix.
* **Definite return**: a function with a return type must return on every path. `if`/`else` where both sides return, `loop` without a `break`, nested blocks and bare `return` count; `while`, an `if` without `else`, or falling off the end do not.
* Calls are checked for arity and argument types (with the parameter type as the expected type, which is how `[]` and `{ ... }` get their types). `return` is checked against the declared return type.

## Functions, entry points

`#[load]` runs at load/`/reload`, `#[tick]` every tick, `#[every(N)]` every `N` ticks (`N >= 1`, an int or a time literal like `1s`). At most one per function; such functions take no parameters and return nothing. Any other attribute is an error, and attributes are only valid on functions.

## Builtins (`sema/builtins.rs`)

**Methods** (`impl`): `impl Point { fn new(x: int, y: int) -> Point { .. } fn sum(self) -> int { .. } pub fn scaled(self, k: int) -> Self { .. } }` attaches functions to a struct or enum. A function whose first parameter is a bare `self` is a method: `p.sum()` is the call `Point::sum(p)` with the receiver as `self`; one without is an associated function, called `Point::new(1, 2)`. `Self` stands for the type inside the block (also `Self { x: 1 }` and `Self::Variant`). Rules:
* the `impl` must be in the module that declares the type; a method is private to its module unless it is `pub`;
* `self` is a copy like every argument, so a method cannot change the receiver (assigning to `self` only changes the copy); return the new value instead;
* a method and a variant of the same enum cannot share a name, and two methods of a type cannot either;
* methods of a *generic* type (`impl<T> Option<T>`) can only be the standard library's built-in ones (below); there are no generic functions yet;
* there is no trait or interface system: methods are looked up by the receiver's type.

**`Option` and `Result` methods** (built into the standard library, `docs/lang/enums.md`): `is_some`, `is_none`, `unwrap`, `unwrap_or(default)`, `expect(message)` for `Option<T>`; `is_ok`, `is_err`, `unwrap`, `unwrap_or(default)`, `expect(message)`, `ok()` (-> `Option<T>`), `err()` (-> `Option<E>`) for `Result<T, E>`. `unwrap` and `expect` on a missing value cannot panic in a data pack: they print `slab: unwrap on None (main.slab:12)` (or your `expect` message) to chat and return from the function with a zero value of its return type. `slab build --abort-on-unwrap` also runs `datapack disable "file/<namespace>"` (`--pack-id` names another pack), which stops the whole pack. Inside a context block (`as @a { }`), which a `return` cannot leave, they only print.

**Equality** (`==`, `!=`): both sides must have the same type, which can be any storable type: `int`, `bool`, `Entity`, `str`, `list<T>` and structs (compared by contents, field by field and element by element; order matters for lists). A selector that matches one entity may stand on either side of an `Entity` (`boss == @s`, `@s != @p`): it is converted to a handle first, and a selector that can match several entities is the usual error. `int` and `bool` compare as scores; everything else is compared in command storage (below). Selectors and `()` themselves cannot be compared.

Builtins that act on the world return `bool`: **did the command work?** `if !tp(boss, 0, 70, 0) { ... }` catches a missing target. Ignoring the result is free (the plain command is emitted); using it costs one `execute store success`. "Worked" is Minecraft's own notion: it targeted something and changed something, so `tag::add` fails if the tag is already there, `setblock` fails if the block is unchanged, `kill`/`tp`/`effect::give` fail when nothing matches, and a run-time value out of range fails that one command.

| Call | Returns | Notes |
|---|---|---|
| `std::print(v)` | `()` | any storable value, or an f-string |
| `game::say(text)` | `()` | any `str`; a computed one is passed at run time |
| `game::kill(sel)` | `bool` | |
| `game::exists(sel)` | `bool` | any entity matches |
| `game::tag::add(sel, name)` `game::tag::remove` | `bool` | any `str`; tag name is `[A-Za-z0-9_.+-]+` |
| `game::tag::has(sel, "t")` | `bool` | the tag must be a compile-time constant |
| `game::effect::give(sel, id, ticks [, amplifier])` | `bool` | duration in **ticks** (`10s` = ten seconds), amplifier 0..=255; both may be variables |
| `game::effect::clear(sel)` | `bool` | |
| `game::tp(sel, x, y, z)` | `bool` | each coordinate is any `int` expression or `~`/`^` |
| `game::setblock(x, y, z, id)` | `bool` | `id` is any `str` |
| `game::summon(id [, x, y, z])` | `game::Entity` | `id` is any `str` |

Methods: `list.push(x)` (needs a mutable place), `list.len()`, `str.len()`. Whether the *backend* can lower each builtin yet is separate from this table; a builtin the backend lacks is reported by the lowering, not here.

## Selectors

`@e[type=zombie, distance=..10, tag=!boss, scores={kills=1..}, limit=1]` is converted to the datapack module's typed `Selector` during checking, so unknown keys, bad numbers/ranges/ids/names and repeated single-use options are compile errors with the span of the offending argument. Supported keys: `x y z dx dy dz distance x_rotation y_rotation level limit sort tag team name gamemode type predicate scores`. `nbt=` and `advancements=` are not supported yet.

**Variables in selectors**: `$name` / `${expression}` in an option value is filled in at run time (`@e[tag=$t, distance=..${r * 2}, scores={kills=$min..}]`, `tag=!$t`, `tag=pre_$suffix`). The text-like options (`tag team name type gamemode`) take a `str`; the numeric ones (`limit x y z dx dy dz`, and each end of `distance x_rotation y_rotation level` and of a score range) take an `int`, as a whole value or as a range end (`..$r`, `$a..$b`), not mixed with other characters. `sort`, `predicate`, `nbt` and `advancements` cannot take a variable. The selector still works wherever a selector does (`for`, `as`/`at`, builtin targets, `objective[holder]`, `exists`). A selector with a variable is not provably one entity, so `let e: Entity = @e[limit=$n]` is an error (a literal `limit=1` is needed). A value that is not valid where it lands (a tag with a space, an unknown entity type) makes that one command do nothing at run time.

## Diagnostics

* Errors are collected for the whole program and reported in source order. An expression that already failed gets the internal `{error}` type, which is compatible with everything, so one mistake produces one error (`let y = x + 1;` with an unknown `x` reports only `x`).
* Warnings (do not stop compilation): `unreachable statement`, `unused variable` (names starting with `_` are exempt; parameters never warn), `this expression has no effect`.
* Errors carry hints where there is an obvious fix.

## What the checker does not decide

Backend limits (the 65536-commands-per-tick loop ceiling, dynamic list indexing, variable coordinates, comparing strings) are reported by `codegen`, not here (`docs/lang/codegen.md`). Names are checked case-sensitively; mangling to the IR's `[a-z0-9_]+` names (and avoiding collisions after lowercasing) happens in the HIR -> IR translation.
