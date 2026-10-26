# Numbers with decimals: `NearDouble` and `FarDouble`

The scoreboard only holds 32-bit integers, and `int` is all the language has at its core. Minecraft
positions are doubles (`Pos: [5.1234d, -20.25d, 9.9999d]`), so slab has two library types for
numbers with a fraction. They work on every version that has `data get ... <scale>`; a real
`float` type that uses 26.x's `/compute` is separate and comes later (`CLAUDE.md`, "Planned features").

Both are plain structs written in `src/stdlib/std.slab`, so ctrl+click shows how they work.

| | `NearDouble` | `FarDouble` |
|---|---|---|
| decimals | 4 | 6 |
| read from the world | one `data get` per coordinate | a scratch marker, about ten commands per position |
| exact for positions | within 21,474 blocks of the origin | anywhere inside the world border |
| needs | nothing | the chunk at 0 0 loaded |
| arithmetic | `+ - *`, `div_int`, comparisons | `+ -`, negation, comparisons |
| made with | `position_near()` | `position_far()` |

```slab
import std::*;
import game::*;

let a: NearDouble = 1.5;          // a decimal literal needs a type to tell which of the two it is
let b = a + 0.25;                 // `a + b` is `a.add(b)`; the literal gets `a`'s type
let c = -b * 2;                   // `-x` is `x.neg()`, an integer converts
if c < a { print(c.text()); }     // "-3.5000": the text Minecraft reads

let cow = summon("minecraft:cow", 4, 70, -6);
let p = cow.position_near();                    // Result<NearVec3, ()>
cow.teleport_near(NearVec3 { x: 1.5, y: 70, z: -0.25 });
let far = cow.position_far();                   // Result<FarVec3, ()>
```

## What a value is

`whole` is the number rounded **down**, `frac` the digits after the point, always 0 to 9999 (or 0 to
999999): `-6.5` is `whole: -7, frac: 5000`. Rounding down keeps the pair unique, makes addition a
carry, and matches how the game itself rounds (below). `==` and `!=` compare both fields.

## Why two types

The type is chosen by how the number gets *out of the game*, because that is where the limits are.
These were measured on a 26.3 server (`docs/mc/05-verified-by-slab.md`, "Reading and writing
doubles"):

1. **The only way to read a double is `data get ... <scale>`**, which gives `floor(value × scale)` as
   an `int`. The scale has to be small enough for the result to fit: scale 1 reaches the whole
   world, scale 10000 only ±214,748 blocks, scale 100000 only ±21,474. Past that the game **clamps**
   at 2147483647 instead of wrapping, so the digits are gone, not wrong in a recoverable way.
2. **The multiplication is floating point**, so the floor can land one too low: 0.57 × 10000 reads as
   5699, and about 6% of random 4-decimal values are off. Reading one digit more (scale 100000) and
   rounding fixes every case. That is why `NearDouble` reads at 100000 and its range is 21,474, not
   214,748.
3. **A scratch marker avoids the limit.** `execute at <entity> summon marker run ...` makes an
   entity at the same spot, `tp @s ~-<whole>` moves it back by the whole part (this is exact: both
   numbers are doubles of similar size), and what is left is a fraction below 1, which any scale can
   read. That keeps the digits at the world border (29,999,984.5678 reads as 5678), at the price of
   an entity, about ten commands, and a loaded chunk 0 0 for the marker to land in. The marker is
   tagged `slab.scratch` and removed at the end of the read.

So one type cannot be both cheap and exact everywhere. `NearDouble` is the cheap one for anything
near spawn (most builds, mini-games, arenas); `FarDouble` is for positions that can be anywhere.
Converting is explicit: `near.to_far()` is exact, `far.to_near()` rounds to 4 decimals and is only
meaningful near the origin.

## Why the arithmetic differs

Inside ±21,474 blocks a `NearDouble` is `whole × 10000 + frac`, which fits an `int`, so add, subtract
and compare are single integer operations and multiplication splits into parts that also fit
(`whole × whole`, `whole × frac`, `frac × frac / 10000`). A product is cut off after the fourth
decimal. A `FarDouble`'s whole part can be 30 million, so a product would need 64 bits and the
scoreboard has 32: it offers only addition, subtraction, negation and comparisons, which cannot
overflow. To multiply big positions, convert to `NearDouble` after moving to a local frame.

## Writing back

`tp` takes decimal text, so `teleport_near` / `teleport_far` build `-6.0005` from the pair
(`text()`): a sign, the whole part of the absolute value, a point, and the fraction with its
**leading zeros**. `.5` would mean 0.5, not 0.0005, and `-7` plus `.5` would be -7.5, not -6.5, which is
why the text is not just `whole.frac`. The game stores exactly the digits it is given
(`7.123456789` reads back as itself).

## Limits to know

* A `NearDouble` read fails with `Err(())` beyond ±21,474 blocks; use `position_far`.
* `position_far` fails with `Err(())` when the chunk at 0 0 is not loaded (it checks first, so no
  marker is left behind). A pack that uses it can run `forceload add 0 0` on load.
* A decimal literal needs a type (`let x: NearDouble = 1.5;`) and at most 4 (or 6) digits; `1e5` is not
  supported.
* Operators are method calls, so, like any call, they cannot appear inside `&&` or `||`
  (`docs/lang/codegen.md`): put the comparison in a `bool` first.
* Divide with `div_int`; dividing two decimals would need 64-bit intermediates.

## How operators work

`a + b` on a struct that has a method `add` is `a.add(b)`; the same for `- * / %` (`sub mul div rem`),
`< <= > >=` (`lt le gt ge`) and unary `-` (`neg`), and `a += b` is `a = a.add(b)`. `==` and `!=`
stay structural. This is the general mechanism; the two types are its only users for now. A struct
marked `#[decimals(N)]` (standard library only) also accepts decimal literals with up to N digits.
