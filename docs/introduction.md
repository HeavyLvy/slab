# slab

**slab** is a small, statically typed programming language that compiles to a Minecraft Java Edition **data pack**. You write code in a familiar, braces-and-types style (it reads a bit like Rust or TypeScript, but you need to know neither), and `slab build` turns it into the `.mcfunction` files, scoreboards and storage the game runs.

```slab
import std::print;
import game::*;

global ticks: int = 0;

#[load]
fn boot() {
    print("hello from slab");
}

#[tick]
fn every_tick() {
    ticks += 1;
    as @a[tag=ready] at @s {                   // for every player with the tag `ready`, where they stand
        effect::give(@s, "minecraft:speed", 10s);
        tp(@s, ~, ~1, ~);
    }
}
```

## What you get

* **Types.** `int`, `bool`, `str`, `list<T>`, your own `struct`s and `enum`s, `Option` and `Result`, and `match`. Mistakes are found when you build, with the file and line, not when a command fails in game.
* **Variables inside commands.** `tp(@s, x, y, z)`, `@e[distance=..${radius * 2}]`, `print(f"{name} has {points} points")`: the compiler uses Minecraft's macro functions for you.
* **Entities as values.** `let boss = summon("minecraft:zombie", 0, 70, 0);` keeps a handle you can store, pass around, move, ask for its position and rotation, and use as `as boss { ... }`.
* **Long jobs.** A `task fn` may take many ticks: its loops run a slice per tick, so a 360-marker ring does not run into the game's limit of 65,536 commands per tick.
* **Real tooling.** A language server and VS Code extension (errors as you type, completion, hover, go to definition), `slab build --watch`, and a standard library you can read, written in slab.
* **No leftovers.** `/function <name>:_uninstall` removes everything the pack created: scoreboards, storage, summoned entities and tags.

## How it works

```text
src/main.slab + modules  →  parse  →  check types  →  typed IR  →  commands  →  data pack folder (or zip)
```

Numbers (`int`, `bool`) live on the scoreboard; strings, lists, structs and enums live in command storage. Locals are fixed slots (so recursion saves them on a stack), and a loop is a function that calls itself. The [compiler internals](lang/codegen.md) pages show exactly which commands each piece of code becomes, generated from the real compiler, so you never have to guess.

## Where to start

* New here? [**Getting started**](guide/getting-started.md): install (one command, no Rust or other tools needed), make a project, load it into a world.
* Learning the language? [**The language**](guide/language.md), then [**Structs, enums and match**](guide/data.md), then [**Talking to Minecraft**](guide/minecraft.md).
* Looking something up? The [**command line**](reference/cli.md), [**`Slab.toml`**](reference/slab-toml.md) and the [**standard library**](reference/std.md) references.
* Curious how it works? The **Compiler internals** section, and the **Minecraft notes** with what was learned by running the output on a real server.

slab targets **Minecraft Java 26.3** (data pack format 121.0); everything it generates has been run on a real 26.3 server.
