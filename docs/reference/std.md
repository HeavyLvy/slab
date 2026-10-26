<!-- generated from src/stdlib/*.slab by `UPDATE_DOCS=1 cargo test --test stdref`; edit the `///` comments there, not this file -->

# Standard library reference

Every function, type and method of `std` and `game`, with the documentation written next to it in the library's source. You can read the same text in your editor: hover a name, or ctrl+click it to open its declaration.

`Target` in a signature means a selector (`@a[tag=x]`) or an `Entity`; `Coord` an `int` expression or a relative coordinate (`~`, `^2`); `Num` a number written out (`0.2`); `const` a value known at compile time.

## `std`

Things that have nothing to do with Minecraft: printing, `Option` and `Result`, and the numbers with decimals. `Option`, `Result`, `Some`, `None`, `Ok` and `Err` need no import; everything else does: `import std::*;` or `import std::print;`.

### `print`

```slab
pub fn print(value: any)
```

Show a message to every player with `tellraw`. It accepts any value; an f-string is shown
part by part (numbers and text keep their natural form), so `print(f"hp = {hp}")` just works.

### `Option`

```slab
pub enum Option<T> {
    None,
    Some(T),
}
```

A value that may be missing: `Some(x)` has one, `None` does not. `Option`, `Some` and `None`
are in scope everywhere. The type argument cannot always be guessed: write
`let x: Option<int> = None;`.

### `Result`

```slab
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

The outcome of something that can fail: `Ok(x)` worked and carries a result, `Err(e)` did not
and carries a reason. `Result`, `Ok` and `Err` are in scope everywhere.

### Methods of `Option`

#### `Option::is_some`

```slab
pub fn is_some(self) -> bool
```

Whether this is `Some`.

#### `Option::is_none`

```slab
pub fn is_none(self) -> bool
```

Whether this is `None`.

#### `Option::unwrap`

```slab
pub fn unwrap(self) -> T
```

The value inside `Some`. On `None` there is nothing to give: the players are told
(`slab: unwrap on None (main.slab:12)` in chat), the pack is stopped when the program was
built with `--abort-on-unwrap`, and the function returns (inside a context block, which
cannot be left, it carries on with a zero value instead).

#### `Option::unwrap_or`

```slab
pub fn unwrap_or(self, default: T) -> T
```

The value inside `Some`, or `default` for `None`. `default` is always evaluated.

#### `Option::expect`

```slab
pub fn expect(self, message: str) -> T
```

Like `unwrap`, with your own message instead of "unwrap on None".

### Methods of `Result`

#### `Result::is_ok`

```slab
pub fn is_ok(self) -> bool
```

Whether this is `Ok`.

#### `Result::is_err`

```slab
pub fn is_err(self) -> bool
```

Whether this is `Err`.

#### `Result::unwrap`

```slab
pub fn unwrap(self) -> T
```

The value inside `Ok`. On `Err` it behaves like `Option::unwrap` on `None`.

#### `Result::unwrap_or`

```slab
pub fn unwrap_or(self, default: T) -> T
```

The value inside `Ok`, or `default` for `Err`. `default` is always evaluated.

#### `Result::expect`

```slab
pub fn expect(self, message: str) -> T
```

Like `unwrap`, with your own message instead of "unwrap on Err".

#### `Result::ok`

```slab
pub fn ok(self) -> Option<T>
```

`Some(value)` for `Ok(value)`, `None` for `Err`.

#### `Result::err`

```slab
pub fn err(self) -> Option<E>
```

`Some(error)` for `Err(error)`, `None` for `Ok`.

### `NearDouble`

```slab
pub struct NearDouble {
    whole: int,
    frac: int,
}
```

A number with 4 decimals, read from the world with no help: `data get` with a scale. Exact
for coordinates within 21,474 blocks of the origin; further out the world gives no more
digits, so `position_near` fails there. All the arithmetic lives here, because the products
fit in 32 bits inside that range.

### `FarDouble`

```slab
pub struct FarDouble {
    whole: int,
    frac: int,
}
```

A number with 6 decimals, read through a scratch marker entity, which keeps the digits
anywhere in the world (up to the border at 29,999,984). It needs the chunk at 0 0 loaded and
costs about ten commands per read. Add, subtract and compare here; multiply after converting
with `to_near`, because a product of big numbers does not fit in 32 bits.

### Methods of `NearDouble`

#### `NearDouble::from_int`

```slab
pub fn from_int(n: int) -> NearDouble
```

A whole number.

#### `NearDouble::from_units`

```slab
pub fn from_units(units: int) -> NearDouble
```

The number of ten-thousandths: `NearDouble::from_units(-65000)` is -6.5.

#### `NearDouble::units`

```slab
pub fn units(self) -> int
```

The number in ten-thousandths (within 214,748 blocks, so it fits an `int`).

#### `NearDouble::floor`

```slab
pub fn floor(self) -> int
```

Rounded down to a whole number.

#### `NearDouble::to_far`

```slab
pub fn to_far(self) -> FarDouble
```

The same number with 6 decimals.

#### `NearDouble::add`

```slab
pub fn add(self, other: NearDouble) -> NearDouble
```

#### `NearDouble::sub`

```slab
pub fn sub(self, other: NearDouble) -> NearDouble
```

#### `NearDouble::neg`

```slab
pub fn neg(self) -> NearDouble
```

#### `NearDouble::mul`

```slab
pub fn mul(self, other: NearDouble) -> NearDouble
```

Product, cut off after the fourth decimal. Exact while the result is within 214,748.

#### `NearDouble::div_int`

```slab
pub fn div_int(self, by: int) -> NearDouble
```

Quotient by a whole number, cut off after the fourth decimal.

#### `NearDouble::lt`

```slab
pub fn lt(self, other: NearDouble) -> bool
```

#### `NearDouble::le`

```slab
pub fn le(self, other: NearDouble) -> bool
```

#### `NearDouble::gt`

```slab
pub fn gt(self, other: NearDouble) -> bool
```

#### `NearDouble::ge`

```slab
pub fn ge(self, other: NearDouble) -> bool
```

#### `NearDouble::text`

```slab
pub fn text(self) -> str
```

The text Minecraft reads as this number: `-6.0005`. Written out in one function with the
common case (not negative) first, because it runs for every number put into a command.

### Methods of `FarDouble`

#### `FarDouble::from_int`

```slab
pub fn from_int(n: int) -> FarDouble
```

A whole number.

#### `FarDouble::floor`

```slab
pub fn floor(self) -> int
```

Rounded down to a whole number.

#### `FarDouble::to_near`

```slab
pub fn to_near(self) -> NearDouble
```

Rounded to 4 decimals. Only meaningful within 21,474 blocks of the origin.

#### `FarDouble::add`

```slab
pub fn add(self, other: FarDouble) -> FarDouble
```

#### `FarDouble::sub`

```slab
pub fn sub(self, other: FarDouble) -> FarDouble
```

#### `FarDouble::neg`

```slab
pub fn neg(self) -> FarDouble
```

#### `FarDouble::lt`

```slab
pub fn lt(self, other: FarDouble) -> bool
```

#### `FarDouble::le`

```slab
pub fn le(self, other: FarDouble) -> bool
```

#### `FarDouble::gt`

```slab
pub fn gt(self, other: FarDouble) -> bool
```

#### `FarDouble::ge`

```slab
pub fn ge(self, other: FarDouble) -> bool
```

#### `FarDouble::text`

```slab
pub fn text(self) -> str
```

The text Minecraft reads as this number: `29999984.567800`.

## `game`

Everything that touches the world: chat, entities, blocks, particles, commands. `import game::*;` brings in all of it, or import single names (`import game::{say, tp};`). A full path always works without an import: `game::say("hi")`.

### `Entity`

```slab
pub struct Entity {}
```

A reference to one entity. It may have died since: every action on a dead entity does
nothing and reports `false`. Create one with `summon`, or from a selector that matches one
entity (`let me = @p;`).

### `say`

```slab
pub fn say(message: str)
```

Show a raw chat message as if typed by the server (`/say`). The text may be computed at
run time.

### `kill`

```slab
pub fn kill(target: Target) -> bool
```

Kill the entities matching the target. `true` when anything was killed.

### `exists`

```slab
pub fn exists(target: Target) -> bool
```

Whether the target matches at least one entity right now.

### `tp`

```slab
pub fn tp(target: Target, x: Coord, y: Coord, z: Coord) -> bool
```

Teleport the matching entities to the coordinates. `true` when it happened.

### `setblock`

```slab
pub fn setblock(x: Coord, y: Coord, z: Coord, block: str) -> bool
```

Place a block at the coordinates (`block` is an id such as `"minecraft:stone"`, computed
or not). `true` when the block was set.

### `summon`

```slab
pub fn summon(kind: str, x: Coord, y: Coord, z: Coord) -> Entity
```

Summon an entity of type `kind` (an id such as `"minecraft:cow"`) at the coordinates, or
where the command runs when they are left out, and give back a handle to it. The entity
is tagged as ours, so `_uninstall` removes it again.

### `particle`

```slab
pub fn particle(kind: str, x: Coord, y: Coord, z: Coord, dx: Num, dy: Num, dz: Num, speed: Num, count: int, mode: const str, viewers: Target) -> bool
```

Show a particle. `kind` is its id (`"minecraft:campfire_signal_smoke"`, or `"end_rod"`).
A particle that takes data gets it as a `{ .. }` right after the kind:

```slab
particle("end_rod");                                          // here
particle("end_rod", ~, ~1, ~);                                // at a position (`^` and `~` work)
particle("item", { item: { id: "minecraft:diamond", count: 1 } }, ~, ~1, ~, 0.2, 0.2, 0.2, 0.1, 20, "force");
particle("dust", { color: [1.0, 0.0, 0.0], scale: 1.5 }, ~, ~, ~, 0, 0, 0, 0, 1);
```

The data is written out with constant values (numbers, strings, `true`/`false`, lists, more
`{ .. }`); a number with a decimal point is a float, which is what `scale` and `color` ask for.
After the position come the spread `dx, dy, dz` (numbers written out, like `0.2`), the
`speed` and the `count`: with a count of 0 one particle flies with the velocity
`delta * speed`, with more they scatter over the delta. Then the `mode` (`"force"` is seen
from 512 blocks, `"normal"` from 32) and the players who see it (`@a[distance=..20]`).
`true` when a player could see it. Without the data the shapes are 1, 4, 9, 10 or 11 arguments.

### `run_command`

```slab
pub fn run_command(command: str) -> Result<int, ()>
```

Run any command, written as text without the leading `/`: `run_command("time set day")`.
The text may be computed at run time. `Ok(n)` carries the command's result (what `/execute
store result` would store: the value of `data get`, the number of entities `kill` killed, ...)
and `Err(())` means the command failed or did not run. Use this for what slab has no
function for yet; it is not checked beyond being one line, so a typo only shows up in game.

### `Vec3i`

```slab
pub struct Vec3i {
    x: int,
    y: int,
    z: int,
}
```

A position in whole blocks.

### `NearVec3`

```slab
pub struct NearVec3 {
    x: NearDouble,
    y: NearDouble,
    z: NearDouble,
}
```

A position with 4 decimals, as `position_near` reads it.

### `Rotation`

```slab
pub struct Rotation {
    yaw: NearDouble,
    pitch: NearDouble,
}
```

Where an entity looks: yaw (0 south, 90 west, -90 east, 180 north) and pitch (-90 up, 90 down).

### Methods of `NearVec3`

#### `NearVec3::add`

```slab
pub fn add(self, o: NearVec3) -> NearVec3
```

#### `NearVec3::sub`

```slab
pub fn sub(self, o: NearVec3) -> NearVec3
```

#### `NearVec3::neg`

```slab
pub fn neg(self) -> NearVec3
```

#### `NearVec3::mul`

```slab
pub fn mul(self, k: NearDouble) -> NearVec3
```

Every component times a number.

### `FarVec3`

```slab
pub struct FarVec3 {
    x: FarDouble,
    y: FarDouble,
    z: FarDouble,
}
```

A position with 6 decimals, as `position_far` reads it.

### Methods of `Entity`

#### `Entity::selector`

```slab
pub fn selector(self) -> str
```

The selector text that finds this entity: `@e[scores={<namespace>_id=7},limit=1]`.
Pass it to `run_command` to do something slab has no function for.

#### `Entity::is_alive`

```slab
pub fn is_alive(self) -> bool
```

Whether the entity is still there (it may have died or been unloaded).

#### `Entity::position`

```slab
pub fn position(self) -> Result<Vec3i, ()>
```

Where the entity is, rounded down to whole blocks. `Err(())` when it is gone. Written in
slab on top of `run_command`: `data get entity <selector> Pos[0]`.

#### `Entity::position_near`

```slab
pub fn position_near(self) -> Result<NearVec3, ()>
```

Where the entity is, with 4 decimals, read straight from the world. `Err(())` when it is
gone or more than 21,474 blocks from the origin (use `position_far` there).

#### `Entity::position_far`

```slab
pub fn position_far(self) -> Result<FarVec3, ()>
```

Where the entity is, with 6 decimals, anywhere in the world. The game cannot hand out
the digits of a big coordinate, so a scratch marker is created on the entity, moved back
to the origin by the whole part (which leaves only the fraction), read, and removed.
`Err(())` when the entity is gone or the chunk at 0 0 is not loaded (the marker needs it).

#### `Entity::rotation`

```slab
pub fn rotation(self) -> Result<Rotation, ()>
```

Which way the entity faces. `Err(())` when it is gone.

#### `Entity::set_rotation`

```slab
pub fn set_rotation(self, yaw: NearDouble, pitch: NearDouble) -> Result<int, ()>
```

Turn the entity: yaw in degrees (0 faces south, 90 west, -90 east, 180 north), pitch
from -90 (up) to 90 (down). A yaw outside -180 to 180 is wrapped into it.

#### `Entity::face`

```slab
pub fn face(self, other: Entity) -> Result<int, ()>
```

Turn the entity to look at another entity (its feet).

#### `Entity::move_local`

```slab
pub fn move_local(self, left: NearDouble, up: NearDouble, forward: NearDouble) -> Result<int, ()>
```

Move the entity along its own axes, like `^left ^up ^forward` in a command: `forward` is
the way it faces (so `e.move_local(0, 0, 5)` goes five blocks ahead), `left` and `up`
are to its left and above it. It keeps its rotation. `tp(e, ^ ^ ^5)` would measure from
whoever runs the command, not from `e`.

#### `Entity::move_by`

```slab
pub fn move_by(self, dx: NearDouble, dy: NearDouble, dz: NearDouble) -> Result<int, ()>
```

Move the entity by a distance along the world axes (`~dx ~dy ~dz`): x is east, y up, z south.

#### `Entity::velocity`

```slab
pub fn velocity(self) -> Result<NearVec3, ()>
```

The entity's velocity in blocks per tick (`Motion`). It changes every tick for anything
the game moves (gravity, friction). `Err(())` when it is gone.

#### `Entity::set_velocity`

```slab
pub fn set_velocity(self, v: NearVec3) -> Result<int, ()>
```

Set the velocity, in blocks per tick. The game does not allow this for players
(`Err(())`), only for other entities.

#### `Entity::push`

```slab
pub fn push(self, by: NearVec3) -> Result<int, ()>
```

Add to the velocity (a push). Not for players, like `set_velocity`.

#### `Entity::teleport_near`

```slab
pub fn teleport_near(self, to: NearVec3) -> Result<int, ()>
```

Move the entity to a position with decimals. `Ok` when it moved.

#### `Entity::teleport_far`

```slab
pub fn teleport_far(self, to: FarVec3) -> Result<int, ()>
```

Move the entity to a position with 6 decimals, anywhere inside the world border.

## `game::tag`

Entity tags (`/tag`). `import game::tag;` then `tag::add(e, "boss")`.

### `add`

```slab
pub fn add(target: Target, name: str) -> bool
```

Add a tag to the matching entities. `true` when anything was tagged (an entity that
already has the tag does not count). Tags added with a name known at compile time are
removed again by `_uninstall`.

### `remove`

```slab
pub fn remove(target: Target, name: str) -> bool
```

Remove a tag from the matching entities. `true` when anything was untagged.

### `has`

```slab
pub fn has(target: Target, name: const str) -> bool
```

Whether every matching entity carries the tag. The tag must be known at compile time,
because it becomes part of a selector.

## `game::effect`

Status effects (`/effect`). `import game::effect;` then `effect::give(@s, "minecraft:speed", 10s, 1)`.

### `give`

```slab
pub fn give(target: Target, effect: str, ticks: int, amplifier: int) -> bool
```

Give an effect to the matching entities. `ticks` is the duration in ticks (`10s` is ten
seconds; the game takes whole seconds, so it is rounded up) and `amplifier` (0 to 255) the
level minus one. Both may be variables. `true` when anything got the effect.

### `clear`

```slab
pub fn clear(target: Target) -> bool
```

Clear all effects from the matching entities. `true` when anything was cleared.
