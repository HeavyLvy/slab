# Plan: a standard library written in slab, dynamic selectors, enums, Result, match, run_command, equality, richer entities

Status: **draft for review, nothing implemented.** "(decided)" marks what you already chose.

## Goals

1. **The standard library is real slab source** embedded in the compiler (`std`, `game`, `game::tag`, `game::effect`): every function, type and method you can call is declared in a `.slab` file, so ctrl+click and hover show what it does. Anything that must emit a Minecraft command is a body-less `#[intrinsic("name")]` declaration; everything else is ordinary slab code on top. **(decided: all of std and game)**
2. `impl` blocks (methods, `self`, associated functions) for any struct or enum, user code included. **(decided)**
3. **Variables inside selectors**: `@e[tag=$tag_name]` uses the variable. Today `tag=tag_name` is the literal tag "tag_name". **(decided)**
4. Rust-style `enum`, generics on enums, `Option<T>`, `Result<T, E>`, `Some/None/Ok/Err`, and `match` **as a statement and as an expression**, plus `if let`. `Option`, `Result` and the four variants are **always in scope**. **(decided)**
5. World-acting builtins (`tp`, `kill`, `tag::add`, `effect::give`, `setblock`, `summon`, ...) return `Result<_, ()>` instead of `bool`; **`game::run_command(str)` returns `Result<int, ()>`** (Err = the command failed, Ok(n) = its integer result). **(decided)**
6. `==` / `!=` for `str`, enums, lists, structs, and an entity handle vs a one-entity selector. **(decided)**
7. Entities: `game::EntityId` (the handle) and `game::Entity` (a snapshot struct with name, tags, position, ...), written in slab. **(decided)**
8. Pay only for what you use: a result nobody looks at compiles to the plain command, with no `execute store`. **(decided)**
9. Out of scope: generic *function bodies*, the `?` operator, typed arguments for builtins (parked: modded ids must keep working).

## What it looks like

```slab
import std::*;      // print (Option, Result, Some, None, Ok, Err are already in scope)
import game::*;

enum Phase { Waiting, Running(int), Done(str) }

impl Phase {
    fn label(self) -> str {
        return match self {                 // match is an expression too
            Phase::Waiting => "waiting",
            Phase::Running(secs) => f"running, {secs}s left",
            Phase::Done(winner) => f"{winner} won",
        };
    }
}

#[load]
fn boot() {
    let tag_name: str = "this";
    let radius = 8;

    // 1. variables in selectors: $name, or ${expression}
    for e in @e[tag=$tag_name, distance=..${radius * 2}] {
        kill(e);                            // result ignored: the bare `kill` command, nothing else
    }

    // 2. commands return Result; check only when you care
    let cow = match summon("minecraft:cow") {        // Result<EntityId, ()>
        Ok(c) => c,
        Err(_) => return,
    };
    if tp(cow, 0, 70, 0).is_err() { print("could not move it"); }

    // 3. read the entity: live reads on the handle, or a snapshot struct
    match cow.name() {
        Ok(n) => print(f"it is called {n}"),
        Err(_) => print("it is gone"),
    }
    if let Ok(info) = cow.fetch() { print(f"{info.kind} at {info.pos.x} {info.pos.y} {info.pos.z}, tags {info.tags}"); }

    // 4. any command
    match run_command("data get entity @e[type=cow,limit=1] Health") {
        Ok(n) => print(f"the cow has {n} health"),
        Err(_) => print("no cow"),
    }
    run_command("gamerule keep_inventory true");    // ignored: just the command

    // 5. equality
    let name: str = "steve";
    if name == "steve" && cow == @e[type=cow, limit=1] { print("same cow"); }
}
```

Ctrl+click on `summon`, `tp`, `name` or `Result` opens the embedded source, e.g. in `game.slab`:

```slab
/// Teleports the targets. `Ok` when something moved, `Err` when nothing matched.
#[intrinsic("tp")]
pub fn tp(target: Target, x: Coord, y: Coord, z: Coord) -> Result<(), ()>;

impl EntityId {
    /// The entity's custom name as plain text; `Ok("")` when it has none, `Err` when it is gone.
    pub fn name(self) -> Result<str, ()> { return nbt_text(self, "CustomName"); }
}
```

## What a "prelude" is

A prelude is the small set of names a language puts in scope in **every file without an import**. Rust's makes `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `Vec`, `String` available everywhere. slab's is exactly six names: `Option`, `Some`, `None`, `Result`, `Ok`, `Err` (all declared in `std.slab`; everything else needs an import). A project can define its own item with one of those names; the local definition wins.

## The standard library in slab

- **Files**: `src/stdlib/std.slab`, `game.slab`, `game/tag.slab`, `game/effect.slab`, embedded with `include_str!` and loaded as ordinary modules named `std`, `game`, `game::tag`, `game::effect` into every project. The checker's special-case code for "builtin modules" (`import_builtin`, `ModRef::Builtin`, the `MODULES` table, hover and completion tables in the language server) is deleted: they are modules like any other. A full path (`game::say(..)`) works without an import, as today.
- **`#[intrinsic("name")]`** on a function **without a body** (`fn tp(..) -> R;`) is a primitive implemented by the compiler: it emits the typed command. Allowed only inside the embedded std modules. `builtins.rs` shrinks to the table of intrinsic names and their lowering; the signatures, docs (`///`, shown on hover) and types live in the `.slab` files.
- **Intrinsic-only types** usable in those signatures: `Selector`, `Coord`, `Target` (a selector or an `EntityId`), `any` (any storable value, for `print`), and `const str` / `const int` parameters (must be compile-time constants, e.g. `tag::has`).
- **`#[lang("entity_id")] pub struct EntityId;`** binds a declaration to a compiler type: the declared name is the entity handle (an int id on the scoreboard, as today), so selector-to-handle conversion, `as e { }`, `for e in @e` and every target argument keep working, and `impl EntityId { .. }` attaches methods to it.
- **Dead code**: only functions reachable from `#[load]`, `#[tick]`, `#[every]` and the user's code are emitted for the std modules, so an unused `fetch` adds nothing to the pack.
- **Language server**: the embedded sources are written (read-only) to a cache directory (`$XDG_CACHE_HOME/slab/std-<version>/`) so go-to-definition returns a real file; the grammar's list of builtin names comes from the std sources instead of a Rust table.

## Language design

### Syntax additions

| Item | Syntax |
|---|---|
| keywords | `enum`, `match`, `impl` (new in `Kw::ALL`, so the grammar generator picks them up) |
| tokens | `=>`, `\|`, `_` as a pattern, `$` inside selector arguments |
| unit type | `()` as a type and a value (`Result<(), ()>`, `Ok(())`); storable, unlike today |
| body-less fn | `#[intrinsic("x")] fn f(..) -> T;` (std only) |
| `impl` | `impl Name { fn m(self, a: T) -> R { .. }  fn make() -> Name { .. } }`; call `value.m(a)` or `Name::make()`; `self` is the receiver |
| enum | `enum Name<T, U> { Unit, Tuple(T, int), Fields { a: T } }`, optional `pub`, trailing commas |
| construct | `Name::Variant`, `Name::Variant(a, b)`, `Name::Variant { a: 1 }`; prelude and imported variants bare |
| `match` | statement or expression: `match expr { pattern => body, ... }`; `body` is a block or one expression |
| patterns | `_`, a binding, literals (`1`, `true`, `"s"`), `Variant`, `Variant(p, ..)`, `Variant { f, g: p }`, nested, or-patterns `A \| B`, guards `if cond` |
| `if let` | `if let Pattern = expr { } else { }`, sugar for a two-arm `match` |
| generics | type parameters on `enum` (and later `struct`) and on `impl<T>` headers. **No generic function bodies** (below) |
| selector variables | `@e[tag=$name]`, `@e[distance=..${r * 2}]`: `$ident` or `${expression}` in an argument value |

### Types and checking

- `Ty::Enum(EnumId, Vec<Ty>)`; `Option<int>` and `Option<str>` are different types. Payload types of a generic enum use `Ty::Param(i)` and are substituted at use.
- **Inference stays local**, as for lists: an expected type (annotation, parameter, return type, field) fills in the arguments. `Some(5)` is `Option<int>` alone; `None`, `Ok(5)` and `Err("x")` need context (`let x: Option<int> = None;`), otherwise "cannot infer the type of `None`, annotate it".
- `match` must be **exhaustive** (error naming the missing variants); unreachable arms are warnings. Standard pattern-matrix check, which also covers nested patterns.
- Arm bindings are immutable locals scoped to the arm. A `match` whose arms all return/break/continue diverges. As an expression every arm has the same type (a diverging arm such as `return` fits any type).
- **`impl`**: methods are functions with a first `self` parameter, named `<type>::<method>` internally. Method lookup is by the receiver's type: structs, enums, and lang types (`EntityId`). `impl<T> Option<T> { .. }` is allowed so std can declare generic methods; because a value of type `T` can be an int (scoreboard) or data (storage), **a generic impl may only contain `#[intrinsic]` methods** (std) in this round. User code can `impl` any non-generic type.
- **Option / Result methods** are intrinsics declared in `std.slab` (they need to fuse with a command outcome, see below):
  - `Option<T>`: `is_some()`, `is_none()`, `unwrap()`, `unwrap_or(d)`, `expect("msg")`
  - `Result<T, E>`: `is_ok()`, `is_err()`, `unwrap()`, `unwrap_or(d)`, `expect("msg")`, `ok() -> Option<T>`, `err() -> Option<E>`
- **`unwrap()` / `expect()` on `None` / `Err`** **(decided)**: prints `slab: unwrap on None at main.slab:12` (or the `expect` message) to chat and the function returns. The opt-in build flag `--abort-on-unwrap` also runs `datapack disable "file/<pack>"` (stops the pack; needs permission level 2, which functions have per `docs/mc/02-commands-core.md`, **to be verified from inside a function**, step 0; the pack id defaults to `file/<namespace>`, override with `--pack-id`; the disable reloads all packs, which is why it is opt-in).

### Builtins return `Result`

- `tp`, `kill`, `tag::add`, `tag::remove`, `effect::give`, `effect::clear`, `setblock` become `Result<(), ()>`; `summon` becomes `Result<EntityId, ()>` (the Ok payload is the new entity's handle: without it you cannot use what you summoned); `run_command` is `Result<int, ()>`. `game::exists(sel)` and `tag::has` stay `bool`: they are queries. `say` and `print` return nothing.
- Cost model (the "pay only for what you use" rule):
  1. **Ignored** (`tp(e, 0, 70, 0);`): the bare command, as today.
  2. **Consumed directly** (`.is_ok()`, `.is_err()`, `match`, `if let`, `.unwrap_or(..)`, `.unwrap()`): one `execute store success score ok [store result score v] run <cmd>` into score temporaries, and the test reads those scores. `if tp(..).is_ok()` costs what `if tp(..)` costs today.
  3. **Stored or passed on** (`let r = tp(..)`, returned): materialised as a `Result` compound.
- Migration: every `if tp(..) { }` / `if !kill(..)` in the examples, tests and e2e becomes `.is_ok()` / `.is_err()`; the old form is a compile error whose message says so.

### `run_command`

- `pub fn run_command(command: str) -> Result<int, ()>` is an intrinsic in `game.slab`. It lowers to a new typed `Command::Run(Arg<String>)` (the documented escape hatch `Command::Raw` becomes this): a literal is validated (single line, no leading `/`); computed text is a macro hole on the whole line.
- Fused form: `execute store success score ok store result score v run <cmd>`; `Ok(v)` when `ok` is 1. With computed text the macro helper does the two stores itself into fixed scratch holders, so we do not depend on how `function` reports success vs result (verified in step 0).

### Entities (written in slab)

- `game::EntityId`: the handle (`#[lang]`, an int id, cheap, copyable, comparable). Replaces today's `Entity` type name everywhere (`let boss: EntityId = ..`; examples, tests and docs migrate).
- `game::Entity`: a **snapshot struct** `{ id: EntityId, kind: str, name: str, tags: list<str>, pos: Vec3i }` plus `pub struct Vec3i { x: int, y: int, z: int }`. `id.fetch() -> Result<Entity, ()>` (also `game::get(id)`) builds it with ordinary slab code from the live reads below.
- Live single reads on the handle, all `Result<_, ()>` (the entity may be dead): `is_alive() -> bool`, `kind() -> Result<str, ()>`, `name()`, `tags() -> Result<list<str>, ()>`, `position() -> Result<Vec3i, ()>`.
- Their bodies are slab code over a few NBT-reading intrinsics (`nbt_int`, `nbt_str`, `nbt_str_list`, `nbt_text`: each copies `data get entity <handle selector> <path>` through a macro). `position` is whole numbers (`Pos[i]` read with scale 1 floors) until floats exist; then a float `Vec3` replaces it. `name` is the plain text of `CustomName` (`""` when absent; component styling is dropped).

### Dynamic selectors (`$name`, `${expr}`)

- Syntax: in a selector argument value, `$ident` or `${expression}` is replaced by the value at run time: `@e[tag=$t]`, `@e[type=$kind, limit=$n]`, `@e[scores={kills=$min..}]`, `@e[distance=..${r + 1}]`, `@e[tag=!$t]`. Parsed with the f-string hole machinery (`parse_embedded`), so errors point at the right place. A plain word keeps meaning itself (`tag=creeper`).
- Typed: `SelectorArg` values that can vary become `Arg<T>` / holes, the way `say` and `tag::add` already work: string-like keys (`tag`, `team`, `name`, `type`, `gamemode`) take a `str`; numeric keys (`limit`, `x y z`, `dx dy dz`, `distance`, `level`, `x_rotation`, `y_rotation`, score ranges) take an `int` (the ends of a range are separate holes). `nbt`, `predicate`, `sort`, `advancements` stay literal for now.
- Works wherever a selector is accepted (`for`, `as`/`at`, `kill`, `exists`, `tp`, `objective[holder]`, ...): the command becomes a macro call with the existing machinery (`docs/lang/codegen.md`). A selector with no `$` compiles exactly as today.
- Same caveat as string macros: a value that is not valid in its place makes that one command fail silently (macro lines that fail to parse do nothing, see `docs/mc/05-verified-by-slab.md`). A selector still cannot be stored in a variable.

### Equality (`==`, `!=`)

| Operands | How |
|---|---|
| `int`, `bool`, `EntityId` | as today (score compare) |
| handle vs selector, selector vs selector | the selector is converted to a handle first (a selector that can match several entities is the usual error), then the ids are compared |
| `str`, enums, `list<T>`, structs, anything else storable | **structural equality on NBT** (below) |

NBT equality relies on `data modify <t> set from <b>` **failing when nothing changes**: copy the left value to a scratch path, then `execute store success score tmp run data modify <scratch> set from <right>`; success = 1 means "different". A literal side can use `execute if data storage <s> {path:"lit"}` (one command). **Verify first** (step 0): strings, lists, compounds, that int `1` and byte `1b` differ, and that key order does not matter.

## Runtime representation

- An enum value is a **compound in command storage**: `{t: <variant index>, f0: .., f1: ..}` (tuple fields `f0, f1, ...`; struct-like variants use their field names). Variant indices follow declaration order (`Option`: `None = 0, Some = 1`; `Result`: `Ok = 0, Err = 1`). Unit variants are `{t:0}`; a `()` payload stores nothing.
- Payloads are NBT exactly like list and struct elements today (ints as int tags, bools 0/1), so enum **layout** needs no per-type code. `Kind::Data`; Data locals are dropped at function exit (`docs/lang/memory.md`).
- `match` lowering: evaluate the scrutinee once into a temporary, read its tag once (`execute store result score tag run data get storage ... t`), then an `if tag == k` chain; each arm starts with `let` bindings that read payload fields. As an expression, a result temporary is assigned in each arm. `return`, `break`, `continue` inside arms work through the existing `If` machinery.
- `const` / `global` initialisers may contain enum values (new `ConstVal::Enum`), rendered as one NBT literal.

## Pipeline changes

| Phase | Work |
|---|---|
| loader (`project.rs`) | add the embedded std modules to every project; their files get names under the cache dir |
| lexer/parser | tokens `=>`, `\|`, `_`; keywords; body-less `fn`; `impl`; `enum` with generics; `match` (statement and expression), `if let`; patterns; `()`; `$`/`${}` inside selector values; type arguments on user types |
| sema | intrinsic signatures and the intrinsic-only types; `#[lang]`; `impl` + method lookup + `self`; `EnumInfo`, `Ty::Enum`/`Ty::Param`, substitution; prelude; constructors; patterns, exhaustiveness; equality; selector holes in HIR (`ExprKind::Selector` carries hole expressions) |
| codegen | intrinsics dispatch (today's `builtin_command`, keyed by intrinsic name), enum layout, construct, `match` lowering, fused-command paths, `DataEq`, selector holes, reachability pruning of std functions |
| IR/lowering | `Expr::DataEq`, `Expr::CommandOutcome` (the fused form) |
| datapack | `SelectorArg` values as `Arg<T>`/holes, the `Run` command (+ Display/Check/unit test/zoo entry) |
| tooling | LSP: go-to-definition/hover/completion into the std files, enum/impl symbols, pattern tokens, `$` in selectors; regenerate the TextMate grammar (drift test); docs: `syntax.md`, `semantics.md`, `codegen.md`, `modules.md`, new `enums.md`, `stdlib.md`, `entities.md`, `docs/mc/05-verified-by-slab.md` |

## Order of work

Each step is a commit that passes `cargo test`, clippy and `tools/verify_all.sh`.

0. **Probe the real server** (no code): `data modify set from` fails on no change (strings, lists, compounds, int vs byte); two `store` clauses on one `execute`; `return run <cmd>` vs `function` success/result; `datapack disable "file/x"` from inside a function; `data get entity` for `Tags`, `CustomName`, `Pos` through a macro; macro selectors with `tag=$(h0)`, `..$(h0)`, `scores={x=$(h0)..}`. Record in `docs/mc/05-verified-by-slab.md`.
1. **Dynamic selectors** (your immediate bug; independent of everything else).
2. **Stdlib in slab, behaviour unchanged**: embedded std/game modules, body-less `#[intrinsic]` fns with today's signatures (`bool` returns still), `#[lang]`, dead-code pruning, the `builtins.rs` / checker / LSP special cases removed, go-to-definition into the cache files. The existing test-suite and e2e must pass untouched apart from import names.
3. **Equality**: `str`, lists, structs, handle vs selector.
4. **Enums without `match`**: parsing, `Ty::Enum`, generics, `()`, prelude, constructors, `Option` / `Result` declared in `std.slab`, const enums, `==`.
5. **`impl` blocks**: `self`, methods, associated functions, lang-type methods, generic `impl` headers for intrinsics; Option / Result methods (intrinsics).
6. **`match` + `if let`**, statement and expression, patterns and exhaustiveness.
7. **`run_command` and the `Result` builtins**: fused / materialised forms, migration of every `bool` use, `unwrap` failure behaviour.
8. **Entities in slab**: `EntityId` rename, `Entity` / `Vec3i`, NBT-reading intrinsics, `name()`/`tags()`/`kind()`/`position()`/`fetch()`.
9. Struct-like variants, LSP polish.

Rough size: 0 small; 1 small-medium; 2 large (a refactor, no new behaviour); 3 small-medium; 4 and 6 large; 5 medium-large; 7 medium-large (touches every builtin and test); 8 medium; 9 medium.

## Risks and limits

- **Inference is local**: `None`, `Ok(..)`, `Err(..)` often need an annotation.
- **No generic function bodies or closures**: no `map` / `and_then`, and no user helpers over `Option<T>`; only the intrinsic methods above. (Boxing generic values as data, or monomorphising, would lift this later.)
- **Entity reads are NBT reads**: positions are floored until floats exist; `name` loses styling; reading from a dead entity is an `Err`.
- **Breaking changes** for existing slab programs: `Entity` → `EntityId`, `if tp(..)` → `.is_ok()`. Both fail with a message that says what to write.
- `datapack disable` reloads all packs (`#load` runs again), hence opt-in.
- Computed selector/command text is pasted into macro lines: malformed text silently does nothing.
- Step 2 is the riskiest refactor (the compiler starts depending on its own embedded sources); it is first on purpose, with unchanged behaviour, so the rest builds on it.

## Decisions so far

1. Entities: `EntityId` handle + `Entity` snapshot + live reads, all in slab. 2. `unwrap`/`expect` failure: print and return; `--abort-on-unwrap` opts into `datapack disable`. 3. Selector variables: `$name` / `${expr}`. 4. `position()` returns whole numbers until floats exist. 5. `==` on lists and structs is enabled. 6. `run_command` returns `Result<int, ()>`. 7. All of std/game is slab source with `#[intrinsic]` declarations; `impl` blocks are a general feature.
