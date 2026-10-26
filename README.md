# slab

[![CI](https://github.com/HeavyLvy/slab/actions/workflows/ci.yml/badge.svg)](https://github.com/HeavyLvy/slab/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A small, statically typed language that compiles to a Minecraft Java **26.3** data pack. Write functions, structs, enums and loops; get a folder of `.mcfunction` files you drop into a world.

> **Status:** early (0.1). The compiler is checked against a real 26.3 dedicated server, but the language can still change. Issues and ideas are welcome.
>
> slab is an unofficial tool. It is not made by, approved by or associated with Mojang or Microsoft. "Minecraft" is a trademark of Mojang Synergies AB; this repository contains no game files.

```slab
// src/main.slab
import std::print;
import game::*;
import util;

global ticks: int = 0;

#[load]
fn boot() {
    print("hello from slab");
}

#[every(1s)]
fn heartbeat() {
    as @a[tag=ready] at @s {
        effect::give(@s, "minecraft:speed", 10s);
        tp(@s, ~, ~1, ~);
    }
    ticks += util::clamp(ticks, 0, 100);
    print(f"tick count is {ticks}");
}
```

```slab
// util.slab - any other .slab file is a module
pub fn clamp(x: int, lo: int, hi: int) -> int {
    if x < lo { return lo; }
    if x > hi { return hi; }
    return x;
}
```

## Documentation

Read it online at <https://heavylvy.github.io/slab/>. The documentation is a book in `docs/` (a guide, the command line and `Slab.toml` references, a standard library reference written from the library's own sources, how the compiler works, and notes on Minecraft 26.3):

```
cargo install mdbook --locked   # once (needs Rust; only for working on the docs)
tools/docs.sh serve             # read it at http://localhost:3000, updated as you edit
tools/docs.sh                   # build it into target/book (static HTML, host it anywhere)
```

`.github/workflows/docs.yml` publishes it with GitHub Pages (Settings -> Pages -> Source: GitHub Actions).

## Install

slab is one small download; **you do not need Rust, Cargo or any other tools**. Paste one line into a terminal (PowerShell on Windows, Terminal on macOS):

Linux (x86_64) and macOS (Apple silicon):

```
curl -fsSL https://raw.githubusercontent.com/HeavyLvy/slab/main/install.sh | sh
```

Windows (PowerShell):

```
irm https://raw.githubusercontent.com/HeavyLvy/slab/main/install.ps1 | iex
```

Then open a new terminal and run `slab --version`. Prefer not to run a script? Download and unpack the archive for your system from the [releases page](https://github.com/HeavyLvy/slab/releases). [Getting started](https://heavylvy.github.io/slab/guide/getting-started.html) walks through it step by step.

Only if there is no download for your computer (Intel Macs, ARM Linux), or if you are a Rust developer: `cargo install slab-lang slab-lsp` builds from crates.io (the crate `slab-lang` installs the `slab` command), `cargo binstall slab-lang slab-lsp` fetches the prebuilt files, and `cargo install --git https://github.com/HeavyLvy/slab slab-lang slab-lsp` gets the newest code.

For VS Code, install the **slab** extension (`code --install-extension heavylvy.slab-vscode`, or search the Extensions view; on VSCodium it comes from [Open VSX](https://open-vsx.org/)). It highlights `.slab` files immediately and, with the `slab-lsp` language server (bundled in the Windows, macOS and Linux versions), adds live errors and warnings from the real compiler, hover with types and builtin docs, go to definition, completion, an outline and formatting. The grammar is generated: `crates/slab-lsp` writes it from the compiler's own keyword/builtin lists, and a test fails if it goes stale.

Working on slab itself: `tools/dev-install.sh` builds this checkout and installs the compiler, the server and the extension (see [CONTRIBUTING.md](CONTRIBUTING.md)).

## Use

```
slab new my_project              # Slab.toml + src/main.slab
cd my_project
slab build                       # -> build/<name>/ (a data pack folder)
slab build --zip                 # also writes <name>.zip
slab build -w -o /path/to/world/datapacks/my_pack   # rebuild into the world each time a source file is saved, then `/reload`
slab check                       # type-check only
slab fmt                         # format the source (--check for CI)
```

Copy the folder (or zip) into `<world>/datapacks/` and `/reload`. To remove everything the pack created (scoreboard, storage, summoned entities, added tags), run `/function <name>:_uninstall` and then `/datapack disable` it.

* A project is a directory with a `Slab.toml` (name, namespace, description, build options) and its code in `src/`; `src/main.slab` is the entry point. `import a::b;` loads `src/a/b.slab`; items are private unless `pub`. `slab new NAME` creates one, `slab init` turns an existing flat folder into one.
* Variables work inside commands (`tp(@s, x, y, z)`, `xs[i]`, `effect::give(@s, "speed", secs * 20)`) - the compiler uses Minecraft's macro functions for you - and in text: `print(f"{name} has {n} points")`.
* `int`/`bool` are stored on the scoreboard, `str`, `list<T>` and structs in command storage. Recursion, `for`, `while`, `loop`, `break`, `continue` all work.
* Variables can hold entities (`let boss: Entity = summon(..)`, `let me = @p`) and act on them (`tp(boss, ..)`, `as boss { }`, `points[boss] += 1`); commands that act on the world return whether they worked, so `if !tp(boss, 0, 70, 0) { .. }` catches a missing target.
* `#[load]`, `#[tick]` and `#[every(N)]` register functions; `as`/`at`/`in`/`positioned`/... blocks map to `/execute`.
* Errors point at the exact file and line; the compiler checks the whole program before writing anything.

* `str` values can be joined with `+` and used in commands; `NearDouble`/`FarDouble` give decimal positions; `match`, `Option`/`Result` and enums are there; `task fn` runs long loops over many ticks.
* `slab fmt` formats code, and names that break the naming conventions are warned about (see `docs/reference/style.md`).

Limits worth knowing: no general `float` type yet, and one function call chain may run at most 65,536 commands per tick (Minecraft's limit; the compiler warns, and `task fn` spreads the work over ticks).

## Learn more

* `docs/lang/` - the language: syntax, semantics, modules, how it compiles.
* `docs/lang/compiled-output.md` - what each piece of slab code turns into, command by command (checked against the compiler by a test).
* `examples/` - `slab/` single files, `projects/hello` a small multi-file project.
* [CONTRIBUTING.md](CONTRIBUTING.md) - building, testing, and how the compiler is organised.
* `CLAUDE.md` - the same notes in the form the project's AI coding assistant reads.

## License

slab (the compiler, the standard library in `src/stdlib/`, the language server `slab-lsp` and the
VS Code extension) is licensed under the [Apache License 2.0](LICENSE). Copyright 2026 HeavyLvy;
see also [NOTICE](NOTICE). Programs you write in slab and the data packs the compiler produces
from them are yours.
