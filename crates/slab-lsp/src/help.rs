//! Hover texts for the parts of the language that are not names: keywords, attributes,
//! literals, selectors and coordinates.

use slab::syntax::token::{Kw, RelKind, Tok};

/// One line about a keyword, with the shape it is used in.
pub fn keyword(k: Kw) -> &'static str {
    match k {
        Kw::Fn => "```slab\nfn name(param: type, ...) -> type { ... }\n```\nDeclares a function. Without `-> type` it returns nothing. Attributes such as `#[load]` make it an entry point.",
        Kw::Let => "```slab\nlet [mut] name[: type] = value;\n```\nDeclares a local variable. Variables cannot be assigned again unless declared `mut`; the type is inferred from the value when it is left out.",
        Kw::Mut => "Makes a variable assignable: `let mut count = 0; count += 1;`.",
        Kw::If => "```slab\nif condition { ... } else if condition { ... } else { ... }\n```\nRuns a block when the condition (a `bool`) is true. Conditions need no parentheses.",
        Kw::Match => "```slab\nmatch value {\n    Some(x) if x > 0 => x,\n    Some(_) => 0,\n    None => return,\n}\n```\nPicks the first arm whose pattern fits the value (and whose `if` guard holds). Patterns: `_`, a name (binds the value), literals, `Variant(p, ..)`, `A | B`. It must cover every case, and can be used as a value. `if let Some(x) = value { .. } else { .. }` is the one-arm form.",
        Kw::Else => "The branch of an `if` that runs when its condition is false.",
        Kw::While => "```slab\nwhile condition { ... }\n```\nRepeats a block while the condition holds. A loop can run about 21,000 times in one tick (Minecraft's command limit).",
        Kw::Loop => "```slab\nloop { ... }\n```\nRepeats a block forever; leave it with `break`.",
        Kw::For => "```slab\nfor i in 0..10 { ... }\nfor x in list { ... }\nfor e in @e[type=zombie] { ... }\n```\nCounts through a range, walks a list, or runs the body once per entity a selector matches (with `e` bound to that entity).",
        Kw::In => "Part of `for x in ...`, and `in` as a context clause: `in minecraft:the_nether { ... }` runs a block in another dimension.",
        Kw::Return => "```slab\nreturn value;\n```\nLeaves the function, giving back a value when the function has a return type.",
        Kw::Break => "Leaves the innermost loop.",
        Kw::Continue => "Skips to the next iteration of the innermost loop.",
        Kw::Struct => "```slab\nstruct Point { x: int, y: int }\n```\nA named record of fields, stored as a compound in command storage. Values are copied when assigned or passed.",
        Kw::Enum => "```slab\nenum Shape { Dot, Line(int, int) }\n```\nA type with a fixed set of variants, each with optional values. `Option` and `Result` are enums.",
        Kw::Impl => "```slab\nimpl Point { fn sum(self) -> int { ... } }\n```\nAttaches functions to a struct or enum. A first parameter `self` makes a method: `p.sum()`.",
        Kw::Global => "```slab\nglobal ticks: int = 0;\n```\nA variable that keeps its value across ticks and `/reload`. The initial value must be a constant.",
        Kw::Const => "```slab\nconst MAX: int = 8;\n```\nA compile-time constant.",
        Kw::Objective => "```slab\nobjective kills = \"playerKillCount\";\n```\nDeclares a scoreboard objective with a criterion; read and write scores with `kills[@s]`.",
        Kw::True | Kw::False => "A `bool` value.",
        Kw::As => "```slab\nas @a { ... }\n```\n`/execute as`: runs the block once for each matching entity, with `@s` being that entity.",
        Kw::At => "```slab\nat @s { ... }\n```\n`/execute at`: runs the block at the position (and dimension) of each matching entity.",
        Kw::Be => "```slab\nbe e { ... }\n```\nShorthand for `as e at @s { ... }`: runs the block as the entity `e`, at its position. `e` is a selector or an entity value, and is looked up once. It chains with the other clauses: `be e positioned ~ ~1 ~ { ... }`.",
        Kw::Positioned => "```slab\npositioned x y z { ... }\n```\n`/execute positioned`: runs the block at a position. Coordinates are integers, variables, or `~`/`^` offsets.",
        Kw::Rotated => "```slab\nrotated yaw pitch { ... }\n```\n`/execute rotated`: runs the block with a rotation.",
        Kw::Facing => "```slab\nfacing x y z { ... }\n```\n`/execute facing`: runs the block turned towards a position.",
        Kw::Align => "```slab\nalign xz { ... }\n```\n`/execute align`: snaps the position to whole blocks on the named axes.",
        Kw::Anchored => "```slab\nanchored eyes { ... }\n```\n`/execute anchored`: sets whether `^` coordinates start at the feet or the eyes.",
        Kw::Pub => "Makes an item visible to the modules that import this one. Without it an item is private to its module.",
        Kw::Import => "```slab\nimport util;            // the module util, used as util::f()\nimport util::{f, Point};\nimport util::*;\n```\nBrings another module (or one item of it) into scope. `std` and `game` are built in.",
    }
}

/// `task` (before `fn`) and `start` (before a task's name): words only there, so they are not keywords.
pub fn contextual(word: &str) -> Option<&'static str> {
    Some(match word {
        "task" => "```slab\ntask fn ring(center: Entity) {\n    let mut i = 0;\n    while i < 360 { ...; i += 1; }\n    print(\"done\");\n}\n```\nA function that may take many ticks. The game stops a function after 65,536 commands in one tick, so a task runs its loops in slices: after about 30,000 commands (`task-budget` in `Slab.toml`) it waits for the next tick and carries on where it was, and the code after a loop runs when the loop is done. Launch it with `start ring(player);`, which returns at once. One run at a time: starting it again begins it over. A task returns nothing and cannot be called, only started.",
        "start" => "```slab\nstart ring(player);\n```\nLaunches a `task fn` and returns at once; the task runs from the next tick, a slice per tick, until it finishes. Starting a task that is still running begins it over.",
        _ => return None,
    })
}

/// What an attribute does.
pub fn attribute(name: &str) -> Option<&'static str> {
    Some(match name {
        "load" => "```slab\n#[load]\nfn boot() { ... }\n```\nRuns the function when the pack loads and on every `/reload`, after the globals are initialised. Takes no parameters.",
        "tick" => "```slab\n#[tick]\nfn each_tick() { ... }\n```\nRuns the function every game tick (20 times a second). Takes no parameters.",
        "every" => "```slab\n#[every(1s)]\nfn heartbeat() { ... }\n```\nRuns the function every N ticks. N is a number of ticks (`20`) or a time (`1s` = 20 ticks, `1d` = 24000).",
        "intrinsic" => "Marks a function of the standard library whose body is built into the compiler (`#[intrinsic(\"id\")] fn f(..);`). Only the embedded standard library may use it.",
        "lang" => "Binds a standard-library type to a type the compiler provides (`#[lang(\"entity\")]`). Only the embedded standard library may use it.",
        _ => return None,
    })
}

/// What a literal token stands for. `text` is its source text.
pub fn literal(tok: &Tok, text: &str) -> Option<String> {
    Some(match tok {
        Tok::Int(n) => match text.chars().last() {
            Some(c @ ('t' | 's' | 'd')) if text.len() > 1 && text[..text.len() - 1].chars().all(|c| c.is_ascii_digit() || c == '_') => {
                let unit = match c {
                    't' => "ticks",
                    's' => "seconds",
                    _ => "days",
                };
                format!("```slab\n{text}\n```\na time literal: {} {unit} = **{n} ticks** (an `int`)", text[..text.len() - 1].replace('_', ""))
            }
            _ => format!("```slab\n{text}\n```\n`int` literal (32-bit, wraps on overflow)"),
        },
        Tok::Float(_) => "Floating point numbers are not supported yet.".to_owned(),
        Tok::Str(s) => format!("```slab\n{text}\n```\n`str` literal, {} character{}", s.chars().count(), if s.chars().count() == 1 { "" } else { "s" }),
        Tok::FStr(_) => "An f-string: text with `{expressions}` in it, built into a `str`. `print(f\"...\")` shows it as one chat message; elsewhere it is joined with a macro.".to_owned(),
        Tok::Rel { kind: RelKind::Tilde, .. } => "```slab\n~  ~5  ~-2\n```\nA coordinate **relative to the position** the command runs at (`~` is the same x, y or z, `~5` is five blocks further).".to_owned(),
        Tok::Rel { kind: RelKind::Caret, .. } => "```slab\n^  ^5\n```\nA **local coordinate**: relative to where the executing entity looks (`^` left, `^ ^` up, `^ ^ ^5` five blocks ahead). All three coordinates must use `^`.".to_owned(),
        _ => return None,
    })
}

/// What a selector variable letter stands for.
pub fn selector_var(var: char) -> &'static str {
    match var {
        'a' => "all players",
        'e' => "all entities",
        'p' => "the nearest player",
        'r' => "a random player",
        's' => "the entity the code is running as",
        'n' => "the nearest entity",
        _ => "entities",
    }
}

/// One option of a selector: `@e[key=value]`.
pub struct SelKey {
    pub name: &'static str,
    /// What goes after the `=`, as a short description.
    pub value: &'static str,
    pub doc: &'static str,
    /// Snippet inserted after the key is picked (the `=` included).
    pub snippet: &'static str,
    /// Whether the same key may appear more than once (`tag=a, tag=!b`).
    pub repeatable: bool,
}

/// The selector options slab accepts (`sema/selector.rs`). `nbt=` and `advancements=` are not
/// supported yet.
pub const SELECTOR_KEYS: &[SelKey] = &[
    SelKey { name: "type", value: "entity id, `#tag`, or `!` either", doc: "Only entities of this type: `type=minecraft:zombie`, a tag `type=#minecraft:skeletons`, or everything but it with `type=!minecraft:player`.", snippet: "type=${1:minecraft:zombie}", repeatable: true },
    SelKey { name: "tag", value: "tag name, or `!name`", doc: "Only entities that have (`tag=boss`) or lack (`tag=!boss`) this tag. Repeat the option to require several.", snippet: "tag=${1:name}", repeatable: true },
    SelKey { name: "team", value: "team name, or `!name`", doc: "Only entities on (`team=red`) or not on (`team=!red`) this team.", snippet: "team=${1:name}", repeatable: true },
    SelKey { name: "name", value: "\"text\", or `!\"text\"`", doc: "Only entities with this exact custom name: `name=\"Bob\"`.", snippet: "name=\"${1:name}\"", repeatable: true },
    SelKey { name: "distance", value: "range, like `..10`, `5..`, `3..8`", doc: "Distance from where the command runs, in blocks. `..10` is up to 10 blocks away, `5..` at least 5.", snippet: "distance=..${1:10}", repeatable: false },
    SelKey { name: "limit", value: "a positive integer", doc: "At most this many entities. With `sort` unset, `@e` and `@a` pick in an arbitrary order; `@p`, `@n` and `@r` already imply their own limit of 1.", snippet: "limit=${1:1}", repeatable: false },
    SelKey { name: "sort", value: "nearest | furthest | random | arbitrary", doc: "The order matches are taken in, which matters together with `limit`.", snippet: "sort=${1|nearest,furthest,random,arbitrary|}", repeatable: false },
    SelKey { name: "scores", value: "{objective=range, ...}", doc: "Only entities whose scores are in range: `scores={kills=5.., deaths=..2}`. The objective is one you declared with `objective`.", snippet: "scores={${1:objective}=${2:1..}}", repeatable: false },
    SelKey { name: "gamemode", value: "survival | creative | adventure | spectator, or `!` one", doc: "Only players in (`gamemode=creative`) or not in (`gamemode=!creative`) this game mode.", snippet: "gamemode=${1|survival,creative,adventure,spectator|}", repeatable: true },
    SelKey { name: "level", value: "range, like `5..10`", doc: "Only players whose experience level is in the range.", snippet: "level=${1:1..}", repeatable: false },
    SelKey { name: "predicate", value: "resource location, or `!` it", doc: "Only entities for which the predicate (a data pack `predicate/*.json`) is true: `predicate=mypack:is_burning`.", snippet: "predicate=${1:namespace:name}", repeatable: true },
    SelKey { name: "x", value: "number", doc: "x of the search origin, instead of where the command runs. Use with `y`, `z`, `dx`, `dy`, `dz` to select entities inside a box.", snippet: "x=${1:0}", repeatable: false },
    SelKey { name: "y", value: "number", doc: "y of the search origin.", snippet: "y=${1:64}", repeatable: false },
    SelKey { name: "z", value: "number", doc: "z of the search origin.", snippet: "z=${1:0}", repeatable: false },
    SelKey { name: "dx", value: "number", doc: "Size of a box along x, starting at the origin: entities touching the box match. Needs no `distance`.", snippet: "dx=${1:5}", repeatable: false },
    SelKey { name: "dy", value: "number", doc: "Size of the box along y.", snippet: "dy=${1:5}", repeatable: false },
    SelKey { name: "dz", value: "number", doc: "Size of the box along z.", snippet: "dz=${1:5}", repeatable: false },
    SelKey { name: "x_rotation", value: "range of pitch, like `-30..30`", doc: "Only entities looking up or down within this range (degrees, -90 is straight up, 90 straight down).", snippet: "x_rotation=${1:-30..30}", repeatable: false },
    SelKey { name: "y_rotation", value: "range of yaw, like `-45..45`", doc: "Only entities facing within this range of horizontal directions (degrees, -180 to 180).", snippet: "y_rotation=${1:-45..45}", repeatable: false },
];

/// A line about using variables, which every option but `sort`/`predicate` accepts.
pub const SELECTOR_VARIABLES: &str = "Values can contain variables: `$name` or `${expr}` (`@e[tag=$t, distance=..${r * 2}]`). `sort` and `predicate` cannot. `nbt=` and `advancements=` are not supported yet.";

pub fn selector_key(name: &str) -> Option<&'static SelKey> {
    SELECTOR_KEYS.iter().find(|k| k.name == name)
}
