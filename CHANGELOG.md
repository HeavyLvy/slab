# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/) as described in [CONTRIBUTING.md](CONTRIBUTING.md#releases-and-versioning).

## [Unreleased]

- `slab test`: write `#[test]` functions in slab and run them on a real Minecraft server that slab sets up (a flat world, reset before every test). Tests can `wait` for ticks, use `assert`, `assert_eq` and `fail` from the new `test` module, and fail when the server logs an error. Needs Java 25+; the server is downloaded once from Mojang after you accept the EULA. `[test]` in `Slab.toml`, a Testing guide, and typed `fill`, `forceload` and `stop` commands.
- The VS Code extension is distributed only as `.vsix` files attached to each release; publishing to the VS Code Marketplace and Open VSX was dropped. The installation instructions explain installing a `.vsix`.

## [0.1.0] - 2026-10-25

The first public release.

- The compiler: `slab new`, `init`, `build` (with `--watch`, `--zip`), `check`, `fmt` and `parse`, for projects with a `Slab.toml` and code in `src/`. Targets Minecraft Java 26.3 (data pack format 121.0); 26.1 and 26.2 build with a warning.
- The language: `int`, `bool`, `str`, `list<T>`, structs, enums with `match` and `if let`, `Option` and `Result`, entities as values, `NearDouble` and `FarDouble` decimals, context blocks (`as`, `at`, `be`, `positioned`, `rotated`, ...), f-strings, modules, and entry points `#[load]`, `#[tick]` and `#[every(..)]`.
- `task fn` and `start` for loops that take many ticks, and warnings for code that would exceed the game's 65,536 commands per tick.
- Naming-convention warnings (`[lint] naming` in `Slab.toml`) and a formatter.
- A language server and a VS Code extension: diagnostics, hover, go to definition, completion, outline, semantic highlighting and formatting.
- Documentation as an mdBook: a guide, references and a generated standard library page.

[Unreleased]: https://github.com/HeavyLvy/slab/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/HeavyLvy/slab/releases/tag/v0.1.0
