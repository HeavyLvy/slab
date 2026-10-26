# Contributing to slab

Thanks for looking. slab is a compiler written in Rust (no dependencies) from `.slab` source to a Minecraft Java data pack, plus a language server and a VS Code extension.

## Build and test

```text
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets      # kept at zero warnings
cargo run -- fmt --check examples src/stdlib # .slab files in the repo stay formatted
```

`tools/dev-install.sh` installs this checkout (compiler, language server and VS Code extension) on your machine. You need a recent stable Rust (1.88 or newer). Do **not** run `cargo fmt`: the Rust code is not rustfmt-formatted and it would rewrite nearly every file. Match the style of the code around you.

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

## Releases and versioning

Versions follow [Semantic Versioning](https://semver.org/): `MAJOR.MINOR.PATCH`, tagged `vMAJOR.MINOR.PATCH` (a pre-release is `v0.2.0-rc.1`). What counts as the public interface: the language (syntax and meaning of `.slab` code), the `slab` command line and `Slab.toml`, and the standard library. The commands in a generated pack are not, but a pack must keep working on the Minecraft versions it targets.

- While the major version is 0, a **minor** bump may break the language, the CLI or `Slab.toml`, and a **patch** only fixes bugs. From 1.0 on, breaking changes need a major bump.
- Supporting a new Minecraft version (a new default `minecraft` target) is a minor bump.
- `slab`, `slab-lsp` and the VS Code extension share one version. It is written in `Cargo.toml`, `crates/slab-lsp/Cargo.toml` and `editors/vscode/package.json` (and `Cargo.lock`), and `tests/version.rs` fails when they differ.
- Every user-visible change gets a line under `## [Unreleased]` in `CHANGELOG.md`.

To release: on `main`, make a commit "Release X.Y.Z" that sets the version in those files, runs `cargo build` to update `Cargo.lock`, and renames `[Unreleased]` to `[X.Y.Z] - date` in the changelog (with a new empty `[Unreleased]` above it and the link lines at the bottom updated). Once CI is green, `git tag -a vX.Y.Z -m "slab X.Y.Z" && git push origin vX.Y.Z`. The `release` workflow checks that the tag matches the files and is on `main`, builds `slab` and `slab-lsp` for Linux, macOS (Apple silicon) and Windows plus the extension (`.vsix`), and publishes a GitHub release with the changelog section as its notes. Version tags are never moved or deleted; a bad release is fixed by a new patch version.

A release can publish more than the GitHub release page (the extension is not on any store yet, and the first release will tell you whether the others work), each part only when its token is set as a repository secret (Settings -> Secrets and variables -> Actions, or `gh secret set NAME`):

| Secret | Publishes | Where the token comes from |
|---|---|---|
| `VSCE_PAT` | the VS Code extension to the Marketplace | an Azure DevOps personal access token with the *Marketplace: Manage* scope for all accessible organizations, for the publisher `HeavyLvy` created at <https://marketplace.visualstudio.com/manage> |
| `OVSX_PAT` | the same packages to Open VSX | an access token from <https://open-vsx.org/user-settings/tokens> (sign in with GitHub, sign the publisher agreement, then `npx ovsx create-namespace HeavyLvy -p <token>` once) |
| `CARGO_REGISTRY_TOKEN` | `slab-lang` and `slab-lsp` to crates.io | a token from <https://crates.io/settings/tokens> with the *publish-new* and *publish-update* scopes |

The extension is published as one package per platform with the language server inside (Linux x64, macOS arm64, Windows x64) plus a universal one that finds `slab-lsp` on `PATH`. To publish a release's extension again, run the `publish-extension` workflow by hand with the tag.

`main` is protected: it takes no force pushes, and CI (`rust` and `extension`) has to pass on pull requests.

## Pull requests

Small, focused changes with tests are easiest to review. Say what you ran. By contributing you agree your work is licensed under the [Apache License 2.0](LICENSE).
