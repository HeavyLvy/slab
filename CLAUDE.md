# slab

A statically typed programming language (written in Rust) that compiles a project of `.slab` files to a Minecraft Java Edition **data pack**. `slab build my_project` -> a pack folder (and optionally a zip).

- **Target version: Minecraft Java 26.3** ("Wilderness Bound", released 2026-09-15). Data pack format **121.0**, resource pack format 97.1.
- Minecraft now uses year-based versions (26.1, 26.2, 26.3). Training-data knowledge of commands and data pack JSON is **outdated** (it mostly stops around 1.21.x), so do not write commands or data pack JSON from memory.

## Check the docs often

**Before writing or changing anything that emits Minecraft commands, JSON, NBT/SNBT, `pack.mcmeta`, or file layout, read the relevant file in [docs/mc/](docs/mc/README.md).** Re-check them while developing, not only at the start: syntax changed a lot between 1.21.4 and 26.3.

| Need | File |
|---|---|
| What changed since 1.21.4, cheat sheet of gotchas | `docs/mc/00-version-deltas-1.21.4-to-26.3.md` |
| pack.mcmeta, directory layout, functions, macros, load/tick tags | `docs/mc/01-datapack-structure.md` |
| execute, scoreboard, data, function, return, schedule, gamerule, selectors, arg types | `docs/mc/02-commands-core.md` |
| give, summon, setblock, fill, item, loot, tp, effect, attribute, etc. | `docs/mc/03-commands-world-entity.md` |
| SNBT, text components, predicates, components, advancements, loot, recipes, dialogs, entity NBT | `docs/mc/04-data-formats.md` |
| What the real 26.3 server accepted/rejected from slab's output (highest authority) | `docs/mc/05-verified-by-slab.md` |

Rules for the docs:
- If a doc says `**UNVERIFIED**` or doesn't cover something, verify against minecraft.wiki / misode.github.io/changelog (or ideally by running a real 26.3 server) before relying on it, then **update the doc** with the verified answer and source.
- When you learn something new about 26.3 behaviour (from a test run, error message, or source), add it to the docs in the same change (`docs/mc/05-verified-by-slab.md` for things found by running slab output).
- `docs/mc/` is reference material about Minecraft; the language's own design is in `docs/lang/` (below). Keep the doc, the code and the tests of a feature in sync.

## The pipeline

```text
Slab.toml + src/main.slab + modules --project.rs--> parsed modules --syntax--> --sema--> HIR --codegen--> IR --lower--> datapack --> pack folder/zip
                      (load)         (lexer, parser)        (types)      (typed)  (HIR->IR)  (IR->commands)  (typed commands)
```

| Phase | Code | Design doc |
|---|---|---|
| load files, follow `import`; add the embedded standard library | `src/project.rs`, `src/stdlib.rs`, `src/stdlib/*.slab` | `docs/lang/modules.md`, `docs/lang/stdlib.md` |
| lex + parse (first error only) | `src/syntax/`, `src/diag.rs` | `docs/lang/syntax.md` |
| name resolution, types, flow checks -> HIR | `src/sema/` | `docs/lang/semantics.md`, `modules.md` |
| HIR -> IR (break/continue, loops, structs, builtins, modules -> flat names) | `src/codegen.rs` | `docs/lang/codegen.md` |
| IR -> typed commands (scoreboard/storage runtime model, recursion stack, scope cleanup, `_uninstall`) | `src/ir.rs`, `src/lower.rs` | `docs/lang/ir.md`, `docs/lang/memory.md` |
| typed Minecraft commands/JSON, validation, output | `src/datapack/` | `docs/mc/` |
| orchestration + CLI | `src/compile.rs`, `src/main.rs` | |

Language decisions: Rust-like syntax; static types with local inference; Minecraft access through context blocks (`as @a at @s { }` = `/execute`) plus builtin functions; entry points are attributes (`#[load]`, `#[tick]`, `#[every(1s)]`); `main.slab` is the root, other files are modules (`import a::b;` = `a/b.slab`, `pub` exports). Runtime model: `int`/`bool` on the scoreboard, `str`/`list`/structs in command storage. Variables in commands: numbers become macro **holes** (`$(h0)`) filled through `<ns>:macro` and a generated helper function; strings are macro holes too (`+`, f-strings as `str` values; a `Concat` is one macro with a quoted template), and `print(f"..")` uses `tellraw` components - see `docs/lang/codegen.md`. Memory: values are copied (no references, so no lifetimes/borrow checker); Data locals are dropped at function exit and `<ns>:_uninstall` removes everything the pack created (`docs/lang/memory.md`).

## Architecture rule: only `src/datapack` produces Minecraft syntax

The frontend, `codegen` and `lower` must never `format!` a command, SNBT, JSON or resource path. They build the typed values in `src/datapack` (`Command`, `Execute`, `Compound`, `Text`, `Function`, `DataPack`, ...), which render and validate themselves. If a command isn't modelled, add a typed variant (with `Display` + `Check` + a unit test, and an entry in `examples/zoo.rs`) rather than using `Command::Raw` (escape hatch only).

- `names.rs` validated newtypes (`ResourceLocation`, `Word` for objective/tag/team names, `HolderName`); `args.rs` coordinates, ranges, selectors, score holders; `nbt.rs` SNBT + paths; `text.rs` text components; `command.rs` the command AST, `Execute` builder, `MacroCommand`; `function.rs`; `pack.rs` `DataPack` (functions, tags, JSON resources, `validate()`, `write_dir`, `write_zip`); `zip.rs` a stored-method zip writer.
- `Check::check()` catches what types can't (mixed `~`/`^`, a negative `players add`, an `execute` chain not ending in `run`). `DataPack::validate()` checks cross-references.
- Not modelled yet: `give`, `fill`, `clone`, `title`, `bossbar`, `team`, `attribute`, `item`, `loot`, `random`, `tick`, `/compute`, `execute if items|slots|stopwatch`, selector `advancements=`/`nbt=`, typed macro arguments (macros are assembled from text pieces).

## Working on the compiler

- `cargo test` (unit + integration), `cargo clippy --all-targets` (kept at zero warnings).
- `slab new NAME`, `slab init`, `slab build [PATH] [-o DIR] [--zip] [--namespace NS]`, `slab check [PATH]`, `slab parse FILE`. A project is a `Slab.toml` plus `src/main.slab` (`docs/lang/modules.md`, `src/manifest.rs`); a bare directory with a `main.slab` still works. Examples: `examples/slab/*.slab` (single files, must parse + check), `examples/projects/hello` (small multi-file project), `examples/projects/e2e` (the end-to-end test project).
- A language feature touches several places: grammar doc -> parser (+ tests) -> checker rules (+ `docs/lang/semantics.md`, + tests in `src/sema/tests.rs`) -> `codegen` (+ `docs/lang/codegen.md`, + tests in `src/compile.rs`) -> IR/lowering if it needs a runtime construct -> **a case in `examples/projects/e2e` with an expected value in `checks.json`**.
- Adding a keyword, operator, builtin or selector letter is visible to the editor tooling: run `cargo run -p slab-lsp -- grammar` to regenerate the extension's TextMate grammar (a drift test in `crates/slab-lsp` fails until you do).

## Verifying output against the real game

Unit tests only prove the output matches *our reading of the docs*. Everything is also checked on a real 26.3 dedicated server:

```text
tools/verify_all.sh          # builds and checks all three packs below; prints zoo/programs/e2e: OK|FAILED
```

- `examples/zoo.rs` -> `target/zoo`: every supported command variant; passes if the server loads it without parse errors.
- `examples/programs.rs` -> IR programs whose *results* are read back (`checks.json`-style: `[{cmd, expect | matches, wait}]`).
- `examples/projects/e2e` + `checks.json` -> compiled from `.slab` source; 165 checks (recursion, loops, `break`/`continue`, structs, lists, strings, objectives, entities, `#[tick]`, `#[every]`, no leftover locals, `_uninstall`, variable indices/coordinates/effect values, f-strings, command results, entity values, string values, selector variables, equality of stored values, enums, methods, Option/Result methods, `run_command`, entity positions, `match` and `if let`). Checks may use `expect`, `matches`, `absent` or `wait`. Run with `--setup "forceload add 0 0"` so entity tests have a loaded chunk.

`tools/verify_pack.py` downloads the official server jar into `target/mc/` (Java 25+ needed; `--jar`/`--java` override). **Run `tools/verify_all.sh` after changing `lower.rs`, `codegen.rs` or anything in `src/datapack/`**, and record surprises in `docs/mc/05-verified-by-slab.md`. Two traps already found that way: `execute store success ... if A if B` doesn't write when `A` fails, and a console-run function executes at the (per-world) spawn point.

## Planned features (decided, in this order)

1. **Entity values (done, `docs/lang/entities.md`)**: a variable can hold an entity (`let boss = summon(..)`, `let me = @p`) and act on it (`tp(boss, ..)`, `kill`, `effect::give`, `as boss { }`, `points[boss] += 1`, `exists(boss)`). Design: a handle is an `int` id stored as a score on the entity (objective `<ns>_id`, ids from a global counter; assigned on first use, so a selector with one match becomes a handle). Acting on a handle is `@e[scores={<ns>_id=N}]` with `N` supplied as a macro hole, so selectors need a hole variant. The score disappears with the entity, so a stale handle matches nothing. A handle going out of scope does not kill the entity; `_uninstall` still kills what the program summoned.
2. **Strings in macros (done, `docs/lang/codegen.md` "String values")**: `str` variables in ids, tag names and `say` text; `+` and f-strings as real `str` values (a macro with a quoted template). Only text with nothing following it is safe against injection, and a value containing `"` or `\` breaks that one command: document it, and keep `print(f"..")` on `tellraw` components.
3. **Doubles (done, `docs/lang/doubles.md`)**: `NearDouble` (4 decimals, read directly, ±21,474 blocks) and `FarDouble` (6 decimals, read through a scratch marker, whole world) in `std.slab`, written in slab with operators calling methods. **Floats (next)**: type `float` = a float tag in storage; arithmetic by `data modify .. set compute default float {provider}` (providers compose, so one command per expression), `int(f)` truncates toward zero, comparisons via `floor`/`ceil` of the difference. Float coordinates work through macro holes. No truncation happens implicitly. See `docs/mc/02-commands-core.md` section 8.1 and `docs/mc/04-data-formats.md` section 3.9.
4. **Tasks for long loops**: `task fn name(..) { .. }` may take many ticks; its loops are time-sliced (about N iterations per tick) and the code after a loop runs when it finishes (the task body becomes a state machine over static slots); `start name(args)` launches it, one running instance per task. Normal functions stay unchanged and keep the 65536-commands-per-tick limit.

## Project layout

- `src/lib.rs` + modules above; `src/main.rs` the CLI.
- `crates/slab-lsp/` the language server (`slab-lsp` stdio; a workspace member): diagnostics
  from the *tolerant* checker (`sema::check_modules_tolerant`), hover, definition,
  completion, document symbols, semantic tokens; `crates/slab-lsp/src/grammar.rs` generates
  the extension's TextMate grammar from `Kw::ALL`/`BUILTINS`/lexer consts, and
  `tests/lsp.rs` has the drift test + feature tests.
- `editors/vscode/` the VS Code extension (grammar, snippets, `vscode-languageclient`
  wrapper; `npm run vsix` packages + installs it). The server binary is found at
  `<workspace>/target/release/slab-lsp`, `slab.server.path`, or PATH.
- `docs/mc/` Minecraft 26.3 reference. `docs/lang/` language design (`syntax`, `semantics`, `modules`, `stdlib`, `enums`, `codegen`, `ir`, `memory`, `entities`), and `docs/lang/compiled-output.md`: for each kind of slab code, the commands it compiles to, generated from the real compiler by `tests/compiled_output.rs` (after a change that alters the generated commands, run `UPDATE_DOCS=1 cargo test --test compiled_output` and review the diff).
- `examples/` and `tools/` (`verify_pack.py`, `verify_all.sh`); `tests/` integration tests (`projects.rs` drives the `slab` binary).
