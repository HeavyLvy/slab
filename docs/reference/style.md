# Style and naming

slab follows Rust's conventions, and `slab check` and `slab build` warn about names that break them. A warning names the spelling it would like:

```text
warning: function `makeRing` should be snake_case
  hint: rename it to `make_ring`
```

| What | Convention | Example |
|---|---|---|
| functions, methods, parameters, variables, loop variables, `global`s, fields, `objective`s | `snake_case` | `make_ring`, `center_x` |
| `struct`s, `enum`s, enum variants, type parameters | `PascalCase` | `Point`, `Running` |
| `const`s | `SCREAMING_SNAKE_CASE` | `MAX_HEALTH` |

A leading `_` (a name you do not use) is ignored: `_unused` is fine. The standard library is not checked, only your own modules.

The warnings are only advice. To turn them off for a project, set `naming = "off"` under `[lint]` in [Slab.toml](slab-toml.md).
