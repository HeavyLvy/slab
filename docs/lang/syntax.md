# slab syntax (v0.2)

Rust-like surface, files end in `.slab`. Decisions: Rust-like style; Minecraft access through **context blocks + builtin functions**; entry points are **attributes on functions**.

Projects span several files: see `docs/lang/modules.md` for `import`, `pub` and `::`. This document is the contract for `src/syntax/` (lexer, parser, AST). Semantics (types, lowering to the IR in `docs/lang/ir.md`) are sketched per construct but not implemented yet; each section says so.

```slab
#[load]
fn setup() {
    ticks = 0;
}

#[tick]
fn step() {
    ticks += 1;
}

#[every(1s)]
fn heartbeat() {
    as @a[tag=ready] at @s {
        effect.give(@s, "minecraft:speed", 10s);
    }
}

global ticks: int = 0;

fn fib(n: int) -> int {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}
```

## Lexical structure

* Whitespace separates tokens. Comments: `// to end of line`, `/* block, nests */`.
* Identifiers: `[A-Za-z_][A-Za-z0-9_]*`. Keywords (cannot be identifiers):
  `fn let mut if else while loop for in return break continue struct global const objective pub import true false as at positioned rotated facing align anchored`.
* Integers: decimal with `_` separators (`1_000`), or hex `0xFF`. They are `i64` in the AST and range-checked to `i32` later.
* **Time literals**: an integer directly followed by `t` (ticks), `s` (seconds, x20) or `d` (days, x24000): `5t`, `10s`, `1d`. They are plain integers (ticks) with no separate type.
* Floats (`1.5`) lex, but are rejected by the type checker until floats exist.
* **F-strings**: `f"text {expression} more"` (no space between `f` and the quote). Any expression may sit between the braces (`{n + 1}`, `{p.name}`, `{xs.len()}`); `{{` and `}}` write a brace. They have the type `text` and can only be passed to `print` for now.
* Strings: `"..."` with escapes `\n \t \r \\ \" \' \0 \u{1F600}`. Strings may not contain a raw newline.
* **Selector literals**: `@p @r @a @e @s @n`, optionally followed *directly* by `[args]`: `@e[type=zombie, distance=..10, tag=!boss]`. The lexer captures the bracket contents verbatim (it tracks nested `[] {} ()` and quotes); the parser splits them into `key=value` pairs; values keep their raw text and are checked/converted during type checking, because they have their own mini-grammar (`1..5`, `!tag`, `{kills=1..}`).
* **Relative coordinates**: `~` and `^`, optionally *directly* followed by a number: `~`, `~1`, `~-2.5`, `^3`. Used where a position is expected (`positioned ~ ~1 ~ { }`, `setblock(~, ~-1, ~, ...)`).
* Punctuation: `( ) { } [ ] , ; : :: . .. ..= -> # = == != < <= > >= + - * / % += -= *= /= %= && || !`.

## Grammar

```
program     := item*
item        := attr* 'pub'? ( fn_decl | global_decl | const_decl | struct_decl | objective_decl )
             | import_decl                                       // `pub import` is an error
import_decl := 'import' IDENT ('::' IDENT)* ['as' IDENT] ';'    // the module a/b -> a/b.slab
             | 'import' IDENT ('::' IDENT)* '::' '{' IDENT (',' IDENT)* [','] '}' ';'   // items
attr        := '#' '[' IDENT ( '(' [expr (',' expr)* [',']] ')' )? ']'

fn_decl     := 'fn' IDENT '(' [param (',' param)* [',']] ')' ['->' type] block
param       := IDENT ':' type
global_decl := 'global' IDENT ':' type '=' expr ';'
const_decl  := 'const'  IDENT ':' type '=' expr ';'
struct_decl := 'struct' IDENT '{' [field_decl (',' field_decl)* [',']] '}'
field_decl  := IDENT ':' type
objective_decl := 'objective' IDENT '=' expr ';'        // expr: criteria string, e.g. "dummy"

type        := [IDENT '::'] IDENT [ '<' type (',' type)* '>' ]   // int, bool, str, list<int>, Player, util::Point

block       := '{' stmt* '}'
stmt        := let_stmt | if_stmt | while_stmt | loop_stmt | for_stmt
             | 'return' [expr] ';' | 'break' ';' | 'continue' ';'
             | context_stmt | block
             | expr ( assign_op expr )? ';'
let_stmt    := 'let' ['mut'] IDENT [':' type] '=' expr ';'
assign_op   := '=' | '+=' | '-=' | '*=' | '/=' | '%='     // target: name, field access or index
if_stmt     := 'if' expr block ['else' (if_stmt | block)]
while_stmt  := 'while' expr block
loop_stmt   := 'loop' block
for_stmt    := 'for' IDENT 'in' expr block

context_stmt := ctx_clause+ block
ctx_clause   := 'as' operand | 'at' operand | 'in' operand
              | 'positioned' operand operand operand
              | 'rotated' operand operand
              | 'facing' operand operand operand
              | 'align' IDENT                // x, y, z or a combination: xz, xyz
              | 'anchored' IDENT             // eyes | feet
operand      := unary-level expression (no struct literals), e.g. @a[tag=x]  ~  ~-1  64  (y + 1)

expr        := range
range       := [or] ('..' | '..=') [or]  |  or             // either side may be omitted
or          := and ('||' and)*
and         := cmp ('&&' cmp)*
cmp         := add (('==' | '!=' | '<' | '<=' | '>' | '>=') add)?   // not chainable
add         := mul (('+' | '-') mul)*
mul         := unary (('*' | '/' | '%') unary)*
unary       := ('-' | '!') unary | postfix
postfix     := primary ( '(' args ')' | '.' IDENT | '[' expr ']' )*
primary     := INT | FLOAT | STRING | 'true' | 'false' | IDENT | SELECTOR | RELCOORD
             | '(' expr ')'
             | '[' [expr (',' expr)* [',']] ']'                    // list
             | '{' [entry (',' entry)* [',']] '}'                  // anonymous compound: { a: 1, "b c": 2 }
             | [IDENT '::'] IDENT '{' [entry (',' entry)* [',']] '}'   // struct literal
             | IDENT '::' IDENT                                    // item of an imported module
             | 'f' STRING-WITH-HOLES                               // f-string: f"x = {expr}"
entry       := (IDENT | STRING) ':' expr
```

Precedence, loosest to tightest: range, `||`, `&&`, comparison, `+ -`, `* / %`, unary `- !`, postfix. Comparisons do not chain (`a < b < c` is an error).

**Struct literals vs. blocks.** In the head of `if`, `while`, `for` and in context-clause operands, `Name {` is *not* a struct literal (the `{` starts the body). Wrap in parentheses to force one: `if (Point { x: 1 }).x == 1 { }`.

**Statements need `;`** except block-like statements (`if`, `while`, `loop`, `for`, context blocks, bare blocks). There is no implicit "last expression" value: functions return with `return`.

## Semantics, per construct (to be implemented in the semantic phase)

| Construct | Meaning / lowering plan |
|---|---|
| `fn` | IR `Function`. Parameters and return types are `int`, `bool` (scoreboard), or `str`, `list<T>`, structs (storage). |
| `#[load]` / `#[tick]` | IR `Entry::Load` / `Entry::Tick`. No parameters. |
| `#[every(N)]` | Runs every `N` ticks (`N` an integer or time literal). Lowers to a tick function with a global countdown (`every(1s)` == `every(20)`). |
| `global`, `const` | IR `Global` (initial value must be constant) / compile-time constant. Globals persist across ticks and `/reload`. |
| `struct` | A compound in command storage; `p.x` is a field path. Only `int`/`bool`/`str`/`list`/struct fields. |
| `let`, `let mut` | Locals. Immutable unless `mut` (checked by the type checker). Type inferred from the initializer if omitted. |
| `if`/`while`/`loop`/`for`/`break`/`continue` | IR `If`/`While`; `loop` is `while true`; `break`/`continue` need IR support (flag variables) - not in the IR yet. `for i in a..b` counts; `for x in list` walks a list. |
| context blocks | `as`/`at`/`in`/`positioned`/`rotated`/`facing`/`align`/`anchored` map one-to-one to `/execute` sub-commands; the body becomes a function called through the `execute` chain. Inside `as X`, `@s` is the entity; outside, `@s` is whatever the enclosing function was run as. Needs a new IR statement (`Stmt::Context`). |
| `kills[@s]` (index on an `objective`) | A scoreboard score of an entity / fake player: reads and writes (`+=`) use `scoreboard players ...`. `objective kills = "playerKillCount";` declares it. |
| `@e[...]` | A selector value (type `entity`/selector). Used with context blocks and builtins (`exists(@e[type=zombie])` -> `execute if entity`). |
| builtins | `say`, `print`, `exists`, `tp`, `setblock`, `summon`, `kill`, `effect.give`, `tag.add`... are resolved by name in the type checker and become typed `datapack::Command`s. The set is TBD. |

## Open questions

* String interpolation / formatting (`f"score: {x}"`) needs macros; deferred.
* Floats (via `/compute`), enums, `match`, modules/imports, generics beyond `list<T>`.
* Whether `if` becomes an expression (`let x = if c { a } else { b };`).
* `for e in @a { ... }` as sugar for `as @a { ... }`.
