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
        Kw::Positioned => "```slab\npositioned x y z { ... }\n```\n`/execute positioned`: runs the block at a position. Coordinates are integers, variables, or `~`/`^` offsets.",
        Kw::Rotated => "```slab\nrotated yaw pitch { ... }\n```\n`/execute rotated`: runs the block with a rotation.",
        Kw::Facing => "```slab\nfacing x y z { ... }\n```\n`/execute facing`: runs the block turned towards a position.",
        Kw::Align => "```slab\nalign xz { ... }\n```\n`/execute align`: snaps the position to whole blocks on the named axes.",
        Kw::Anchored => "```slab\nanchored eyes { ... }\n```\n`/execute anchored`: sets whether `^` coordinates start at the feet or the eyes.",
        Kw::Pub => "Makes an item visible to the modules that import this one. Without it an item is private to its module.",
        Kw::Import => "```slab\nimport util;            // the module util, used as util::f()\nimport util::{f, Point};\nimport util::*;\n```\nBrings another module (or one item of it) into scope. `std` and `game` are built in.",
    }
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
