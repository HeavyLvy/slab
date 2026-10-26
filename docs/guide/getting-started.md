# Getting started

## Install

There are two programs: `slab`, the compiler you run in a terminal, and `slab-lsp`, the language server that gives editors errors, hover and completion. Pick one way to get them.

**Linux and macOS** (x86_64 Linux, Apple silicon): download the latest release.

```text
curl -fsSL https://raw.githubusercontent.com/HeavyLvy/slab/main/install.sh | sh
```

**Windows** (PowerShell):

```text
irm https://raw.githubusercontent.com/HeavyLvy/slab/main/install.ps1 | iex
```

Both put `slab` and `slab-lsp` in a per-user folder (`~/.local/bin`, or `%LOCALAPPDATA%\slab\bin`) and tell you if it is not on your `PATH`. Set `SLAB_VERSION` to install an older version and `SLAB_INSTALL_DIR` to choose the folder. The scripts are short; read them first if you like.

**With Rust**, on any platform:

```text
cargo install slab-lang slab-lsp                   # builds from crates.io
cargo binstall slab-lang slab-lsp                  # or download the prebuilt release (needs cargo-binstall)
cargo install --git https://github.com/HeavyLvy/slab slab-lang slab-lsp   # the newest, unreleased code
```

(`slab-lang` is the crate that installs the `slab` command.) You can also download the archive for your platform from the [releases page](https://github.com/HeavyLvy/slab/releases) and unpack it anywhere on your `PATH`.

Check it works with `slab --version`.

### The editor extension

For VS Code, install the **slab** extension (`heavylvy.slab-vscode`): search for it in the Extensions view, or run `code --install-extension heavylvy.slab-vscode`. VSCodium and other editors that use [Open VSX](https://open-vsx.org/) find it there too. It gives highlighting, errors and warnings as you type, hover, completion, go to definition, an outline and formatting.

The version for your platform (Windows x64, macOS Apple silicon, Linux x64) already contains the language server. On other platforms it looks for `slab-lsp` in your `PATH` and in `~/.cargo/bin`, so install it as above, or set *slab: Server Path* in the settings. A `.vsix` file for each platform is also attached to every release (*Extensions: Install from VSIX...*).

To remove slab again: delete the two programs (or `cargo uninstall slab-lang slab-lsp`) and uninstall the extension.

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

The VS Code extension (see [the editor extension](#the-editor-extension)) understands `.slab` files: errors are shown as you type (all of them, not just the first), completion works inside half-typed code, and hover shows types, signatures and documentation. Ctrl+click on `kill`, `tag::add`, `Entity`, … opens the real declaration in the standard library. Inside a selector, `@e[` completes the options (`type`, `distance`, `scores`, …).

## Next

[The language](language.md) walks through values, control flow and functions.
