# The command line

```text
slab new NAME                 create a project: NAME/Slab.toml and NAME/src/main.slab
slab init                     make the current directory a project (moves .slab files into src/)
slab build [PATH] [options]   compile a project to a data pack
slab check [PATH]             parse and type-check a project, reporting errors and warnings
slab fmt [PATH...] [options]  format source files in place
slab test [PATH] [options]    run the project's #[test] functions on a real Minecraft server
slab parse <file.slab>        parse one file and print its syntax tree
slab --version                print the version
```

`PATH` is a project directory, or any directory inside one (the default is `.`; slab looks for a `Slab.toml` in that directory and then in each parent directory), or the path of a `main.slab`. A project is a directory with a [`Slab.toml`](slab-toml.md) and its code in `src/`; `src/main.slab` is the root module and `import a::b;` loads `src/a/b.slab`. A bare directory with a `main.slab` and no `Slab.toml` also works.

## `slab build`

```text
slab build [PATH] [-o DIR] [--zip] [--watch] [options]
```

| Option | Meaning |
|---|---|
| `-o`, `--out DIR` | write the data pack folder here (default: `<project>/build/<namespace>`). An existing folder is replaced. |
| `--zip` | also write `DIR.zip` |
| `-w`, `--watch` | keep running and build again whenever a source file is saved, added or removed. Errors are shown and watching goes on. Ctrl+C stops it. |
| `--namespace NAME` | the data pack namespace (default: from `Slab.toml`, else the directory's name) |
| `--description TEXT` | the description shown by `/datapack list` |
| `--minecraft VERSION` | the game version to build for: `26.1`, `26.2` or `26.3` (default: `Slab.toml`, else `26.3`) |
| `--abort-on-unwrap` | a failed `unwrap()`/`expect()` also runs `datapack disable` for the pack |
| `--pack-id ID` | the pack id for that (default: `file/<namespace>`) |
| `--task-budget N` | commands a `task fn` may use per tick (default 30000) |
| `--command-limit N` | set the game's commands-per-tick limit to N when the pack loads |

Anything given on the command line wins over `Slab.toml`.

Warnings are printed but do not fail the build. They include names that break the [naming conventions](style.md) (turn that off with `[lint] naming = "off"` in `Slab.toml`). They include an entry point that is estimated to run far more than the game's 65,536 commands per tick; see [Tasks](../lang/tasks.md).

## `slab check`

Parses and type-checks without writing anything. Exit code 0 means no errors; warnings are printed.

## `slab fmt`

```text
slab fmt [PATH...] [--check] [--stdin]
```

Rewrites `.slab` files in the canonical layout described in [Style and naming](style.md). Each `PATH` is a file, a project (its `src/` is formatted) or a directory (every `.slab` file below it); the default is `.`. It prints the files it changed.

| Option | Meaning |
|---|---|
| `--check` | Change nothing. List the files that are not formatted and exit with 1 if there are any. Meant for CI. |
| `--stdin` | Read one file from standard input and write it formatted to standard output. This is what editors use. |

A file with a syntax error is reported and left as it is. The language server offers the same formatting as "Format Document".

## `slab test`

```text
slab test [PATH] [--filter TEXT] [--list] [--timeout SECS] [--seed N] [--strict-commands] [--realtime] [--hold | --serve] [--port N]
               [--java PATH] [--jar PATH] [--accept-eula] [-v]
```

Builds the project **with its `#[test]` functions**, starts a real Minecraft server in `build/test-server/`, runs each test in a reset world and reports which passed. Needs Java 25 or newer, and downloads the server jar from Mojang once, which means accepting the [Minecraft EULA](https://aka.ms/MinecraftEULA) (`--accept-eula`, `SLAB_ACCEPT_EULA=1`, or `accept-eula = true` under `[test]` in `Slab.toml`). Exit code 1 when a test fails. The [Testing guide](../guide/testing.md) explains how tests are written and what a test starts in.

| Option | Meaning |
|---|---|
| `--filter TEXT` | only the tests whose name contains `TEXT` |
| `--list` | list the tests without running them (no Java or server needed) |
| `--timeout SECS` | seconds one test may take (default 60, or `timeout` under `[test]`) |
| `--seed N` | the world seed (default 0, or `seed` under `[test]`): the same seed gives the same run |
| `--strict-commands` | every test fails when a command whose result it ignores fails (or `strict-commands = true` under `[test]`); `#[test(lenient)]` opts one out |
| `--realtime` | let game time pass by itself; by default it stands still and is advanced by slab, see [Testing](../guide/testing.md#the-same-run-every-time) |
| `--hold` | after the tests, keep the server running for you, with a console |
| `--serve` | start the server with your pack and no tests, for you to join and use |
| `--port N` | the server port (with `--hold`/`--serve`, 25565 when it is free) |
| `--java PATH` | the Java to run the server with (default: `$JAVA`, `$JAVA_HOME`, then `java` on `PATH`) |
| `--jar PATH` | use this server jar instead of the downloaded one (or `$SLAB_SERVER_JAR`) |
| `--accept-eula` | you accept the Minecraft EULA |
| `-v`, `--verbose` | show the server's log lines of every test |

`$SLAB_CACHE_DIR` moves the folder where the server jar is kept.

## `slab new` and `slab init`

`slab new my_pack` makes a folder with a `Slab.toml`, `src/main.slab` and a `.gitignore`. `slab init` does the same in the current directory, and if it holds a flat project (a `main.slab` at the top) it moves the `.slab` files and the module folders into `src/`.

## Exit codes

| | |
|---|---|
| 0 | success |
| 1 | the program has errors, or something could not be read or written |
| 2 | the command line was wrong |
