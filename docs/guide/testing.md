# Testing in a real server

The compiler can check your program, but not what Minecraft does with it: whether the zombie
really spawns, whether a command fails, whether the server logs an error that nobody sees in
game. `slab test` runs your tests **inside a real Minecraft server** that it sets up for you, in a
world you control.

```slab
import std::*;
import game::*;
import testing::*;

#[test]
fn zombie_is_summoned_and_removed() {
    let zombie = summon("minecraft:zombie", 0, -60, 0);
    assert(exists(zombie), "the zombie exists");
    wait(10);                                   // let 10 ticks (half a second) pass
    kill(zombie);
    wait(2);
    assert(!exists(zombie), "the zombie is gone");
}
```

Put tests anywhere in your project (the same file as the code they test, or a `tests.slab` module)
and run them:

```text
slab test --accept-eula
```

```text
running 1 test of `my_pack` on Minecraft 26.3 (server log: build/test-server/logs/latest.log)
  ok      zombie_is_summoned_and_removed  (0.7s)

test result: ok. 1 passed
```

`slab build` leaves tests out of your pack; only `slab test` includes them.

## What you need

* **Java 25 or newer**, which Minecraft 26.x requires ([adoptium.net](https://adoptium.net)). Use
  `--java PATH` if it is not the `java` on your `PATH`.
* **The Minecraft server**, which slab downloads once from Mojang (with `curl`, checked against
  Mojang's checksum) and keeps in a cache folder. Downloading it means you accept the
  [Minecraft EULA](https://aka.ms/MinecraftEULA), so slab asks you to say so: pass `--accept-eula`
  the first time, or put `accept-eula = true` under `[test]` in `Slab.toml`, or set the
  environment variable `SLAB_ACCEPT_EULA=1` (handy in CI). `--jar PATH` uses a server jar you
  already have.

## Writing tests

A test is a function marked `#[test]` with no parameters. It can use everything your program can:
call your functions, summon entities, place blocks, read scoreboards, run commands.

| `testing::` | |
|---|---|
| `assert(condition, "message")` | fails the test with the message unless the condition holds |
| `assert_eq(actual, expected, "message")` | the same for two numbers; the report shows both |
| `fail("message")` | fails the test |
| `wait(20)` | lets that many game ticks pass (20 per second) before the next line runs |

* **A failed assertion does not stop the test**, so one run shows every problem.
* **`wait`** needs a number written out (`wait(20)`, not `wait(n)`). It works in `#[test]` functions
  and in a [`task fn`](../lang/tasks.md), and not inside an `as`/`at`/`be` block. Without a `wait`,
  a test runs inside a single tick.
* Tests run **one after the other**, in the order they are written.

## Catching failed commands and heavy ticks

Two options of `#[test(..)]`, each for one test at a time (nothing is checked for the others):

```slab
import std::*;
import game::*;
import testing::*;

#[test(strict_commands, max_commands(20000))]
fn the_guards_arrive() {
    let guard = summon("minecraft:zombie", 0, -60, 0);
    tag::add(guard, "guard");
    assert(exists(guard), "the guard is there");
}
```

* **`strict_commands`**: Minecraft does not log a command that fails inside a function (nobody to
  `kill`, a tag that is already there, a block that did not change): it only counts as "failed", and
  slab lets that pass unless you look at it. In a strict test, every command whose result your code
  ignores must have worked, in the test **and in everything it calls**, or the test fails and says where:
  `command failed: main.slab:6: kill`. Commands whose result you use (`let ok = kill(..)`,
  `run_command(..).is_ok()`) are yours to judge and are never reported. Only a test with this option pays
  for the extra checks, and only in the test build.
* **`max_commands(N)`**: Minecraft stops a function chain after 65,536 commands in one tick, and **every test already fails
  when that happens** (`Command execution stopped due to limit`). With this option the limit for this
  test is `N` instead, so a function that should stay cheap fails the moment one tick costs more. The
  count includes the test's own commands between two `wait`s. It does not change other tests.

Combine them with a comma. They cannot hide a failure for other tests: each test starts with the strict
check off and the normal command limit.

## The world a test starts in

Before **every** test slab resets everything, so tests cannot affect each other:

* the pack is reset to just after it loaded: its scoreboards and storage are removed and made again
  by running its `_init` (so globals have their start values, and your `#[load]` functions have run
  again);
* the **arena** is empty: a flat world, grass at y = -61 (so things stand at y = -60), the 17 x 17
  blocks around `0 0` cleared of everything above the ground and put back below it, and every entity
  that is not a player removed;
* time and weather do not move, and mobs do not spawn by themselves (the `advance_time`,
  `advance_weather` and `spawn_mobs` rules are off). The chunks around the origin are always loaded.

Keep a test's blocks and entities inside the arena (x and z from -8 to 8). Your `#[tick]` and
`#[every]` functions keep running while a test waits, so a test can check them too:

```slab
import std::*;
import game::*;
import testing::*;

global ticks: int = 0;

#[tick]
fn count() {
    ticks += 1;
}

#[test]
fn ticks_pass_while_a_test_waits() {
    let before = ticks;
    wait(20);
    assert(ticks - before >= 19, "the tick function ran about once per tick");
}
```

## What fails a test

A test fails when

* an assertion fails;
* it never reaches its end within the **timeout** (60 seconds; `--timeout SECS`, or `timeout = 30` under
  `[test]`), for instance because it waits too long;
* the **server logs an error while it runs**: an `ERROR` line, a function that failed to load, an
  exception, or `Command execution stopped due to limit` (the 65,536 commands per tick that Minecraft
  allows, see [tasks](../lang/tasks.md)). The report shows the line and the top of its stack trace.

If the pack cannot even load (a function with a command the game rejects), no test is run and the
errors are printed: that is the same kind of error, found before it could confuse a test.

Be aware that Minecraft does **not** log everything: a command that fails inside a function (nobody
has that score, a computed command that does not parse) just returns "failed" without a log line.
Check what you care about: `run_command("...").is_ok()`, `exists(...)`, a score you read back.

## The command line

```text
slab test [PATH] [--filter TEXT] [--list] [--timeout SECS] [--java PATH] [--jar PATH] [--accept-eula] [-v]
```

`--filter` runs the tests whose name contains the text, `--list` shows them without starting a
server, `-v` prints the server's log for every test. The server lives in `build/test-server/` of the
project (its `logs/latest.log` has everything the game said); it is rebuilt from scratch on every run.
The exit code is 0 when every test passed and 1 otherwise, so `slab test` can run in CI:

```text
# .github/workflows/test.yml (the relevant steps)
- uses: actions/setup-java@v4
  with: { distribution: temurin, java-version: 25 }
- run: slab test --accept-eula
```

## The same run every time

A problem you cannot repeat is a problem you cannot fix, so `slab test` takes the things it can
control out of the picture:

* **The world:** a flat world with a fixed **seed** (`--seed N`, or `seed` under `[test]`; default 0), the
  sun at noon, clear weather, difficulty normal, no natural spawning.
* **Random numbers:** before each test every random sequence starts over, so `random value 1..100 my_sequence`
  gives the same numbers in the same order every time.
* **Time:** while a test runs, the game is **frozen** and slab moves it forward a few ticks at a time
  (`tick sprint`), as fast as the server can. So a test sees the same ticks whether the computer is fast
  or slow or busy, and a test that waits 20 ticks takes a fraction of a second. `--realtime` lets time
  run by itself instead (ticks then depend on the speed of the machine).
* **The start:** the pack and the arena are reset before every test, as described above.

What is not controlled is the game's own randomness in mobs' behaviour (where a zombie decides to
walk). If a test depends on it, make the test not depend on it, for instance with `/random` sequences
in your own code, or by checking the outcome and not the path.

## Looking at it yourself

When a test fails, the report ends with the command to see it for yourself:

```text
slab test --filter ticks_pass_while_a_test_waits --hold
```

`--hold` runs the tests and then **keeps the server running**. slab prints an address
(`localhost:25565`): join it from Minecraft Java (multiplayer, direct connection, any name; you start
in creative mode and can fly) and look at the world the test left. `--serve` starts the same server with
your pack and **no tests**, to experiment in.

The terminal where `slab test` runs is the server's console. What you type goes to the game
(`time query gametime`, `summon minecraft:zombie 0 -60 0`, `scoreboard players get ...`), and lines
starting with `!` are slab's own:

| | |
|---|---|
| `!tests` | list the tests |
| `!run NAME` | reset the world and the pack, then run the test whose name contains `NAME` |
| `!reset` | reset the world and the pack, as before every test |
| `!step N` | let `N` ticks pass (time stands still while you look, unless `--realtime`) |
| `!freeze` / `!unfreeze` | stop / run time |
| `!quit` | stop the server (so does Ctrl-D) |

Every run also leaves its server in `build/test-server/`: `logs/latest.log` has everything the game
said, and the world is there to open. It is replaced by the next run.

## What it cannot do yet

There are **no players** on this server, so code that needs one (`@a`, `@p`, inventories, anything a
player does) cannot be exercised yet; tests work with entities, blocks, scoreboards, storage and
time. Simulated players are planned.
