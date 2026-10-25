# Plan: dynamic selectors, enums, Option/Result, match, run_command, equality, richer entities

Status: **draft for review, nothing implemented.** "(decided)" marks what you already chose; the rest of the open points are at the end.

## Goals

1. **Variables inside selectors**: `@e[tag=$tag_name]` uses the variable. Today `tag=tag_name` is the literal tag "tag_name". **(decided: fix it)**
2. Rust-style `enum`, generics on enums, `Option<T>`, `Result<T, E>`, `Some/None/Ok/Err`, and `match` **as a statement and as an expression**, plus `if let`. **(decided)**
3. `Some / None / Ok / Err` (and the `Option` / `Result` types) are **always in scope**. **(decided)**
4. `game::run_command("...")` returns `Option<int>` (None = failed). **(decided)**
5. World-acting builtins (`tp`, `kill`, `tag::add`, `effect::give`, `setblock`, `summon`, ...) return `Result<_, ()>` instead of `bool`. **(decided)**
6. `==` / `!=` for `str`, enums (so `Option` / `Result`), and `Entity` vs a one-entity selector. **(decided)**
7. A real entity type with information (name, tags, position, ...). **(decided: wanted; shape is question 1)**
8. Pay only for what you use: a result nobody looks at compiles to the plain command, with no `execute store`. **(decided)**
9. Out of scope: generic functions, the `?` operator, typed arguments for builtins (parked, modded ids must keep working).

## What it looks like

```slab
import std::*;      // print (Option, Result, Some, None, Ok, Err are already in scope)
import game::*;

enum Phase { Waiting, Running(int), Done(str) }

fn describe(p: Phase) -> str {
    return match p {                       // match is an expression too
        Phase::Waiting => "waiting",
        Phase::Running(secs) => f"running, {secs}s left",
        Phase::Done(winner) => f"{winner} won",
    };
}

#[load]
fn boot() {
    let tag_name: str = "this";
    let radius = 8;

    // 1. variables in selectors: $name, or ${expression}
    for e in @e[tag=$tag_name, distance=..${radius * 2}] {
        kill(e);                            // result ignored: the bare `kill` command, nothing else
    }

    // 2. commands return Result<(), ()>; check only when you care
    let cow = match summon("minecraft:cow") {      // Result<Entity, ()>
        Ok(c) => c,
        Err(_) => return,
    };
    if tp(cow, 0, 70, 0).is_err() { print("could not move it"); }

    // 3. any command, with Option<int>
    match run_command("data get entity @e[type=cow,limit=1] Health") {
        Some(n) => print(f"the cow has {n} health"),
        None    => print("no cow"),
    }
    run_command("gamerule keep_inventory true");   // ignored: just the command

    // 4. equality
    let name: str = "steve";
    if name == "steve" && cow == @e[type=cow, limit=1] { print("same cow"); }
}
```

## What a "prelude" is

A prelude is the small set of names a language puts in scope in **every file without an import**. Rust's prelude makes `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `Vec`, `String` available everywhere; that is why you can write `Some(3)` without `use`. slab has had no prelude so far (even `print` needs `import std::print`). Following your answer, slab gets a **tiny prelude of exactly six names**: `Option`, `Some`, `None`, `Result`, `Ok`, `Err`. Everything else stays behind an import. A project can still define its own item with one of those names; the local definition wins.

## Language design

### Syntax additions

| Item | Syntax |
|---|---|
| keywords | `enum`, `match` (new in `Kw::ALL`, so the grammar generator picks them up) |
| tokens | `=>`, `\|`, `_` as a pattern, `$` inside selector arguments |
| unit type | `()` as a type and a value (`Result<(), ()>`, `Ok(())`); storable, unlike today |
| enum declaration | `enum Name<T, U> { Unit, Tuple(T, int), Fields { a: T } }`, optional `pub`, trailing commas |
| construct | `Name::Variant`, `Name::Variant(a, b)`, `Name::Variant { a: 1 }`; imported or prelude variants bare |
| `match` | statement or expression: `match expr { pattern => body, ... }`; `body` is a block or one expression |
| patterns | `_`, a binding, literals (`1`, `true`, `"s"`), `Variant`, `Variant(p, ..)`, `Variant { f, g: p }`, nested, or-patterns `A \| B`, guards `if cond` |
| `if let` | `if let Pattern = expr { } else { }`, sugar for a two-arm `match` |
| generics | type arguments on `enum` (and later `struct`) only. **No generic functions** |
| selector variables | `@e[tag=$name]`, `@e[distance=..${r * 2}]`: `$ident` or `${expression}` in an argument value |

### Types and checking

- `Ty::Enum(EnumId, Vec<Ty>)`; `Option<int>` and `Option<str>` are different types. Payload types of a generic enum use `Ty::Param(i)` and are substituted at use.
- **Inference stays local**, as for lists: an expected type (annotation, parameter, return type, field) fills in the arguments. `Some(5)` is `Option<int>` alone; `None`, `Ok(5)` and `Err("x")` need context (`let x: Option<int> = None;`), otherwise "cannot infer the type of `None`, annotate it".
- `match` must be **exhaustive** (error naming the missing variants); unreachable arms are warnings. Standard pattern-matrix check, which also covers nested patterns.
- Arm bindings are immutable locals scoped to the arm. A `match` whose arms all return/break/continue diverges, so a function can end in one without a trailing `return`.
- As an expression every arm must produce the same type (an arm that diverges, like `return`, fits any type).
- `Option`, `Result` and the variants are synthetic enum declarations inside the checker (no embedded `.slab` source). `Option::Some(1)` and `std::Option::Some(1)` also work.
- Built-in methods (no `impl` blocks and no closures in the language yet):
  - `Option<T>`: `is_some()`, `is_none()`, `unwrap()`, `unwrap_or(d)`, `expect("msg")`
  - `Result<T, E>`: `is_ok()`, `is_err()`, `unwrap()`, `unwrap_or(d)`, `expect("msg")`, `ok() -> Option<T>`, `err() -> Option<E>`
- **`unwrap()` / `expect()` on `None` / `Err`** **(decided)**: prints `slab: unwrap on None at main.slab:12` (or the `expect` message) to chat and the function returns. The opt-in build flag `--abort-on-unwrap` also runs `datapack disable "file/<pack>"` (stops the pack; it needs permission level 2, which functions have per `docs/mc/02-commands-core.md`, **to be verified from inside a function**, step 0; the pack id defaults to `file/<namespace>`, override with `--pack-id`; the disable reloads all packs, which is why it is opt-in).

### Builtins return `Result`

- Today `tp`, `kill`, `exists`-style action builtins return `bool`. They become `Result<(), ()>`; `summon` becomes `Result<Entity, ()>` (the Ok payload is the new entity's handle: without it there is no way to use what you summoned; tell me if you really meant `Result<(), ()>`). `game::exists(sel)` and `tag::has` stay `bool`: they are queries, not actions.
- Cost model (the "pay only for what you use" rule), shared with `run_command`:
  1. **Ignored** (`tp(e, 0, 70, 0);`): the bare command, as today.
  2. **Consumed directly** (`.is_ok()`, `.is_err()`, `match`, `if let`, `.unwrap_or(..)`): one `execute store success score ok run <cmd>` into a score temporary, and the test reads that score. This is exactly what `if tp(..)` costs today.
  3. **Stored or passed on** (`let r = tp(..)`, returned): materialised as a `Result` compound `{t:0}` / `{t:1}`.
- Migration: every `if tp(..) { }` / `if !kill(..)` in the examples, tests and e2e becomes `.is_ok()` / `.is_err()` (a mechanical rewrite, done in the same commit as the change).

### Entities

- Today `game::Entity` is a bare handle (an int id). Reading anything about an entity (name, tags, position) needs entity NBT through macros (`data get entity <selector> Tags`), so they are all `Result`-returning reads: the entity may be dead.
- Recommended shape: **`game::EntityId`** is the handle (today's `Entity`, renamed: cheap, copyable, comparable) and **`game::Entity`** is a **struct snapshot** `{ id: EntityId, kind: str, name: str, tags: list<str>, ... }` taken at one moment by `game::get(id) -> Result<Entity, ()>` (or `id.fetch()`). Cheap single reads exist directly on the handle: `id.tags()`, `id.name()`, `id.kind()`, `id.is_alive()`, each `Result<_, ()>`.
- Position needs floats (`Pos` is a list of doubles). Until floats land, `id.position()` returns whole numbers (`data get entity <sel> Pos[0] 1` floors) as a `Vec3i { x, y, z }` struct; it becomes a float `Vec3` with the floats feature. Say if you'd rather wait than ship the floored version.
- `name`: the plain text of `CustomName` (empty string if it has none); component styling is dropped.

### Dynamic selectors (`$name`, `${expr}`)

- Syntax: in a selector argument value, `$ident` or `${expression}` is replaced by the value at run time: `@e[tag=$t]`, `@e[type=$kind, limit=$n]`, `@e[scores={kills=$min..}]`, `@e[distance=..${r + 1}]`, `@e[tag=!$t]`. Parsed with the same machinery as f-string holes (`parse_embedded`), so errors point at the right place. A plain word keeps meaning itself (`tag=creeper`).
- Typed: `SelectorArg` values that can vary become `Arg<T>` / holes, the way `say` and `tag::add` already work: string-like keys (`tag`, `team`, `name`, `type`, `gamemode`) take a `str`; numeric keys (`limit`, `x y z`, `dx dy dz`, `distance`, `level`, `x_rotation`, `y_rotation`, score ranges) take an `int` (ends of a range are separate holes). Everything else (`nbt`, `predicate`, `sort`, `advancements`) stays literal for now.
- Works wherever a selector is already accepted (`for`, `as`/`at`, `kill`, `exists`, `tp`, `objective[holder]`, ...): the command becomes a macro call with the existing machinery (`docs/lang/codegen.md`). A selector with no `$` compiles exactly as today.
- Same caveat as string macros: a value that is not valid in its place (a tag with a space, a bad entity type) makes that one command fail silently (macro lines that fail to parse do nothing, see `docs/mc/05-verified-by-slab.md`).
- A selector still cannot be stored in a variable (it is a query); `let me = @p;` stores the entity, as before.

### Equality (`==`, `!=`)

| Operands | How |
|---|---|
| `int`, `bool`, `EntityId` | as today (score compare) |
| handle vs selector, selector vs selector | the selector is converted to a handle first (a selector that can match several entities is the usual error), then the ids are compared |
| `str`, enums, and any other storable value | **structural equality on NBT** (below) |
| `list<T>`, structs | free with the same mechanism; enabled |

NBT equality relies on `data modify <t> set from <b>` **failing when nothing changes**: copy the left value to a scratch path, then `execute store success score tmp run data modify <scratch> set from <right>`; success = 1 means "different". A literal side can use `execute if data storage <s> {path:"lit"}` (one command). **Verify first** (step 0): strings, lists, compounds, that int `1` and byte `1b` differ, and that key order does not matter.

## Runtime representation

- An enum value is a **compound in command storage**: `{t: <variant index>, f0: .., f1: ..}` (tuple fields `f0, f1, ...`; struct-like variants use their field names). Variant indices follow declaration order (`Option`: `None = 0, Some = 1`; `Result`: `Ok = 0, Err = 1`). Unit variants are `{t:0}`; a `()` payload stores nothing.
- Payloads are NBT exactly like list and struct elements today (ints as int tags, bools 0/1), so **generics need no code duplication**: codegen works from the layout. `Kind::Data`; Data locals are dropped at function exit (`docs/lang/memory.md`).
- `match` lowering: evaluate the scrutinee once into a temporary, read its tag once (`execute store result score tag run data get storage ... t`), then an `if tag == k` chain; each arm starts with `let` bindings that read payload fields. As an expression, a result temporary is assigned in each arm. `return`, `break`, `continue` inside arms work through the existing `If` machinery.
- `const` / `global` initialisers may contain enum values (new `ConstVal::Enum`), rendered as one NBT literal.
- `run_command` / `Result` builtins in fused form (shape 2 above) skip the compound entirely.

### `run_command`

- `Command::Raw` already exists as the documented escape hatch; `run_command` becomes a `Command::Run(Arg<String>)`-style variant (literal text validated: single line, no leading `/`; computed text is a macro hole on the whole line).
- Fused form for `Option<int>`: `execute store success score ok store result score v run <cmd>` (one command, two score temporaries). With computed text the macro helper does the two stores itself into fixed scratch holders, so we do not depend on how `function` reports success vs result (verify in step 0).

## Pipeline changes

| Phase | Work |
|---|---|
| lexer/parser | tokens `=>`, `\|`, `_`; keywords; `enum` with generics; `match` (statement and expression), `if let`; patterns; `()`; `$`/`${}` inside selector values; type arguments on user types |
| sema | `EnumInfo`, `Ty::Enum`/`Ty::Param`, substitution; prelude; constructors; patterns, exhaustiveness; methods; equality; builtin signatures to `Result`; selector holes in HIR (`ExprKind::Selector` carries hole expressions); entity types |
| codegen | enum layout, construct, `match` lowering (statement and value), fused-command paths, `DataEq`, selector holes |
| IR/lowering | `Expr::DataEq`, `Expr::CommandOutcome` (the fused form), nothing for `match` (lowered to `If` chains in codegen) |
| datapack | `SelectorArg` values as `Arg<T>`/holes, the `Run` command (+ Display/Check/unit test/zoo entry) |
| tooling | LSP: enum symbols, variant hover/completion, pattern tokens, `$` in selectors; regenerate the TextMate grammar (the drift test enforces it); docs: `syntax.md`, `semantics.md`, `codegen.md`, new `enums.md`, `entities.md`, `docs/mc/05-verified-by-slab.md` |

## Order of work

Each step is a commit that passes `cargo test`, clippy and `tools/verify_all.sh`.

0. **Probe the real server** (no code): `data modify set from` fails on no change (strings, lists, compounds, int vs byte); two `store` clauses on one `execute`; `return run <cmd>` vs `function` success/result; `datapack disable "file/x"` from inside a function; macro selectors with `tag=$(h0)`, ranges `..$(h0)`, `scores={x=$(h0)..}`. Record in `docs/mc/05-verified-by-slab.md`.
1. **Dynamic selectors** (your immediate bug; independent of everything else).
2. **Equality**: `str`, handle vs selector.
3. **Enums without `match`**: parsing, `Ty::Enum`, generics, `()`, the prelude, constructors, `Option` / `Result`, methods, `==`, const enums.
4. **`match` + `if let`**, statement and expression, patterns and exhaustiveness.
5. **`run_command` and the `Result` builtins**: fused / materialised forms, migration of every `bool` use, `unwrap` failure behaviour.
6. **Entities**: `EntityId` / `Entity`, snapshot, live reads, positions as whole numbers.
7. Struct-like variants, LSP polish.

Rough size: 0 small; 1 and 2 small-medium; 3 and 4 large (parser, checker, codegen); 5 medium-large (touches every builtin and every existing test); 6 medium; 7 medium.

## Risks and limits

- **Inference is local**: `None`, `Ok(..)`, `Err(..)` often need an annotation.
- **No generic functions, closures or `impl` blocks**: no `map` / `and_then`, and no user helpers over `Option<T>`.
- **Entity reads are NBT reads**: positions are floored until floats exist; `name` loses styling; reading a field from a dead entity is an `Err`.
- Changing the builtins from `bool` to `Result` is a **breaking change** for any existing slab program (like yours): `if tp(..)` stops compiling, with a message that says to use `.is_ok()`.
- `datapack disable` also reloads the world's data packs, so every pack's `#load` runs again; fine for a fatal error, bad for routine use, hence opt-in.
- Computed selector/command text is pasted into macro lines: malformed text silently does nothing.

## Decisions from the last round

1. Entities: `game::EntityId` (the handle, today's `Entity` renamed) + `game::Entity` (struct snapshot from `game::get(id)`) + live reads on the handle. **(decided)**
2. `unwrap()` / `expect()` failure: print and return; `--abort-on-unwrap` opts into `datapack disable`. **(decided)**
3. Selector variables: `$name` / `${expr}`. **(decided)**
4. `position()` returns whole numbers (`Vec3i`) until floats exist. **(decided)**
5. `==` on `list<T>` and structs: enabled (it falls out of the NBT mechanism). *(my default; say if you object)*

Nothing is left open. The next step is step 0 (probing the real server), then step 1 (dynamic selectors).
