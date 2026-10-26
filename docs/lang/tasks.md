# Tasks: loops that take many ticks

Minecraft stops a function after **65,536 commands in one tick** (`max_command_sequence_length`),
silently. A loop that does real work (a ring of 360 markers, a scan of a big area) runs into that
long before it is done. A `task fn` is a function that is allowed to take as many ticks as it needs:
its loops run a *slice* per tick.

```slab
task fn create_ring(center: Entity) {
    let mut angle = 0;
    while angle < 360 {
        be center {
            let marker = summon("minecraft:marker");
            marker.set_rotation(NearDouble::from_int(angle), 0);
            marker.move_local(0, 0, 5);
        }
        angle += 1;
    }
    print("ring done");            // runs when the loop is done, a few ticks after `start`
}

#[load]
fn boot() {
    for player in @p { start create_ring(player); }    // returns at once
}
```

* `start name(args);` launches the task and returns immediately; the first slice runs on the next
  tick. A task cannot be called like a function, and it returns nothing (nobody is waiting for it).
* **One run at a time.** Starting a task that is still running begins it over (the arguments of
  the new `start` are used); the old run's locals are overwritten.
* A task can `start` other tasks, call ordinary functions, use `be`/`as` blocks, `return;` to finish
  early, and have as many loops as it likes, nested or in a row, with `break` and `continue`.
* `task` and `start` are words only where they mean something (`task fn`, `start name(..)`), so
  they remain usable as names.
* Not (yet): a loop, `break` or `continue` inside a `match` of a task (use `if`/`else`; it is an
  error that says so), and a task with an `#[load]`/`#[tick]` attribute (start it from one).
  Loops inside a `be`/`as` block of a task are ordinary loops: they are not sliced, since that block
  may run once per entity.

## How it runs

The body becomes a **state machine** inside one function, `<task>__step__run`, so the locals keep
their slots between ticks (`codegen/task.rs` has the full account):

```text
budget = <task-budget>
while true {
    if budget <= 0 { schedule function <task>__step 1t; suspend }    # suspend: return, keep the locals
    if state == 0 { ..code up to the first loop..          ; budget -= its cost }
    if state == 1 { ..a loop, one trip at a time, checking the budget at the top of each trip.. }
    if state == 2 { ..code after the loop.. }
}
```

* Code between loops, and loops that contain other loops, are **blocks** that set `state` (a
  control-flow graph; `break`/`continue` are jumps).
* A loop with no loop inside it is one block that is an ordinary loop with a budget check at the
  top of each trip. When the budget is gone it schedules the next slice and returns *without*
  cleaning up; the next tick enters the same block again. The set-up of the loop (the counter of a
  `for`, the copy of a list) is in the block before, so a resumed task does not run it twice.
* `start` copies the arguments into globals (`<task>__arg_<name>`), sets `state` to 0, `running`
  to 1 and schedules `<task>__step`. The step copies them into locals when the first block runs.

## The budget is measured

`task-budget` (default **30,000** commands per tick, `[build] task-budget` in `Slab.toml` or
`--task-budget N`) is how much of a tick one task may use, leaving room for other code. What a trip
of a loop, or a block, costs is **measured, not guessed**: the first code generation pass numbers
them, `compile.rs` lowers the pack and measures each with the estimator of `src/cost.rs`, and a
second pass writes the numbers into the `budget -= cost` constants (only constants change, so the
measurements stay right). The measure counts the commands of every branch that can run together (of an `if`/`else`, only the dearer
side), so a slice uses somewhat less than the budget (the ring above: about 35 trips per tick, around
10,000 of the 30,000 commands). That is deliberate: too little only makes a task a little slower, too much would be cut
off by the game.

If a slice does hit the limit anyway (a block with a loop inside a `be` block, say), the guard that
every entry point and task step has (`docs/lang/codegen.md`, "Telling when the limit was hit")
reports it in chat.

## What it costs

* A task step adds a few commands per tick while it runs (the guard, the dispatcher's `if state == k`
  for each block) and nothing when it is idle (no function is scheduled).
* Every loop trip has two extra commands (the budget check and the subtraction).
* `start` is a handful of commands.

Verified on a real server (`examples/projects/e2e`): a task counting to 30,000, nested loops with
`break`/`continue`, an early `return`, a restart, and a task creating 200 markers with library calls
(more than 76,000 commands in total) all finish with the right answers and no cut-off.
