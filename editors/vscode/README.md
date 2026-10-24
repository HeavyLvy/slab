# slab for VS Code

Syntax highlighting and language support for slab, the language that compiles to Minecraft
data packs (see `README.md` in the repository root).

## What it gives you

* **Instant highlighting** of keywords, strings, f-strings (with their `{holes}`), entity
  selectors (`@e[type=zombie]`), relative coordinates (`~`, `^2`), time literals (`10s`),
  attributes (`#[load]`) and more — no server needed.
* **Diagnostics in real time**: errors and warnings exactly where `slab check` reports them,
  across every module of the project.
* **Hover** with types, function signatures and builtin documentation.
* **Go to definition** for functions, globals, structs, fields, locals and imports —
  across module files.
* **Completions**: keywords; builtins with argument snippets; struct fields and
  list/string methods after `.`; module items after `import a::`; types after `: `.
* **Outline** of every file: functions, structs, globals, consts, objectives.

## Setup

The extension needs the `slab-lsp` binary from this repository:

```
cargo build --release -p slab-lsp
```

The server is found automatically at `<workspace>/target/release/slab-lsp`, or set
*slab: Server Path* (`slab.server.path`) to an explicit path (or put `slab-lsp` on PATH).

## Building the extension

```
cd editors/vscode
npm install
npm run install      # compile + package a .vsix + install it into VS Code
```

`npm run grammar` regenerates `syntaxes/slab.tmLanguage.json` from the compiler's own
definitions (so a grammar never drifts from the language); the language server runs a test
that fails if the committed grammar is stale.

## Notes

* A project is a directory with a `main.slab`; the server finds it by walking up from every
  opened file. A `.slab` file under no project gets checked by itself.
* Unsaved edits are checked with the in-buffer text.
* Only syntax errors show one at a time (the parser stops at its first); type errors are
  all reported together.