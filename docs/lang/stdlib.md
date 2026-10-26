# The standard library

`std` and `game` (with `game::tag` and `game::effect`) are written in slab. Their sources are
files in the compiler (`src/stdlib/std.slab`, `game.slab`, `game/tag.slab`, `game/effect.slab`),
embedded with `include_str!` and loaded into every project as ordinary modules (`src/stdlib.rs`,
`src/project.rs`). Hover or ctrl+click on `kill`, `tag::add`, `Entity`, ... in the editor opens
the declaration and its `///` documentation.

## Intrinsics

A function with no body marked `#[intrinsic("id")]` is implemented by the compiler:

```slab
/// Teleport the matching entities to the coordinates. `true` when it happened.
#[intrinsic("tp")]
pub fn tp(target: Target, x: Coord, y: Coord, z: Coord) -> bool;
```

The declaration *is* the signature the checker enforces. The second attribute argument is the
number of required parameters when the rest are optional (`#[intrinsic("summon", 1)]`).
Parameter types in an intrinsic are one of:

| Type | Accepts |
|---|---|
| `Target` | a selector (`@a[tag=x]`) or an `Entity` |
| `Coord` | an `int` expression or a relative coordinate (`~`, `^2`) |
| `any` | any storable value (for `print`) |
| `str`, `int` | an expression; a computed one reaches the command at run time (a macro) |
| `const str`, `const int` | a value known at compile time (it becomes part of a selector) |

The result is `bool` (did the command work), an `Entity` (`summon`), or nothing.

`id` must be one of the names in `sema::builtins::IDS`; code generation (`codegen.rs`,
`builtin_command` / `builtin_stmt`) matches on it and builds the typed commands. A test checks
that the sources and `IDS` list the same intrinsics. `#[intrinsic]` and `#[lang]` are only
accepted in these embedded modules.

## Option and Result

`std.slab` also declares `Option<T>` and `Result<T, E>` as ordinary enums (`docs/lang/enums.md`). The checker makes `Option`, `Result`, `Some`, `None`, `Ok` and `Err` visible in every module (the prelude); the variants of enums declared in the standard library are names of their own.

The methods of both are declared in `impl<T> Option<T>` / `impl<T, E> Result<T, E>` blocks of `std.slab` as body-less `#[intrinsic("option.unwrap")]` functions, with ordinary types (`fn unwrap_or(self, default: T) -> T;`): the receiver's type argument fills in `T`. The names it accepts are `sema::builtins::METHOD_IDS`; `codegen.rs` (`method_intrinsic`) implements them.

## Compiler types

`#[lang("entity")] pub struct Entity {}` in `game.slab` binds the name `Entity` to the type the
compiler provides for entity handles (an `int` id on the scoreboard, `docs/lang/entities.md`).
Selector conversion (`let me = @p;`), `for e in @e`, context blocks and every target argument
keep working; the declaration is where `game::Entity`, `import game::Entity;` and hover come from.

## Library code written in slab

Not everything in `game.slab` is an intrinsic. `Vec3i`, `NearVec3`, `FarVec3` and the methods of `Entity` (`position`, `is_alive`, `teleport_near`, ...) are plain slab, built on `run_command(text) -> Result<int, ()>`, which runs any command written as text (`docs/lang/compiled-output.md`, "Any command"). This is the rule for new library features: add an intrinsic only when the compiler has to (here `Entity::selector`, which needs the pack's namespace) and write the rest in slab, so ctrl+click shows what it does.

`std.slab` also has `NearDouble` and `FarDouble`, the fixed-point numbers of `docs/lang/doubles.md`, with the attribute `#[decimals(N)]` that lets the checker build them from decimal literals; `game.slab` reads and writes positions with them (`position_near`, `position_far`, `teleport_near`, `teleport_far`).

## How it is wired

- The loader adds the embedded modules after the project's own, so module 0 is still `main` and
  user modules keep their numbers; `Project::modules[i].std` marks them. Their source files are
  named after the cache directory (`stdlib::cache_dir()`, `$XDG_CACHE_HOME/slab/std-<version>-<hash>/`).
  The language server writes the files there (read-only) when it starts, so go-to-definition has a
  real file to open; the compiler itself never touches the disk for them.
- A full path (`game::say(..)`) works without an import: `std` and `game` are always known.
  Everything else is the normal module system: `import game::*;` brings in the functions, `Entity`
  and the submodules `tag` and `effect`; `import game::tag;` is the module; `import std::print;`
  is one item (also `as` another name).
- Nothing is in scope without an import, and a bare `print(..)` is an error whose hint names the
  path. `game.say(..)` is an error that says to use `::`.
- Only the intrinsics a program calls produce commands; the standard library adds nothing to the
  pack by existing.

## What comes next

Today every function of the library is an intrinsic. When `run_command` and `Result` exist
(`docs/lang/plan-enums-results.md`), library functions that do not need special code
generation can be written in slab on top of `run_command`; the intrinsics stay for what needs
the compiler's control (macro arguments, selectors, success flags, reading NBT into values).
`run_command` only reports success and the command's integer result, never text, so values
such as an entity's name or tags come from a few NBT-reading intrinsics instead.
