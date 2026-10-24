# slab

A small, statically typed language that compiles to a Minecraft Java **26.3** data pack.

```slab
// main.slab
import util;

global ticks: int = 0;

#[load]
fn boot() {
    print("hello from slab");
}

#[every(1s)]
fn heartbeat() {
    as @a[tag=ready] at @s {
        effect.give(@s, "minecraft:speed", 10s);
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

## Use

```
cargo build --release
slab build my_project            # -> my_project/build/<name>/ (a data pack folder)
slab build my_project --zip      # also writes <name>.zip
slab check my_project            # type-check only
```

Copy the folder (or zip) into `<world>/datapacks/` and `/reload`. To remove everything the pack created (scoreboard, storage, summoned entities, added tags), run `/function <name>:_uninstall` and then `/datapack disable` it.

* A project is a directory with a `main.slab` (the entry point). `import a::b;` loads `a/b.slab`; items are private unless `pub`.
* Variables work inside commands (`tp(@s, x, y, z)`, `xs[i]`, `effect.give(@s, "speed", secs * 20)`) - the compiler uses Minecraft's macro functions for you - and in text: `print(f"{name} has {n} points")`.
* `int`/`bool` are stored on the scoreboard, `str`, `list<T>` and structs in command storage. Recursion, `for`, `while`, `loop`, `break`, `continue` all work.
* `#[load]`, `#[tick]` and `#[every(N)]` register functions; `as`/`at`/`in`/`positioned`/... blocks map to `/execute`.
* Errors point at the exact file and line; the compiler checks the whole program before writing anything.

Limits worth knowing: no floats, no string concatenation (f-strings can only be printed), and a loop can run about 21,000 iterations per tick (Minecraft's command limit).

## Learn more

* `docs/lang/` - the language: syntax, semantics, modules, how it compiles.
* `examples/` - `slab/` single files, `projects/hello` a small multi-file project.
* `CLAUDE.md` - how the compiler is organised and how it is tested against a real server (`tools/verify_all.sh`).
