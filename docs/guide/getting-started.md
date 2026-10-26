# Getting started

## Install

slab is built with Rust. From a clone of the repository:

```text
./install.sh
```

This installs three things: the `slab` command (`cargo install --path .`), the `slab-lsp` language server, and the VS Code extension. It needs a Rust toolchain and, for the extension, Node.js and the `code` command. To install only the compiler:

```text
cargo install --path . --locked
```

Check it works with `slab --help`.

## Your first project

```text
slab new my_pack
cd my_pack
```

This creates

```text
my_pack/
  Slab.toml        # the project: name, Minecraft version, options
  src/
    main.slab      # where the program starts
  .gitignore       # ignores build/
```

and `src/main.slab` is already a working program:

```slab
import std::print;

#[load]
fn hello() {
    print("hello from my_pack");
}
```

`#[load]` marks a function that runs when the pack loads (and again on every `/reload`). `print` shows a message in chat.

## Build it

```text
slab build
```

writes a data pack folder to `build/my_pack/`. The name `my_pack` is the pack's *namespace*: it is how you run its functions in game (`/function my_pack:hello`) and the prefix of everything it creates.

## Put it in a world

A data pack lives in a world's `datapacks` folder. Build straight into it:

```text
slab build -o "<minecraft folder>/saves/<world>/datapacks/my_pack"
```

Then, in the game, `/reload` (or open the world). You will see `hello from my_pack` in chat.

While you work, let slab rebuild for you:

```text
slab build --watch -o "<minecraft folder>/saves/<world>/datapacks/my_pack"
```

Every time you save a `.slab` file (or `Slab.toml`) it builds again and tells you how long it took; a mistake is shown and watching goes on. Press `/reload` in the game to load each new version.

## Removing a pack cleanly

A pack creates scoreboard objectives, storage, and sometimes entities and tags. To remove all of it:

```text
/function my_pack:_uninstall
/datapack disable "file/my_pack"
```

## The editor

The VS Code extension (installed by `install.sh`) understands `.slab` files: errors are shown as you type (all of them, not just the first), completion works inside half-typed code, and hover shows types, signatures and documentation. Ctrl+click on `kill`, `tag::add`, `Entity`, … opens the real declaration in the standard library. Inside a selector, `@e[` completes the options (`type`, `distance`, `scores`, …).

## Next

[The language](language.md) walks through values, control flow and functions.
