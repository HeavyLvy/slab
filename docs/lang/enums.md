# Enums, `Option` and `Result`

An `enum` is a type with a fixed set of **variants**, each optionally carrying values:

```slab
enum Shape {
    Dot,                  // no payload
    Line(int, int),       // a payload of two values
    Named(str, list<int>),
}

fn demo() {
    let a = Shape::Dot;
    let b = Shape::Line(1, 2);
    if b == Shape::Line(1, 2) { print("same"); }
}
```

Every variant is written `Enum::Variant`, with the payload in parentheses. Variants are compared with `==` / `!=` like any stored value. (Looking inside a value with `match` is the next step in `docs/lang/plan-enums-results.md`; until then `Option` and `Result` have methods, below, and an enum can be built, stored, passed, returned, printed and compared. Your own enums can have methods too: `impl Shape { fn area(self) -> int { .. } }`, see `docs/lang/semantics.md`.)

## Generic enums

An enum can take type parameters, written like Rust's:

```slab
enum Either<A, B> { Left(A), Right(B) }

let x: Either<int, str> = Either::Left(3);
```

Type arguments are **inferred locally**: from the context (an annotation, a parameter, a return type, a field, the other side of `==`) or from the payload (`Either::Left(3)` alone is `Either<int, ?>`, which is not enough: the other argument has to come from the context). When nothing says what a parameter is, the compiler asks you to annotate: `let a = None;` is an error, `let a: Option<int> = None;` is not. There are **no generic functions** yet; generics exist on enums only.

`()` is the type with one value, `()`. It can be an enum's payload (`Result<(), str>`, `Ok(())`) but cannot be stored in a variable by itself, and a variant with a `()` payload stores nothing for it.

## `Option` and `Result`

Two enums are declared in the standard library (`src/stdlib/std.slab`):

```slab
pub enum Option<T> { None, Some(T) }
pub enum Result<T, E> { Ok(T), Err(E) }
```

`Option`, `Result`, `Some`, `None`, `Ok` and `Err` are the **prelude**: they are visible in every module without an import (a definition of your own with the same name wins). They are also reachable as `Option::Some(1)`, `std::Option::None` and `import std::*;`. The variants of enums in the standard library are names of their own (`Some`, not only `Option::Some`); the variants of your enums are not, they are always written `Shape::Dot`.

### Methods

| `Option<T>` | |
|---|---|
| `is_some()`, `is_none()` | `bool` |
| `unwrap()` | the `T` inside `Some` |
| `unwrap_or(default)` | the `T` inside, or `default` |
| `expect("message")` | like `unwrap`, with your message |

| `Result<T, E>` | |
|---|---|
| `is_ok()`, `is_err()` | `bool` |
| `unwrap()`, `expect("message")` | the `T` inside `Ok` |
| `unwrap_or(default)` | the `T` inside, or `default` |
| `ok()`, `err()` | `Option<T>`, `Option<E>` |

`default` is always evaluated. `unwrap()` and `expect()` on `None` / `Err` have no panic to use: they print `slab: unwrap on None (main.slab:12)` to chat and return from the function (zero value of its return type; a context block, which cannot be left, only prints). Built with `--abort-on-unwrap` they also stop the pack (`datapack disable "file/<namespace>"`, or the `--pack-id` you give).

## Constants and globals

Enum values can be constants and global initialisers when their payload is constant: `global start: Option<int> = Some(7);`, `const NOTHING: Option<str> = None;`.

## How they are stored

A value is a compound in command storage: `{t: <variant index>, f0: .., f1: ..}`. `t` is the index in declaration order (`None` is 0 and `Some` 1; `Ok` is 0 and `Err` 1); the payload values are `f0`, `f1`, ... as NBT (an `int` is an int tag, a `bool` is 0 or 1, an `EntityId` is its id). A unit variant is `{t:0}`. See `docs/lang/compiled-output.md` ("Enums") for the commands a program produces, and `docs/lang/codegen.md` for the rest of the mechanism. Enums are `Data` values like lists and structs: copied when assigned, and removed from storage when the function that owns them ends.

## What comes next

`match` and `if let` to take a value apart, commands that return `Result`, and `game::run_command`. The plan is in `docs/lang/plan-enums-results.md`.
