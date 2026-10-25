# Projects and modules

A slab project is a directory with a `main.slab` at its root. `main.slab` is the entry module (like `main.rs` or `main.c`). Every other `.slab` file is a **module** that is loaded when something imports it. Implemented in `src/project.rs` (loading) and `src/sema/check.rs` (scopes, visibility).

```
my_game/
  main.slab          # root module, always "main"
  util.slab          # module `util`
  game/
    arena.slab       # module `maps::arena`
```

`slab build my_game` compiles the whole project to one data pack.

## Importing

```slab
import util;                    // the module, used as util::clamp(x)
import util as u;               // renamed:      u::clamp(x)
import maps::arena;            // maps/arena.slab, bound as `arena`: arena::reset()
import util::{clamp, Point};    // items brought in unqualified: clamp(x), Point { .. }
import util::*;                 // every pub item of util, unqualified (an explicit import or own item wins)
```

* `import a::b;` always means the file `a/b.slab` **relative to the directory containing `main.slab`**, never relative to the importing file. There is no `super`/`self`, so a path means the same thing everywhere.
* The module name bound by `import a::b;` is its last segment (`b`) unless renamed with `as`. A braced list imports items instead and binds no module name.
* Imports are top-level only, and a file's imports are private to it (no re-exporting; `pub import` is an error).
* `main.slab` cannot be imported. A module cannot import itself.
* **Import cycles are fine** (`a` imports `b`, `b` imports `a`): every module's items are collected before any name is resolved.
* Each file is loaded once, in order of first import. Files that exist but are never imported are ignored.

## Visibility

Items are private to their file unless declared `pub`: `pub fn`, `pub struct`, `pub const`, `pub global`, `pub objective`. Using a private item from another module is an error pointing at the use (`` `secret` is private to module `util` ``). Struct fields are all public once the struct is. Functions with `#[load]`/`#[tick]`/`#[every]` run regardless of `pub`; the attribute is what registers them.

## Names

* Qualified names to a **project** module are exactly two segments: `module::item`, in expressions (`util::clamp(1, 2, 3)`, `util::LIMIT`, `util::counter += 1`, `util::points[@s] += 1`), in struct literals (`util::Point { x: 1, y: 2 }`) and in types (`list<util::Point>`).
* A name defined in a module shadows nothing from other modules: two modules may both define `helper`. Importing an item whose name the module already uses is an error.
* Constants are evaluated in the module that declares them (a private constant can feed a `pub const`).
* A struct type is identified by its declaration, not its name: `util::Point` and a local `Point` are different types.

## Built-in modules

Two modules are part of the language and need no file of your own (their source is embedded in the compiler): **`std`** (`print`) and **`game`** (everything that touches Minecraft). `game` has the submodules `game::tag` and `game::effect` and the type `game::EntityId`. They follow the same rules as any module, with two differences: a full path always works without an import (`game::say("hi")`, `game::tag::add(e, "x")`, `let e: game::EntityId = game::summon("minecraft:cow");`), and a path may have more than two segments. `import a::b::c;` also imports one item (`c`) when `a::b` is a module and `c` is in it, for the standard library and for your own modules.

```slab
import std::print;            // one item, usable as print(..)
import game::*;               // everything game has: say, kill, tp, summon, EntityId, and the modules tag and effect
import game::tag;             // the submodule, used as tag::add(..)
import game::summon as spawn; // renamed
import game::tag::{add, has}; // items of a submodule
```

* Nothing is in scope without an import (there is no prelude): a bare `print(..)` is an error whose hint names the path. Calls use `::`, never `.`; `game.say(..)` is an error that says so. `.` is for fields and methods of values.
* The names of built-in modules and their items are ordinary names: a project can define its own `print`, and a glob import never overrides a definition or an explicit import.
* Project modules called `std` or `game` (the files `std.slab`, `game.slab`, `std/…`, `game/…`) are never loaded for such imports; pick another name (the examples use `maps::arena`).
* Where they live: they are `.slab` files embedded in the compiler (`src/stdlib/`, `docs/lang/stdlib.md`); functions without a body marked `#[intrinsic]` are implemented in `src/codegen.rs`.

## What the pack looks like

All modules end up in one namespace (the pack namespace, by default the project directory's name, or `--namespace`). Item names are flattened: `maps::arena::start` becomes the function `<ns>:maps__arena__start`; the root module's items have no prefix (`<ns>:boot`). Names are lowercased and collisions get a numeric suffix (`helper`, `helper_2`). Objectives declared in the root keep their exact name (`kills`); objectives from other modules are prefixed with the module (`maps_arena_points`).
