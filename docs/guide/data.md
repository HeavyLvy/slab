# Structs, enums and match

## Structs

```slab
struct Point {
    x: int,
    y: int,
}

impl Point {
    fn origin() -> Point {                       // an associated function: Point::origin()
        return Point { x: 0, y: 0 };
    }

    fn sum(self) -> int {                        // a method: p.sum()
        return self.x + self.y;
    }
}

fn structs() {
    let mut p = Point { x: 1, y: 2 };
    p.x += 10;
    let total = p.sum();
    let o = Point::origin();
    if p != o { print(f"{p.x}, {p.y}, total {total}"); }
}
```

`==` and `!=` compare field by field. A struct is copied when assigned or passed, so changing the copy never changes the original.

## Enums

An enum is one of a fixed set of *variants*, each of which may carry values.

```slab
enum Phase {
    Waiting,
    Running(int),                                // seconds left
    Done(str),                                   // the winner
}

impl Phase {
    fn is_over(self) -> bool {
        return match self {
            Phase::Done(_) => true,
            _ => false,
        };
    }
}
```

## `match`

`match` picks the first arm whose pattern fits, and it must cover every case, or the compiler names the one you forgot (`this match does not cover `None``).

```slab
enum Phase { Waiting, Running(int), Done(str) }           // as above

fn label(p: Phase) -> str {
    return match p {
        Phase::Waiting => "waiting",
        Phase::Running(n) if n > 10 => "a long time",       // a guard: `if` after the pattern
        Phase::Running(n) => f"running, {n}s left",
        Phase::Done(winner) => f"{winner} won",
    };
}

fn describe(n: int, word: str) -> int {
    match n {
        0 | 1 => print("tiny"),                              // alternatives
        x if x > 100 => print("huge"),
        _ => print("some"),
    }
    return match word {
        "yes" => 1,
        "no" => 0,
        _ => -1,
    };
}
```

* Patterns are `_` (anything), a name (binds the value), a literal (`1`, `true`, `"yes"`), `Variant`, `Variant(p, ..)` nested as deep as you like, and `a | b`.
* A `match` can be a value (every arm gives one type) or a statement. An arm may `return`, `break` or `continue`, which fits any type.
* `if let` is the one-arm form: `if let Some(x) = value { ... } else { ... }`.

## `Option` and `Result`

Two enums that need no import:

```slab
fn lookup(xs: list<int>, i: int) -> Option<int> {
    if i < 0 || i >= xs.len() { return None; }
    return Some(xs[i]);
}

fn options() {
    let found = lookup([5, 6, 7], 1);
    let n = found.unwrap_or(0);
    if let Some(v) = found { print(f"found {v}"); }
    match found {
        Some(v) => print(v),
        None => print("nothing"),
    }
    let missing: Option<int> = None;               // `None` needs its type from the context
    if missing.is_none() { print(n); }
}

fn results() {
    let r = run_command("scoreboard players get #score pack");   // Result<int, ()>
    match r {
        Ok(value) => print(f"score is {value}"),
        Err(_) => print("no score"),
    }
}
```

| `Option<T>` | | `Result<T, E>` | |
|---|---|---|---|
| `is_some()`, `is_none()` | which one | `is_ok()`, `is_err()` | which one |
| `unwrap()` | the value, or a message in chat if it is missing | `unwrap()` | the value, or a message |
| `unwrap_or(d)` | the value or `d` | `unwrap_or(d)` | the value or `d` |
| `expect("msg")` | like `unwrap` with your message | `ok()`, `err()` | an `Option` of either side |

`unwrap` on a missing value does not crash anything: it tells the players (`slab: unwrap on None (main.slab:12)`) and returns from the function. Build with `--abort-on-unwrap` to also stop the pack.

More: [Enums, `Option` and `Result`](../lang/enums.md) goes into variants, generics and constants.

Next: [Talking to Minecraft](minecraft.md).
