# What slab compiles to

This page shows, for each kind of slab code, the Minecraft commands it becomes. It is for the curious: you never have to read the compiler to see what a line of slab does at the level of `.mcfunction` files.

**These listings are generated.** Every example below is compiled by a test (`tests/compiled_output.rs`) and the commands shown are the compiler's real output, so this page cannot drift from the compiler. Each program is compiled with `import std::*; import game::*;` in front and the namespace `demo` (the namespace is the name of the pack and of the scoreboard objective). The exact numbering of temporaries (`t_0`, `t_1`, ...) and helper functions (`b0`, `m1`, ...) can change between compiler versions; the shape does not. After an intended compiler change, run `UPDATE_DOCS=1 cargo test --test compiled_output` and review the diff.

## How values are stored

| slab | lives in | looks like |
|---|---|---|
| `int`, `bool` (and an `EntityId` handle) | the scoreboard, objective `demo` | holder `#fn.v_name` for a local, `#fn.t_N` for a temporary, `#g.name` for a global, `#ret` for a return value |
| `str`, `list`, structs | command storage `demo:vars` | `fn.v_name` for a local, `fn.t_N` for a temporary |
| globals of those types | command storage `demo:globals` | `name` |
| arguments of a command that are only known at run time | command storage `demo:macro` | keys `h0`, `h1`, ... (see "Minecraft commands" below) |

`fn` is the function's name; a function of module `util` called `double` is the function `util__double`. Every function is a file `data/demo/function/<name>.mcfunction`. Anything with a body (`if`, `while`, a context block) becomes its own small function, named `<fn>/b0`, `<fn>/b1`, ... (blocks) or `<fn>/m0`, ... (macro helpers).

Minecraft limits one tick to 65,536 commands, which is why a loop can run about 21,000 times per tick (each iteration is a call).

## Reading the output

- `#f.v_x` is the local `x` of function `f`; `#f.t_0` is a temporary the compiler needed; `#ret` carries a function's return value; `#g.x` is a global; `#c_2` holds the constant 2.
- The `return` lines at the end of a function (`return 1`, `return 0`) are bookkeeping: a function made for an `if` or loop body returns 1 when the user's function must stop (an early `return`, `break`), and its caller then does `run return 1`; a loop body returns 0 when the loop is simply over.
- `# function demo:f` lines are not part of the output: they only say which file the commands are in.
- Commands that contain `$(h0)`-style holes are *macros*: Minecraft fills them in from a storage compound when the function is called `with storage demo:macro`. They are the only way to put a run-time value into a command, and each macro call costs about `2n + 3` commands for `n` values.

## Functions and calls

Arguments are written into the callee's own score holders before the call, the result comes back in `#ret`.

```slab show=double,boot
fn double(x: int) -> int { return x * 2; }

#[load]
fn boot() {
    let y = double(21);
    print(y);
}
```
```mcfunction
# function demo:double
scoreboard players operation #ret demo = #double.v_x demo
scoreboard players operation #ret demo *= #c_2 demo
return 1

# function demo:boot
scoreboard players set #double.v_x demo 21
function demo:double
scoreboard players operation #boot.v_y demo = #ret demo
tellraw @a {score:{name:"#boot.v_y",objective:"demo"}}
```

`print` with a number shows the score with a `tellraw` text component, so the number is read at the moment the message is sent.

### Recursion

A function that can run again while it is still running (directly or through other functions) saves its live variables on a stack in `demo:stack` before the recursive call and restores them after it.

```slab show=fact
fn fact(n: int) -> int {
    if n <= 1 { return 1; }
    return n * fact(n - 1);
}
```
```mcfunction
# function demo:fact
execute if score #fact.v_n demo matches ..1 if function demo:fact/b0 run return 1
scoreboard players operation #fact.t_0 demo = #fact.v_n demo
scoreboard players operation #fact.t_1 demo = #fact.v_n demo
scoreboard players remove #fact.t_1 demo 1
data modify storage demo:stack i append value 0
execute store result storage demo:stack i[-1] int 1 run scoreboard players get #fact.v_n demo
data modify storage demo:stack i append value 0
execute store result storage demo:stack i[-1] int 1 run scoreboard players get #fact.t_0 demo
scoreboard players operation #fact.v_n demo = #fact.t_1 demo
function demo:fact
execute store result score #fact.t_0 demo run data get storage demo:stack i[-1]
data remove storage demo:stack i[-1]
execute store result score #fact.v_n demo run data get storage demo:stack i[-1]
data remove storage demo:stack i[-1]
scoreboard players operation #fact.t_1 demo = #ret demo
scoreboard players operation #ret demo = #fact.t_0 demo
scoreboard players operation #ret demo *= #fact.t_1 demo
return 1
```

## Variables and arithmetic

`+ - * / %` map to `scoreboard players operation`. Division and remainder are *floored* (like Minecraft's own), and everything wraps at 32 bits.

```slab show=f
fn f(a: int, b: int) -> int {
    let mut n = a + 5;
    n *= b;
    n -= 1;
    n = n / 2 % 10;
    return n;
}
```
```mcfunction
# function demo:f
scoreboard players operation #f.v_n demo = #f.v_a demo
scoreboard players add #f.v_n demo 5
scoreboard players operation #f.v_n demo *= #f.v_b demo
scoreboard players remove #f.v_n demo 1
scoreboard players operation #f.t_0 demo = #f.v_n demo
scoreboard players operation #f.t_0 demo /= #c_2 demo
scoreboard players operation #f.v_n demo = #f.t_0 demo
scoreboard players operation #f.v_n demo %= #c_10 demo
scoreboard players operation #ret demo = #f.v_n demo
return 1
```

A constant on the right becomes `scoreboard players add`/`remove` where possible; other constants are kept in a holder (`#c_N`) that `_init` sets once.

### Comparisons, `&&`, `||`, `!`

A comparison is an `execute if score` test. Chained `&&` becomes several tests in one `execute`; `||` sets a flag from each branch.

```slab show=f
fn f(a: int, b: int) -> bool {
    return a < b && a != 0 || b == 7;
}
```
```mcfunction
# function demo:f
scoreboard players set #f.t_0 demo 0
execute if score #f.v_a demo < #f.v_b demo unless score #f.v_a demo matches 0 run scoreboard players set #f.t_0 demo 1
execute if score #f.v_b demo matches 7 run scoreboard players set #f.t_0 demo 1
execute store success score #ret demo if score #f.t_0 demo matches 1
return 1
```

## Control flow

An `if`/`else` body is a function; the condition is the `execute if/unless` that calls it.

```slab show=f,f/b0,f/b1
fn f(x: int) {
    if x > 10 {
        say("big");
    } else {
        say("small");
    }
}
```
```mcfunction
# function demo:f
execute store success score #f.t_0 demo if score #f.v_x demo matches 11..
execute if score #f.t_0 demo matches 1 run function demo:f/b0
execute unless score #f.t_0 demo matches 1 run function demo:f/b1

# function demo:f/b0
say big

# function demo:f/b1
say small
```

A `while` loop is a function that calls itself while the condition holds (a tail call, so it does not use the stack).

```slab show=f,f/b0
fn f() {
    let mut i = 0;
    while i < 3 {
        i += 1;
    }
}
```
```mcfunction
# function demo:f
scoreboard players set #f.v_i demo 0
function demo:f/b0

# function demo:f/b0
execute unless score #f.v_i demo matches ..2 run return 0
scoreboard players add #f.v_i demo 1
return run function demo:f/b0
```

`for i in a..b` is a `while` with a counter; `break` and `continue` become flags that guard the statements after them.

```slab show=f,f/b0,f/b1,f/b2,f/b3,f/b4
fn f() {
    let mut sum = 0;
    for i in 0..10 {
        if i == 3 { continue; }
        if i == 7 { break; }
        sum += i;
    }
}
```
```mcfunction
# function demo:f
scoreboard players set #f.v_sum demo 0
scoreboard players set #f.v_i demo 0
scoreboard players set #f.v_end demo 10
scoreboard players set #f.v_brk demo 0
scoreboard players set #f.v_cont demo 0
function demo:f/b0

# function demo:f/b0
scoreboard players set #f.t_0 demo 0
execute store success score #f.t_0 demo if score #f.v_brk demo matches 0 if score #f.v_i demo < #f.v_end demo
execute unless score #f.t_0 demo matches 1 run return 0
scoreboard players set #f.v_cont demo 0
execute if score #f.v_i demo matches 3 run function demo:f/b1
execute if score #f.v_cont demo matches 0 run function demo:f/b2
scoreboard players add #f.v_i demo 1
return run function demo:f/b0

# function demo:f/b1
scoreboard players set #f.v_cont demo 1

# function demo:f/b2
execute if score #f.v_i demo matches 7 run function demo:f/b3
execute if score #f.v_brk demo matches 0 run function demo:f/b4

# function demo:f/b3
scoreboard players set #f.v_brk demo 1

# function demo:f/b4
scoreboard players operation #f.v_sum demo += #f.v_i demo
```

`for x in list` walks a copy of the list, taking the first element each time.

```slab show=f,f/b0
fn f() {
    let mut total = 0;
    for x in [4, 5, 6] {
        total += x;
    }
}
```
```mcfunction
# function demo:f
scoreboard players set #f.v_total demo 0
data modify storage demo:vars f.v_iter set value [4,5,6]
function demo:f/b0
data remove storage demo:vars f.v_iter

# function demo:f/b0
execute store result score #f.t_0 demo run data get storage demo:vars f.v_iter
execute unless score #f.t_0 demo matches 1.. run return 0
execute store result score #f.v_x demo run data get storage demo:vars f.v_iter[0]
data remove storage demo:vars f.v_iter[0]
scoreboard players operation #f.v_total demo += #f.v_x demo
return run function demo:f/b0
```

## Strings

A string is a value in command storage. `+` and f-strings that are used as values build a new string with one *macro* command (a command line starting with `$` whose `$(h0)`-style holes are filled from `demo:macro`):

```slab show=f,f/m0
fn f(name: str, n: int) {
    let s = "hello " + name;
    let t = f"{name} has {n} points";
}
```
```mcfunction
# function demo:f
data modify storage demo:macro h0 set from storage demo:vars f.v_name
function demo:f/m0 with storage demo:macro
data remove storage demo:macro h0
data modify storage demo:vars f.v_s set from storage demo:vars f.t_0
data remove storage demo:vars f.t_0
data modify storage demo:macro h0 set from storage demo:vars f.v_name
execute store result storage demo:macro h1 int 1 run scoreboard players get #f.v_n demo
function demo:f/m1 with storage demo:macro
data remove storage demo:macro h0
data remove storage demo:macro h1
data modify storage demo:vars f.v_t set from storage demo:vars f.t_0
data remove storage demo:vars f.t_0
function demo:f/_drop

# function demo:f/m0
$data modify storage demo:vars f.t_0 set value "hello $(h0)"
```

`print(f"...")` does not use macros: it is one `tellraw` whose parts are text, `score` and `nbt` components read when the message is sent.

```slab show=f
fn f(name: str, n: int) {
    print(f"{name} has {n} points");
}
```
```mcfunction
# function demo:f
tellraw @a {text:"",extra:[{nbt:"f.v_name",storage:"demo:vars"}," has ",{score:{name:"#f.v_n",objective:"demo"}}," points"]}
data remove storage demo:vars f.v_name
```

## Lists and structs

Lists and structs are NBT in storage. Literals made only of constants are a single `data modify ... set value`.

```slab show=f
struct Point { x: int, name: str }

fn f() {
    let mut p = Point { x: 3, name: "a" };
    let mut xs = [1, 2, 3];
    p.x = 9;
    xs.push(4);
}
```
```mcfunction
# function demo:f
data modify storage demo:vars f.v_p set value {x:3,name:"a"}
data modify storage demo:vars f.v_xs set value [1,2,3]
data modify storage demo:vars f.v_p.x set value 9
data modify storage demo:vars f.v_xs append value 4
function demo:f/_drop
```

Reading a field into an `int` is `execute store result score ... run data get`. A **variable** index cannot be written into the command, so it becomes a macro: the index is stored in `demo:macro` and a helper function does the access.

```slab show=f,f/m0
fn f(i: int) -> int {
    let xs = [10, 20, 30];
    return xs[i];
}
```
```mcfunction
# function demo:f
data modify storage demo:vars f.v_xs set value [10,20,30]
execute store result storage demo:macro h0 int 1 run scoreboard players get #f.v_i demo
function demo:f/m0 with storage demo:macro
data remove storage demo:macro h0
execute store result score #ret demo run data get storage demo:vars f.v_tmp
function demo:f/_drop
return 1

# function demo:f/m0
$data modify storage demo:vars f.v_tmp set from storage demo:vars f.v_xs[$(h0)]
```

## Enums

An enum value is a compound in storage: `t` is the variant's index in declaration order (`None` is 0, `Some` is 1; `Ok` is 0, `Err` is 1) and `f0`, `f1`, ... are the payload values. A `()` payload stores nothing. Constant values are one literal; a payload only known at run time is filled in afterwards, as for a struct.

```slab show=f
enum Shape { Dot, Line(int, int), Named(str) }

fn f(n: int) {
    let a: Option<int> = None;
    let b = Some(7);
    let c = Some(n + 1);
    let d = Shape::Line(1, n);
    let e: Result<(), str> = Ok(());
    let g: Result<(), str> = Err("no");
}
```
```mcfunction
# function demo:f
data modify storage demo:vars f.v_a set value {t:0}
data modify storage demo:vars f.v_b set value {t:1,f0:7}
data modify storage demo:vars f.v_tmp set value {t:1}
scoreboard players operation #f.t_0 demo = #f.v_n demo
scoreboard players add #f.t_0 demo 1
execute store result storage demo:vars f.t_0 int 1 run scoreboard players get #f.t_0 demo
data modify storage demo:vars f.v_tmp.f0 set from storage demo:vars f.t_0
data remove storage demo:vars f.t_0
data modify storage demo:vars f.v_c set from storage demo:vars f.v_tmp
data modify storage demo:vars f.v_tmp_2 set value {t:1}
data modify storage demo:vars f.v_tmp_2.f0 set value 1
execute store result storage demo:vars f.t_0 int 1 run scoreboard players get #f.v_n demo
data modify storage demo:vars f.v_tmp_2.f1 set from storage demo:vars f.t_0
data remove storage demo:vars f.t_0
data modify storage demo:vars f.v_d set from storage demo:vars f.v_tmp_2
data modify storage demo:vars f.v_e set value {t:0}
data modify storage demo:vars f.v_g set value {t:1,f0:"no"}
function demo:f/_drop
```

Enums compare like any stored value, and a global's initial value is written the same way:

```slab show=f,_init
global start: Option<int> = Some(3);

fn f(o: Option<int>) -> bool {
    return o == Some(3);
}
```
```mcfunction
# function demo:f
data modify storage demo:vars f.t_0 set from storage demo:vars f.v_o
execute store success score #f.t_0 demo run data modify storage demo:vars f.t_0 set value {t:1,f0:3}
data remove storage demo:vars f.t_0
execute store success score #ret demo unless score #f.t_0 demo matches 1
data remove storage demo:vars f.v_o
return 1

# function demo:_init
# generated by slab: runs on every load; initialises state once
scoreboard objectives add demo dummy
execute unless data storage demo:globals start run data modify storage demo:globals start set value {t:1,f0:3}
```

### `match` and `if let`

Without guards, a `match` is an `if`/`else` chain: the first arm whose pattern fits runs. The tag `t` of the enum is read, payloads are copied into the arm's bindings, and the last arm of an exhaustive match needs no test. Used as a value, the chosen arm assigns a temporary.

```slab show=pick
fn pick(o: Option<int>) -> int {
    return match o {
        Some(v) => v + 1,
        None => 0,
    };
}
```
```mcfunction
# function demo:pick
scoreboard players set #pick.v_m demo 0
execute store result score #pick.t_0 demo run data get storage demo:vars pick.v_o.t
execute store success score #pick.t_1 demo if score #pick.t_0 demo matches 1
execute if score #pick.t_1 demo matches 1 run function demo:pick/b0
execute unless score #pick.t_1 demo matches 1 run function demo:pick/b1
scoreboard players operation #ret demo = #pick.v_m demo
data remove storage demo:vars pick.v_o
return 1
```

With a guard, a flag records that an arm already ran, because the guard is only known after the bindings exist:

```slab show=big
fn big(o: Option<int>) -> str {
    return match o {
        Some(v) if v > 9 => "big",
        Some(_) => "small",
        None => "none",
    };
}
```
```mcfunction
# function demo:big
data modify storage demo:vars big.v_m set value ""
scoreboard players set #big.v_done demo 0
execute store result score #big.t_0 demo run data get storage demo:vars big.v_o.t
execute if score #big.v_done demo matches 0 if score #big.t_0 demo matches 1 run function demo:big/b0
execute store result score #big.t_0 demo run data get storage demo:vars big.v_o.t
execute if score #big.v_done demo matches 0 if score #big.t_0 demo matches 1 run function demo:big/b2
execute store result score #big.t_0 demo run data get storage demo:vars big.v_o.t
execute if score #big.v_done demo matches 0 if score #big.t_0 demo matches 0 run function demo:big/b3
data modify storage demo:ret v set from storage demo:vars big.v_m
function demo:big/_drop
return 1
```

`int`, `bool` and `str` patterns compare with `==` (a `str` against a literal is one `execute if data` test); `A | B` is `||` of the tests. `if let P = v { A } else { B }` is the same as `match v { P => A, _ => B }`.

## Methods

A method is an ordinary function named after its type (`Point::sum` is `point__sum`) whose first parameter is `self`. `p.sum()` and `Point::sum(p)` are the same call: the receiver is copied into `self`, like any argument.

```slab show=f,point__new,point__sum
struct Point { x: int, y: int }

impl Point {
    fn new(x: int, y: int) -> Point { return Point { x: x, y: y }; }
    fn sum(self) -> int { return self.x + self.y; }
}

fn f() -> int {
    let p = Point::new(1, 2);
    return p.sum();
}
```
```mcfunction
# function demo:f
scoreboard players set #point__new.v_x demo 1
scoreboard players set #point__new.v_y demo 2
function demo:point__new
data modify storage demo:vars f.t_0 set from storage demo:ret v
data remove storage demo:ret v
data modify storage demo:vars f.v_p set from storage demo:vars f.t_0
data remove storage demo:vars f.t_0
data modify storage demo:vars point__sum.v_self set from storage demo:vars f.v_p
function demo:point__sum
data remove storage demo:vars f.v_p
return 1

# function demo:point__new
data modify storage demo:vars point__new.v_tmp set value {}
execute store result storage demo:vars point__new.t_0 int 1 run scoreboard players get #point__new.v_x demo
data modify storage demo:vars point__new.v_tmp.x set from storage demo:vars point__new.t_0
data remove storage demo:vars point__new.t_0
execute store result storage demo:vars point__new.t_0 int 1 run scoreboard players get #point__new.v_y demo
data modify storage demo:vars point__new.v_tmp.y set from storage demo:vars point__new.t_0
data remove storage demo:vars point__new.t_0
data modify storage demo:ret v set from storage demo:vars point__new.v_tmp
data remove storage demo:vars point__new.v_tmp
return 1

# function demo:point__sum
execute store result score #point__sum.t_0 demo run data get storage demo:vars point__sum.v_self.x
execute store result score #point__sum.t_1 demo run data get storage demo:vars point__sum.v_self.y
scoreboard players operation #ret demo = #point__sum.t_0 demo
scoreboard players operation #ret demo += #point__sum.t_1 demo
data remove storage demo:vars point__sum.v_self
return 1
```

### Option and Result

`is_some`, `is_ok`, ... read the variant tag `t` into a score and compare it with the variant's index. `unwrap_or(d)` evaluates the default, then takes the stored value when there is one:

```slab show=f,f/b0
fn f(o: Option<int>) -> int {
    return o.unwrap_or(9);
}
```
```mcfunction
# function demo:f
scoreboard players set #f.v_or demo 9
execute store result score #f.t_0 demo run data get storage demo:vars f.v_o.t
execute if score #f.t_0 demo matches 1 run function demo:f/b0
scoreboard players operation #ret demo = #f.v_or demo
data remove storage demo:vars f.v_o
return 1

# function demo:f/b0
execute store result score #f.v_or demo run data get storage demo:vars f.v_o.f0
```

`unwrap()` and `expect(msg)` check first. When the value is missing, the failure branch tells the players in chat (the line of your code is in the message), stops the pack if the program was built with `--abort-on-unwrap` (`datapack disable "file/<namespace>"`), and returns from the function with a zero value. Inside a context block, which cannot be left, it only reports.

```slab show=f,f/b0
fn f(o: Option<int>) -> int {
    return o.unwrap();
}
```
```mcfunction
# function demo:f
execute store result score #f.t_0 demo run data get storage demo:vars f.v_o.t
execute unless score #f.t_0 demo matches 1 if function demo:f/b0 run return 1
execute store result score #ret demo run data get storage demo:vars f.v_o.f0
data remove storage demo:vars f.v_o
return 1

# function demo:f/b0
tellraw @a {text:"",extra:["slab: unwrap on None (main.slab:2)"]}
scoreboard players set #ret demo 0
data remove storage demo:vars f.v_o
return 1
```

`ok()` and `err()` turn a `Result` into an `Option`:

```slab show=f,f/b0
fn f(r: Result<int, str>) -> Option<int> {
    return r.ok();
}
```
```mcfunction
# function demo:f
data modify storage demo:vars f.v_opt set value {t:0}
execute store result score #f.t_0 demo run data get storage demo:vars f.v_r.t
execute if score #f.t_0 demo matches 0 run function demo:f/b0
data modify storage demo:ret v set from storage demo:vars f.v_opt
function demo:f/_drop
return 1

# function demo:f/b0
data modify storage demo:vars f.v_opt set value {t:1}
data modify storage demo:vars f.v_opt.f0 set from storage demo:vars f.v_r.f0
```

## Minecraft commands

A call whose arguments are constants is the plain command:

```slab show=f
fn f() {
    say("hello");
    tp(@s, 1, 64, ~2);
    setblock(0, 64, 0, "minecraft:gold_block");
    effect::give(@s, "minecraft:speed", 10s, 1);
    tag::add(@s, "seen");
    kill(@e[type=zombie, distance=..10]);
}
```
```mcfunction
# function demo:f
say hello
teleport @s 1 64 ~2
setblock 0 64 0 minecraft:gold_block
effect give @s minecraft:speed 10 1
tag @s add seen
kill @e[type=minecraft:zombie,distance=..10]
```

When an argument is only known at run time (a variable coordinate, text, a number), the command line is a *macro*. The compiler stores the values under `h0`, `h1`, ... in `demo:macro`, calls a generated helper function `with storage demo:macro`, and removes the keys again:

```slab show=f,f/m0
fn f(x: int, block: str) {
    setblock(x, 64, 0, block);
}
```
```mcfunction
# function demo:f
execute store result storage demo:macro h0 int 1 run scoreboard players get #f.v_x demo
data modify storage demo:macro h1 set from storage demo:vars f.v_block
function demo:f/m0 with storage demo:macro
data remove storage demo:macro h0
data remove storage demo:macro h1
data remove storage demo:vars f.v_block

# function demo:f/m0
$setblock $(h0) 64 0 $(h1)
```

### Did the command work?

Commands that act on the world return a `bool`. If you **ignore** it, the plain command is emitted and nothing is checked (no cost). If you **use** it, the command runs under `execute store success`:

```slab show=ignored,used
fn ignored() {
    tp(@s, 1, 64, 2);
}

fn used() -> bool {
    return tp(@s, 1, 64, 2);
}
```
```mcfunction
# function demo:ignored
teleport @s 1 64 2

# function demo:used
scoreboard players set #used.t_0 demo 0
execute store success score #used.t_0 demo run teleport @s 1 64 2
scoreboard players operation #ret demo = #used.t_0 demo
return 1
```

### Any command: `run_command`

`run_command(text)` runs a command written as text (no leading `/`) for what slab has no function for. Its value is a `Result<int, ()>`: `Ok(n)` with the command's result when it worked, `Err(())` when it failed. Ignored, it is the bare command; used, one `execute` stores both the success and the result:

```slab show=ignored,used
fn ignored() {
    run_command("time set day");
}

fn used() -> int {
    let r = run_command("scoreboard players get #x demo");
    return r.unwrap_or(0 - 1);
}
```
```mcfunction
# function demo:ignored
time set day

# function demo:used
execute store success score #used.v_ok demo store result score #used.v_val demo run scoreboard players get #x demo
data modify storage demo:vars used.v_res set value {t:1}
execute unless score #used.v_ok demo matches 0 run function demo:used/b0
data modify storage demo:vars used.v_r set from storage demo:vars used.v_res
scoreboard players set #used.v_or demo -1
execute store result score #used.t_0 demo run data get storage demo:vars used.v_r.t
execute if score #used.t_0 demo matches 0 run function demo:used/b1
scoreboard players operation #ret demo = #used.v_or demo
function demo:used/_drop
return 1
```

Text computed at run time (`run_command(f"...")`) becomes a macro line that is the whole command, so a value that is not a valid command only fails in game. A successful command whose result is 0 is still `Ok(0)`.

## Context blocks

`as`, `at`, `positioned`, ... are `/execute` sub-commands. The body is its own function, called once per matching entity.

```slab show=f,f/b0
fn f() {
    as @a[tag=ready] at @s {
        effect::give(@s, "minecraft:speed", 10s);
        tp(@s, ~, ~1, ~);
    }
}
```
```mcfunction
# function demo:f
execute as @a[tag=ready] at @s run function demo:f/b0

# function demo:f/b0
effect give @s minecraft:speed 10
teleport @s ~ ~1 ~
```

## Entities

An `EntityId` value is a number: an id stored as a score named `demo_id` on the entity itself. The first time an entity becomes a value it is given the next free id. Acting on it addresses `@e[scores={demo_id=N},limit=1]` with `N` filled in through a macro. `_ensure_id` and `_alloc_id` are small generated helpers.

```slab show=f,f/m0
fn f() {
    let cow = summon("minecraft:cow", 0, 64, 0);
    tp(cow, 0, 70, 0);
}
```
```mcfunction
# function demo:f
scoreboard players set #new_id demo 0
execute positioned 0 64 0 summon minecraft:cow run function demo:_new_entity
scoreboard players operation #f.v_cow demo = #new_id demo
execute store result storage demo:macro h0 int 1 run scoreboard players get #f.v_cow demo
function demo:f/m0 with storage demo:macro
data remove storage demo:macro h0

# function demo:f/m0
$teleport @e[scores={demo_id=$(h0)},limit=1] 0 70 0
```

```slab show=_ensure_id,_alloc_id,_new_entity,_init
fn f() {
    let cow = summon("minecraft:cow", 0, 64, 0);
}
```
```mcfunction
# function demo:_ensure_id
# generated by slab: the executing entity's id, allocated on first use
execute unless score @s demo_id matches 1.. run function demo:_alloc_id
scoreboard players operation #new_id demo = @s demo_id

# function demo:_alloc_id
# generated by slab: give the executing entity the next entity id
scoreboard players add #next_id demo 1
scoreboard players operation @s demo_id = #next_id demo

# function demo:_new_entity
# generated by slab: a new entity gets an id and is marked as ours
function demo:_alloc_id
tag @s add demo.owned
scoreboard players operation #new_id demo = @s demo_id

# function demo:_init
# generated by slab: runs on every load; initialises state once
scoreboard objectives add demo dummy
scoreboard objectives add demo_id dummy
execute unless score #next_id demo matches -2147483648..2147483647 run scoreboard players set #next_id demo 0
```

A selector that matches one entity turns into a value the same way, and `for e in @selector` runs its body once per match, with `e` bound to the entity:

```slab show=f,f/b0
fn f() {
    let me = @p;
    for e in @e[type=zombie] {
        kill(e);
    }
}
```
```mcfunction
# function demo:f
scoreboard players set #new_id demo 0
execute as @p run function demo:_ensure_id
scoreboard players operation #f.v_me demo = #new_id demo
execute as @e[type=minecraft:zombie] run function demo:f/b0

# function demo:f/b0
scoreboard players set #new_id demo 0
execute as @s run function demo:_ensure_id
scoreboard players operation #f.v_e demo = #new_id demo
execute store result storage demo:macro h0 int 1 run scoreboard players get #f.v_e demo
function demo:f/m0 with storage demo:macro
data remove storage demo:macro h0
```

### Variables in selectors

`$name` and `${expression}` in a selector option are macro holes too:

```slab show=f,f/m0
fn f(tag: str, r: int) {
    kill(@e[tag=$tag, distance=..${r * 2}]);
}
```
```mcfunction
# function demo:f
scoreboard players operation #f.v_hv demo = #f.v_r demo
scoreboard players operation #f.v_hv demo *= #c_2 demo
data modify storage demo:macro h0 set from storage demo:vars f.v_tag
execute store result storage demo:macro h1 int 1 run scoreboard players get #f.v_hv demo
function demo:f/m0 with storage demo:macro
data remove storage demo:macro h0
data remove storage demo:macro h1
data remove storage demo:vars f.v_tag

# function demo:f/m0
$kill @e[tag=$(h0),distance=..$(h1)]
```

## Equality

`int`, `bool` and `EntityId` values compare as scores. Everything stored in command storage (strings, lists, structs) compares by contents. A stored string against a literal is one `execute if data` test:

```slab show=f
fn f(a: str) -> bool {
    return a == "hi";
}
```
```mcfunction
# function demo:f
execute store success score #ret demo if data storage demo:vars {f:{v_a:"hi"}}
data remove storage demo:vars f.v_a
return 1
```

Anything else uses the fact that `data modify ... set from` **fails when it changes nothing**: the left value is copied to a scratch place and set from the right one; `store success` is then 1 exactly when they differ.

```slab show=f
fn f(a: list<int>, b: list<int>) -> bool {
    return a == b;
}
```
```mcfunction
# function demo:f
data modify storage demo:vars f.t_0 set from storage demo:vars f.v_a
execute store success score #f.t_0 demo run data modify storage demo:vars f.t_0 set from storage demo:vars f.v_b
data remove storage demo:vars f.t_0
execute store success score #ret demo unless score #f.t_0 demo matches 1
function demo:f/_drop
return 1
```

### Queries

`exists(sel)` and `tag::has` are `execute if entity` tests (the tag becomes part of the selector); they never change anything.

```slab show=f
fn f() -> bool {
    return exists(@e[type=creeper]) && tag::has(@s, "boss");
}
```
```mcfunction
# function demo:f
scoreboard players set #ret demo 0
execute store success score #ret demo if entity @e[type=minecraft:creeper] if entity @s[tag=boss]
return 1
```

## Modules

Every file is part of one pack. A function `double` in module `util` is the function `util__double`, a global gets the same prefix, and an objective declared outside `main.slab` is prefixed too.

```slab file=util.slab
pub fn double(x: int) -> int { return x * 2; }
```
```slab show=boot,util__double
import util;

#[load]
fn boot() {
    print(util::double(4));
}
```
```mcfunction
# function demo:boot
scoreboard players set #util__double.v_x demo 4
function demo:util__double
scoreboard players operation #boot.t_0 demo = #ret demo
tellraw @a {score:{name:"#boot.t_0",objective:"demo"}}

# function demo:util__double
scoreboard players operation #ret demo = #util__double.v_x demo
scoreboard players operation #ret demo *= #c_2 demo
return 1
```

## Scoreboard objectives

```slab show=f,_init
objective points = "dummy";

fn f() {
    points[@s] += 1;
    let mine = points[@s];
    points["#total"] = mine;
}
```
```mcfunction
# function demo:f
scoreboard players add @s points 1
execute store result score #f.v_mine demo run scoreboard players get @s points
scoreboard players operation #total points = #f.v_mine demo

# function demo:_init
# generated by slab: runs on every load; initialises state once
scoreboard objectives add demo dummy
scoreboard objectives add points dummy
```

## Globals and entry points

A global lives in the scoreboard (`#g.name`) or in `demo:globals`. `_init` is in the `#minecraft:load` function tag: on every load and `/reload` it creates the objectives, sets each global the first time (so a global keeps its value across `/reload`), and then calls your `#[load]` functions. `#[tick]` functions are in the `#minecraft:tick` tag.

```slab show=_init,boot,counter
global ticks: int = 0;
global greeting: str = "hi";

#[load]
fn boot() {
    ticks = 100;
}

#[tick]
fn counter() {
    ticks += 1;
}
```
```mcfunction
# function demo:_init
# generated by slab: runs on every load; initialises state once
scoreboard objectives add demo dummy
execute unless score #g.ticks demo matches -2147483648..2147483647 run scoreboard players set #g.ticks demo 0
execute unless data storage demo:globals greeting run data modify storage demo:globals greeting set value "hi"
function demo:boot

# function demo:boot
scoreboard players set #g.ticks demo 100

# function demo:counter
scoreboard players add #g.ticks demo 1
```

`#[every(1s)]` is a counter global plus a tick function that calls yours when the counter reaches the period (here 20 ticks):

```slab show=heartbeat,heartbeat__every_tick,heartbeat__every_tick/b0
global beats: int = 0;

#[every(1s)]
fn heartbeat() {
    beats += 1;
}
```
```mcfunction
# function demo:heartbeat
scoreboard players add #g.beats demo 1

# function demo:heartbeat__every_tick
scoreboard players add #g.heartbeat__every demo 1
execute if score #g.heartbeat__every demo matches 20.. run function demo:heartbeat__every_tick/b0

# function demo:heartbeat__every_tick/b0
scoreboard players set #g.heartbeat__every demo 0
function demo:heartbeat
```

## Cleanup

String, list and struct locals are removed from storage when the function ends (`<fn>/_drop` when there are several), temporaries as soon as they are not needed, and `demo:_uninstall` removes everything the pack created: objectives, storage, summoned entities and the tags you added. See `docs/lang/memory.md`.

```slab show=_uninstall
objective points = "dummy";
global seen: int = 0;

fn f() {
    let e = summon("minecraft:cow", 0, 64, 0);
    tag::add(e, "seen");
}
```
```mcfunction
# function demo:_uninstall
# generated by slab: removes all state this pack created. Run it, then `/datapack disable` the pack.
kill @e[tag=demo.owned]
tag @e[tag=seen] remove seen
scoreboard objectives remove points
scoreboard objectives remove demo_id
scoreboard objectives remove demo
data remove storage demo:vars f
data remove storage demo:ret v
data remove storage demo:stack i
data remove storage demo:stack d
```
