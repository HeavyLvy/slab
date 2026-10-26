# Contributing to slab

Thanks for looking. slab is a compiler written in Rust (no dependencies) from `.slab` source to a Minecraft Java data pack, plus a language server and a VS Code extension.

## Build and test

```text
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets      # kept at zero warnings
cargo run -- fmt --check examples src/stdlib # .slab files in the repo stay formatted
```

You need a recent stable Rust (1.88 or newer). Do **not** run `cargo fmt`: the Rust code is not rustfmt-formatted and it would rewrite nearly every file. Match the style of the code around you.

The unit and integration tests prove the output matches *our reading* of the Minecraft docs. `tools/verify_all.sh` also loads the generated packs into a real 26.3 dedicated server (needs Java 25; it downloads the official server jar into `target/mc/`, which is not committed). Run it after changing `src/lower.rs`, `src/codegen.rs` or anything in `src/datapack/`.

## How it is organised

```text
Slab.toml + src/main.slab + modules --project.rs--> parsed modules --syntax--> --sema--> HIR --codegen--> IR --lower--> datapack --> pack folder/zip
```

- `src/syntax/` lexer and parser, `src/sema/` names and types, `src/codegen.rs` HIR to IR, `src/lower.rs` IR to commands, `src/datapack/` typed Minecraft commands, JSON and file layout.
- **Only `src/datapack/` produces Minecraft syntax.** The rest builds typed values and never `format!`s a command.
- `src/stdlib/*.slab` is the standard library, written in slab. `docs/reference/std.md` is generated from its `///` comments (`UPDATE_DOCS=1 cargo test --test stdref`).
- `src/fmt.rs` is `slab fmt`, `src/lint.rs` the naming warnings, `crates/slab-lsp/` the language server, `editors/vscode/` the extension.
- `docs/` is an mdBook (`tools/docs.sh serve`). `docs/lang/` is the language design, `docs/mc/` notes on Minecraft 26.3. The notes are the reference for what the game accepts: Minecraft's command syntax changed a lot after 1.21, so check `docs/mc/` (and the real server) rather than writing commands from memory.

`CLAUDE.md` has the longer version of this, including which docs and tests a language feature touches.

## Changing the language

A feature usually touches: the grammar doc, parser, checker, `codegen` (and `lower` if it needs a runtime construct), the docs in `docs/lang/` and `docs/guide/`, and **a case in `examples/projects/e2e` with an expected value in its `checks.json`**, which the real-server run verifies. Adding a keyword, operator or builtin changes the editor grammar: run `cargo run -p slab-lsp -- grammar` (a test fails until you do).

If generated commands change, run `UPDATE_DOCS=1 cargo test --test compiled_output` and review the diff of `docs/lang/compiled-output.md`.

## Pull requests

Small, focused changes with tests are easiest to review. Say what you ran. By contributing you agree your work is licensed under the [Apache License 2.0](LICENSE).
