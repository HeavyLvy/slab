# Style and naming

## Naming

slab uses the naming conventions most modern languages share (they are Rust's), and `slab check` and `slab build` warn about names that break them. A warning names the spelling it would like:

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

## Formatting

`slab fmt` lays out your code the one way, so nobody has to argue about it (see [the command line](cli.md)). In an editor with the slab extension, "Format Document" does the same.

```slab
import std::print;
import maps::{Arena, build};   // names inside `import { }` hug the braces

struct Point { x: int, y: int }

#[every(1s)]
fn report(points: list<Point>) -> int {
    let mut total = 0;
    for p in points {
        if p.x > 0 && p.y > 0 {
            total += p.x;
        } else {
            total -= 1;
        }
    }
    return total;
}
```

What it decides:

- Four spaces per level. A line is indented one level deeper than the line that opened the bracket it is inside, and one more when it continues a statement from the line before (an operator at the start of a line, a `.method()` chain, an argument list that breaks).
- One space around binary operators, `=`, `->`, `=>` and `|`; none inside `()` and `[]`, before `,` `;` `:`, or around `.`, `::` and `..`. A space after `,` and `:`.
- `{` stays on the line of its header, and `else` on the line of the `}` before it. `{ .. }` has a space inside, `{}` has none.
- Type arguments are tight (`list<int>`, `Result<int, str>`), and `<` between two values is a comparison with spaces.
- At most one blank line in a row, none right after `{` or before `}`. No trailing spaces; the file ends with one newline.

What it leaves to you: where lines break (it neither wraps long lines nor joins short ones, so `fn f() { return 1; }` on one line stays on one line), comments and their positions, and the text inside strings and f-strings. Code that does not parse is never touched, and the formatter checks that only whitespace changed before it writes anything.
