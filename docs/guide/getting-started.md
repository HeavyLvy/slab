# Getting started

## What you need

* **Minecraft: Java Edition 26.3** (slab can also target 26.1 and 26.2). Bedrock Edition is not supported.
* **The `slab` program**, one small download. You do **not** need to know Rust or install it, and there is nothing else to set up.
* **A text editor.** Any will do; [Visual Studio Code](https://code.visualstudio.com/) with the slab extension is the best fit, because it shows errors as you type.

slab is used from a *terminal* (a window where you type commands): on Windows open **PowerShell** from the Start menu, on macOS open **Terminal** (in Applications → Utilities), on Linux open your terminal app.

## Install

The install command downloads the latest release and puts `slab` where your terminal can find it. Paste it into the terminal and press Enter.

**Windows** (PowerShell):

```text
irm https://raw.githubusercontent.com/HeavyLvy/slab/main/install.ps1 | iex
```

**macOS** (Apple silicon) and **Linux** (x86_64):

```text
curl -fsSL https://raw.githubusercontent.com/HeavyLvy/slab/main/install.sh | sh
```

Then **close the terminal and open a new one** (so it notices the new program) and check it worked:

```text
slab --version
```

It should print `slab 0.1.0` (or a newer version). That is all: continue with [your first project](#your-first-project).

The scripts are short and you can read them first ([install.sh](https://github.com/HeavyLvy/slab/blob/main/install.sh), [install.ps1](https://github.com/HeavyLvy/slab/blob/main/install.ps1)). They install two programs into a folder of your own (`~/.local/bin`, or `%LOCALAPPDATA%\slab\bin` on Windows), and change nothing else; no administrator rights are needed.

### Without a script

Download the archive for your system from the [releases page](https://github.com/HeavyLvy/slab/releases/latest) (`slab-<version>-windows-x86_64.zip`, `-macos-arm64.tar.gz` or `-linux-x86_64.tar.gz`) and unpack it. Inside are `slab` and `slab-lsp`. You can run `slab` from that folder (`./slab` or `.\slab.exe`), or move both files to a folder on your `PATH` to run it from anywhere.

### Other computers (Intel Macs, ARM Linux, ...)

There are no ready-made downloads for these yet, so slab has to be built from its source code, which needs the free Rust toolchain. Install Rust from [rustup.rs](https://rustup.rs) (it is one command or one installer), then:

```text
cargo install slab-lang slab-lsp
```

(`slab-lang` is the crate that installs the `slab` command.) Rust developers can also use `cargo binstall slab-lang slab-lsp` for the prebuilt files, or `cargo install --git https://github.com/HeavyLvy/slab slab-lang slab-lsp` for the newest, unreleased code.

### The editor extension

The extension gives VS Code highlighting, errors and warnings as you type, hover, completion, go to definition, an outline and formatting.

> **Note:** the extension is **not** on the VS Code Marketplace or on Open VSX. We could not publish it there because of difficulties working with the VS Code Marketplace and Open VSX. It is distributed as a `.vsix` file attached to every [GitHub release](https://github.com/HeavyLvy/slab/releases/latest), and installing one takes a minute.

**The install command does it for you.** If VS Code (or VSCodium) is installed, the command above has already installed the extension: reload any open VS Code window and open a `.slab` file. (Set `SLAB_NO_EXTENSION=1` to skip it.)

**Installing it by hand:**

1. Open the [latest release](https://github.com/HeavyLvy/slab/releases/latest) and, under *Assets*, download the `.vsix` for your system:

   | Your system | File |
   |---|---|
   | Windows (64-bit) | `slab-vscode-win32-x64-<version>.vsix` |
   | macOS (Apple silicon) | `slab-vscode-darwin-arm64-<version>.vsix` |
   | Linux (64-bit) | `slab-vscode-linux-x64-<version>.vsix` |
   | anything else | `slab-vscode-<version>.vsix` (it needs `slab-lsp` installed as above) |

2. Install it, either way:
   * **In VS Code:** open the Extensions view (Ctrl+Shift+X, or Cmd+Shift+X on macOS), click the **`...`** menu at the top of that panel, choose **Install from VSIX...**, and pick the file you downloaded.
   * **Or from a terminal:** `code --install-extension path/to/slab-vscode-....vsix` (use `codium` for VSCodium).
3. Reload the window if VS Code asks (*Developer: Reload Window* in the Command Palette), then open any `.slab` file.

To **update**, repeat this with a newer `.vsix` (installing over the old one replaces it); to **remove**, use the gear menu of the extension in the Extensions view, or `code --uninstall-extension HeavyLvy.slab-vscode`.

The platform packages already contain the language server (`slab-lsp`), so there is nothing more to do. The plain one, and the packages on other computers, look for `slab-lsp` in your `PATH` and in `~/.cargo/bin`, so install it as above, or set *slab: Server Path* in the settings.

### Removing slab

Delete the two programs from the folder above (or run `cargo uninstall slab-lang slab-lsp` if you used cargo) and uninstall the extension as above.

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

A data pack lives in a world's `datapacks` folder, inside the Minecraft folder: `%APPDATA%\.minecraft` on Windows, `~/Library/Application Support/minecraft` on macOS, `~/.minecraft` on Linux (a launcher such as Prism or CurseForge keeps each instance's folder somewhere of its own: look for "open folder" in its menu). Build straight into it:

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
