# slab for VS Code

Syntax highlighting and language support for slab, the language that compiles to Minecraft
data packs (see `README.md` in the repository root).

## What it gives you

* **Instant highlighting** of keywords, strings, f-strings (with their `{holes}`), entity
  selectors (`@e[type=zombie]`), relative coordinates (`~`, `^2`), time literals (`10s`),
  attributes (`#[load]`), doc comments (`///`, `//!`) and more — no server needed.
* **Diagnostics in real time**: errors and warnings exactly where `slab check` reports them,
  across every module of the project.
* **Documentation** written as `///` comment runs above functions, globals, structs and
  their fields, `const`s, parameters and `let` bindings (and `//!` at the top of a file for
  the module), shown on hover — also at use sites, other files, and completion lists.
* **Every syntax error at once**, not just the first: the parser recovers at the next
  statement or item, and follow-on type errors inside a broken item are hidden.
* **Hover** with types, function signatures and builtin documentation, plus explanations of
  keywords, attributes (`#[every]`), time literals (`2s` = 40 ticks), selectors (`@a`),
  `~`/`^` coordinates, and the values of constants and globals.
* **Selector options**: inside `@e[`, completion lists the options (`type`, `distance`,
  `scores`, `sort`, ...) with their value forms, offers values for `sort`/`gamemode` and your
  objectives inside `scores={`; hovering an option or a bare `@e` explains them.
* **Signature help** while typing a call: the parameter under the cursor is highlighted.
* **Go to definition** for functions, globals, structs, fields, locals and imports —
  across module files.
* **Completions** that keep working while the code is half-typed: keywords; local
  variables and parameters with their types; builtins; struct fields and methods after `.`;
  module items after `a::`; enum variants and functions after `Type::`; types after `: `.
* **Outline** of every file: functions, structs, globals, consts, objectives.

## Setup

The Windows (x64), macOS (Apple silicon) and Linux (x64) versions of the extension contain the
language server, so there is nothing else to install for the editor features. To build and run
programs you also need the `slab` command: see
[Install](https://heavylvy.github.io/slab/guide/getting-started.html#install) (a one-line script, or
`cargo install slab-lang slab-lsp`).

On other platforms the extension looks for `slab-lsp` on your `PATH` and in `~/.cargo/bin`;
install it with `cargo install slab-lsp`, or set *slab: Server Path* (`slab.server.path`) to
an explicit path. In a folder you have not marked as trusted, only the bundled server and `PATH`
are used.

## Building the extension

```
cd editors/vscode
npm install
npm run vsix         # compile + package a .vsix + install it into VS Code
```

(`npm run package:target -- linux-x64` makes a platform package; it expects the server in
`server/`, as the release workflow puts it.) `npm run grammar` regenerates
`syntaxes/slab.tmLanguage.json` from the compiler's own definitions (so a grammar never drifts
from the language); the language server runs a test that fails if the committed grammar is stale.

## Notes

* A project is a `Slab.toml` with its code in `src/main.slab`; the server finds the `main.slab` by
  walking up from every opened file. A `.slab` file under no project gets checked by itself.
* Unsaved edits are checked with the in-buffer text.
* Only syntax errors show one at a time (the parser stops at its first); type errors are
  all reported together.
## License

Apache License 2.0, see `LICENSE` (the same license as slab and slab-lsp).
