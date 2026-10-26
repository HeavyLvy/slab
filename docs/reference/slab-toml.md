# Slab.toml

`Slab.toml` describes a project, the way `Cargo.toml` does for a Rust one. It sits at the top of the project, next to the `src/` folder.

```toml
[project]
name = "my_game"                 # required
version = "0.1.0"
description = "Shown by /datapack list"
namespace = "my_game"            # optional
minecraft = "26.3"               # optional

[build]
abort-on-unwrap = false
pack-id = "file/my_game"
task-budget = 30000
command-limit = 1000000

[lint]
naming = "warn"                  # or "off"
```

## `[project]`

| Key | Meaning |
|---|---|
| `name` | **Required.** The project's name. It is also the pack's namespace unless `namespace` says otherwise (lowercased, with every character that is not a letter or digit turned into `_`: `My-Game` becomes `my_game`). |
| `version` | Free text, for you. |
| `description` | Shown by `/datapack list`. |
| `namespace` | The pack's namespace, when it should differ from the name. Lowercase letters, digits and `_ - . +`. It is the prefix of everything the pack creates (`<namespace>:boot`, the scoreboard objective `<namespace>`, the storage `<namespace>:vars`). |
| `minecraft` | The game version the pack is for: `26.1`, `26.2` or `26.3` (a patch release such as `26.1.2` counts as its family). It sets the data pack format in `pack.mcmeta` (26.1 is 101.1, 26.2 is 107.1, 26.3 is 121.0). Only 26.3 has been run against a real server: building for another version works but prints a warning. Versions before 26.1 are refused. Default: `26.3`. |

## `[build]`

| Key | Meaning |
|---|---|
| `abort-on-unwrap` | When `unwrap()`/`expect()` fails, besides telling the players, run `datapack disable` for the pack. Default `false`. |
| `pack-id` | The pack that command disables. Default `file/<namespace>`. |
| `task-budget` | Commands a [`task fn`](../lang/tasks.md) may use per tick before it waits for the next. 100 or more; default 30000. |
| `command-limit` | Raise the game's `max_command_sequence_length` (65,536 by default) to this when the pack loads; `_uninstall` puts the old value back. See [Tasks](../lang/tasks.md) and [Projects and modules](../lang/modules.md). It is a rule of the whole world. |

## `[lint]`

| Key | Meaning |
|---|---|
| `naming` | `"warn"` (default) reports names that break the [naming conventions](style.md) as warnings; `"off"` turns that off. The warnings never stop a build. |

Only this part of TOML is read: tables, and `key = "text"`, `true`/`false` or a number, with `#` comments. A key slab does not know is an error that names the line, so a typo cannot be ignored silently.

The same settings exist as [command line options](cli.md), and those win.
