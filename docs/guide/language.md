# The language

slab uses braces, `let`, `fn`, `->` and `::` for paths, so if you have seen Rust, TypeScript, Swift or Kotlin it will look familiar; if not, this page explains each piece. Every example on this page is checked by the compiler when the documentation is built, so what you see works.

## Values and variables

```slab
fn values() {
    let health = 20;                 // an int: a 32-bit number that wraps around
    let mut score = 0;               // `mut` lets you assign again
    score += 5;
    let name: str = "Steve";         // the type can be written out
    let alive = true;                // a bool
    let drops = [1, 2, 3];           // list<int>
    let delay = 10s;                 // time: an int counted in ticks (10s is 200)
    print(f"{name} has {health} hp and {score} points, drops: {drops}");
}
```

* **Types** are `int`, `bool`, `str`, `list<T>`, your own [structs and enums](data.md), `Entity` (see [Talking to Minecraft](minecraft.md)), and numbers with decimals ([`NearDouble` and `FarDouble`](../lang/doubles.md)).
* **Inference.** A `let` takes its type from its value; you only write it when there is nothing to infer from (`let xs: list<int> = [];`).
* **Time literals.** `20t` is 20 ticks, `1s` is 20 ticks, `1d` is a whole day (24,000). They are plain `int`s.
* **Values are copied.** Assigning or passing a list or a struct copies it; there are no references, so nothing can change under you.
* **Comments.** `// like this`, `/* or this */`. `///` documents the item below it (the editor shows it on hover) and `//!` documents the file.

## Operators

```slab
fn operators(a: int, b: int, ok: bool) -> bool {
    let sum = a + b * 2 - 1;         // + - * / %
    let half = a / 2;                // division and % round down, like the scoreboard
    let same = a == b || a != b;     // == != < <= > >=, and && || !
    let text = "hp: " + "20";        // + joins strings
    return !ok && sum > half;
}
```

`&&` and `||` evaluate both sides, so a function call is not allowed inside them: put the call in a `let` first.

## Control flow

```slab
fn flow(n: int) -> int {
    let mut total = 0;
    for i in 0..n { total += i; }                // 0..n stops before n; 0..=n includes it
    while total > 100 { total -= 100; }
    loop {                                       // forever, until `break`
        total += 1;
        if total > 5 { break; }
    }
    for x in [1, 2, 3] {                         // over a list
        if x == 2 { continue; }
        total += x;
    }
    if total > 10 {
        return 10;
    } else if total < 0 {
        return 0;
    }
    return total;
}
```

Conditions are plain `bool`s and need no parentheses. A function that returns a value must return on every path.

**How long can a loop be?** Minecraft stops a function after 65,536 commands in one tick, and a loop with real work in it gets there quickly. slab warns when it can tell; for long jobs use a [`task`](../lang/tasks.md).

## Functions

```slab
fn clamp(x: int, lo: int, hi: int) -> int {
    if x < lo { return lo; }
    if x > hi { return hi; }
    return x;
}

fn fact(n: int) -> int {                         // recursion works
    if n <= 1 { return 1; }
    return n * fact(n - 1);
}
```

Parameters are typed, the return type follows `->` (leave it out for "nothing"). Functions can appear in any order.

## Lists and strings

```slab
fn collections() {
    let mut names = ["Ann", "Bob"];
    names.push("Cy");
    let first = names[0];
    let count = names.len();
    let greeting = "hi " + first;
    let long = greeting.len() > 3;
    if first == "Ann" && long { print(f"{count} names"); }
}
```

## Constants and globals

```slab
const MAX_HEALTH: int = 20;                      // known at compile time
global kills: int = 0;                           // keeps its value across ticks and /reload

fn kill_one() {
    kills += 1;
    if kills >= MAX_HEALTH { kills = 0; }
}
```

A global's starting value must be a constant. Constants and globals can be lists, structs and enums too.

## Text with values in it

```slab
fn text(n: int, who: str) {
    print(f"{who} found {n + 1} diamonds");      // any expression inside the braces
    say("plain chat message");                   // a string works too
    let line = f"{{braces}} and {n}";            // {{ and }} are literal braces
    say(line);
}
```

`print` shows a message to every player. An f-string is also an ordinary `str` value you can store and pass around.

## Modules

A project is split into files; `import` names what you want from another file.

```text
src/main.slab
src/util.slab          // import util;          util::clamp(x, 0, 10)
src/maps/arena.slab    // import maps::arena;   arena::reset()
```

Everything is private to its file unless marked `pub`. See [Projects and modules](../lang/modules.md) for the details, and [Talking to Minecraft](minecraft.md) for the built-in `std` and `game` modules.

Next: [Structs, enums and match](data.md).
