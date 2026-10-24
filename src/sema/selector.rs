//! Validates the raw `key=value` selector arguments and converts them to the typed
//! `datapack::Selector`, so a bad selector is a compile error with a source span instead of a
//! runtime parse failure inside the game.

use crate::datapack::{Gamemode, IdOrTag, IntRange, Objective, ResourceLocation, Selector, SelectorArg, SelectorVar, Sort, Word, FloatRange};
use crate::diag::Diagnostic;
use crate::syntax::ast;

type R<T> = Result<T, String>;

pub fn convert(lit: &ast::SelectorLit) -> Result<Selector, Vec<Diagnostic>> {
    let var = match lit.var {
        'p' => SelectorVar::P,
        'r' => SelectorVar::R,
        'a' => SelectorVar::A,
        'e' => SelectorVar::E,
        's' => SelectorVar::S,
        _ => SelectorVar::N,
    };
    let mut sel = Selector::new(var);
    let mut errors = Vec::new();
    for a in &lit.args {
        match arg(&a.key, &a.value) {
            Ok(args) => sel.args.extend(args),
            Err(msg) => {
                // don't repeat the key when the message already names it
                let full = if msg.contains(&format!("`{}`", a.key)) { msg } else { format!("in selector argument `{}`: {msg}", a.key) };
                errors.push(Diagnostic::new(full, a.span));
            }
        }
    }
    if errors.is_empty() {
        use crate::datapack::Check;
        if let Err(e) = sel.check() {
            errors.push(Diagnostic::new(e.message().to_owned(), lit.args.first().map_or_else(Default::default, |a| a.span)));
        }
    }
    if errors.is_empty() { Ok(sel) } else { Err(errors) }
}

/// Splits a leading `!`.
fn negation(v: &str) -> (bool, &str) {
    match v.strip_prefix('!') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, v),
    }
}

fn word(v: &str) -> R<Word> {
    Word::new(v).map_err(|e| e.message().to_owned())
}

fn id(v: &str) -> R<ResourceLocation> {
    ResourceLocation::parse(v).map_err(|e| e.message().to_owned())
}

fn float(v: &str) -> R<f64> {
    match v.parse::<f64>() {
        Ok(n) if n.is_finite() => Ok(n),
        _ => Err(format!("expected a number, found `{v}`")),
    }
}

fn int(v: &str) -> R<i32> {
    v.parse::<i32>().map_err(|_| format!("expected an integer, found `{v}`"))
}

fn int_range(v: &str) -> R<IntRange> {
    let r = match v.split_once("..") {
        None => IntRange::exactly(int(v)?),
        Some((a, b)) => IntRange {
            min: if a.is_empty() { None } else { Some(int(a)?) },
            max: if b.is_empty() { None } else { Some(int(b)?) },
        },
    };
    check_range(r.min.is_none() && r.max.is_none(), r.min.zip(r.max).is_some_and(|(a, b)| a > b))?;
    Ok(r)
}

fn float_range(v: &str) -> R<FloatRange> {
    let r = match v.split_once("..") {
        None => {
            let n = float(v)?;
            FloatRange::between(n, n)
        }
        Some((a, b)) => FloatRange {
            min: if a.is_empty() { None } else { Some(float(a)?) },
            max: if b.is_empty() { None } else { Some(float(b)?) },
        },
    };
    check_range(r.min.is_none() && r.max.is_none(), r.min.zip(r.max).is_some_and(|(a, b)| a > b))?;
    Ok(r)
}

fn check_range(empty: bool, inverted: bool) -> R<()> {
    if empty {
        return Err("a range needs at least one bound".into());
    }
    if inverted {
        return Err("the range minimum is bigger than its maximum".into());
    }
    Ok(())
}

fn strip_quotes(v: &str) -> R<String> {
    match v.chars().next() {
        Some(q @ ('"' | '\'')) => {
            if v.len() >= 2 && v.ends_with(q) {
                Ok(v[1..v.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\"))
            } else {
                Err("unterminated string".into())
            }
        }
        _ => Ok(v.to_owned()),
    }
}

fn gamemode(v: &str) -> R<Gamemode> {
    Ok(match v {
        "survival" => Gamemode::Survival,
        "creative" => Gamemode::Creative,
        "adventure" => Gamemode::Adventure,
        "spectator" => Gamemode::Spectator,
        other => return Err(format!("unknown game mode `{other}` (survival, creative, adventure, spectator)")),
    })
}

fn arg(key: &str, value: &str) -> R<Vec<SelectorArg>> {
    let one = |a: SelectorArg| Ok(vec![a]);
    match key {
        "x" => one(SelectorArg::X(float(value)?)),
        "y" => one(SelectorArg::Y(float(value)?)),
        "z" => one(SelectorArg::Z(float(value)?)),
        "dx" => one(SelectorArg::Dx(float(value)?)),
        "dy" => one(SelectorArg::Dy(float(value)?)),
        "dz" => one(SelectorArg::Dz(float(value)?)),
        "distance" => one(SelectorArg::Distance(float_range(value)?)),
        "x_rotation" => one(SelectorArg::XRotation(float_range(value)?)),
        "y_rotation" => one(SelectorArg::YRotation(float_range(value)?)),
        "level" => one(SelectorArg::Level(int_range(value)?)),
        "limit" => match value.parse::<u32>() {
            Ok(n) if n >= 1 => one(SelectorArg::Limit(n)),
            _ => Err(format!("limit must be a positive integer, found `{value}`")),
        },
        "sort" => one(SelectorArg::Sort(match value {
            "nearest" => Sort::Nearest,
            "furthest" => Sort::Furthest,
            "random" => Sort::Random,
            "arbitrary" => Sort::Arbitrary,
            other => return Err(format!("unknown sort `{other}` (nearest, furthest, random, arbitrary)")),
        })),
        "tag" => {
            let (negated, v) = negation(value);
            one(SelectorArg::Tag { name: word(v)?, negated })
        }
        "team" => {
            let (negated, v) = negation(value);
            one(SelectorArg::Team { name: word(v)?, negated })
        }
        "name" => {
            let (negated, v) = negation(value);
            one(SelectorArg::Name { name: strip_quotes(v)?, negated })
        }
        "gamemode" => {
            let (negated, v) = negation(value);
            one(SelectorArg::Gamemode { mode: gamemode(v)?, negated })
        }
        "type" => {
            let (negated, v) = negation(value);
            let id = match v.strip_prefix('#') {
                Some(tag) => IdOrTag::Tag(id(tag)?),
                None => IdOrTag::Id(id(v)?),
            };
            one(SelectorArg::Type { id, negated })
        }
        "predicate" => {
            let (negated, v) = negation(value);
            one(SelectorArg::Predicate { id: id(v)?, negated })
        }
        "scores" => {
            let inner = value.strip_prefix('{').and_then(|v| v.strip_suffix('}')).ok_or("expected `{objective=range, ...}`")?;
            let mut entries = Vec::new();
            for part in inner.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let (name, range) = part.split_once('=').ok_or_else(|| format!("expected `objective=range`, found `{part}`"))?;
                entries.push((Objective::new(name.trim()).map_err(|e| e.message().to_owned())?, int_range(range.trim())?));
            }
            if entries.is_empty() {
                return Err("scores={} needs at least one objective".into());
            }
            one(SelectorArg::Scores(entries))
        }
        "nbt" | "advancements" => Err(format!("`{key}=` is not supported in selectors yet")),
        other => Err(format!("unknown selector argument `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::ast::{SelectorArg as A, SelectorLit};

    fn sel(var: char, args: &[(&str, &str)]) -> Result<String, Vec<String>> {
        let lit = SelectorLit {
            var,
            args: args.iter().map(|(k, v)| A { key: (*k).into(), value: (*v).into(), span: Default::default() }).collect(),
        };
        convert(&lit).map(|s| s.to_string()).map_err(|ds| ds.into_iter().map(|d| d.message).collect())
    }

    #[test]
    fn converts_arguments() {
        assert_eq!(sel('s', &[]).unwrap(), "@s");
        assert_eq!(
            sel('e', &[("type", "zombie"), ("distance", "..10"), ("tag", "!boss"), ("limit", "1"), ("sort", "nearest")]).unwrap(),
            "@e[type=minecraft:zombie,distance=..10,tag=!boss,limit=1,sort=nearest]"
        );
        assert_eq!(sel('a', &[("scores", "{kills=1.., deaths=..3}")]).unwrap(), "@a[scores={kills=1..,deaths=..3}]");
        assert_eq!(sel('a', &[("name", "\"Bob\""), ("gamemode", "!creative"), ("level", "5..10")]).unwrap(), "@a[name=\"Bob\",gamemode=!creative,level=5..10]");
        assert_eq!(sel('e', &[("type", "#minecraft:skeletons"), ("x", "1.5")]).unwrap(), "@e[type=#minecraft:skeletons,x=1.5]");
    }

    #[test]
    fn reports_each_bad_argument() {
        let errs = sel('e', &[("typ", "x"), ("distance", "5..1"), ("limit", "0"), ("tag", "a b"), ("gamemode", "fly")]).unwrap_err();
        assert_eq!(errs.len(), 5, "{errs:?}");
        assert!(errs[0].contains("unknown selector argument `typ`"));
        assert!(errs[1].contains("minimum is bigger"));
        assert!(errs[2].contains("positive integer"));
        assert!(errs[3].contains("invalid name"));
        assert!(errs[4].contains("unknown game mode"));
        assert!(sel('e', &[("nbt", "{}")]).unwrap_err()[0].contains("not supported"));
    }

    #[test]
    fn duplicate_single_use_options_are_rejected() {
        assert!(sel('a', &[("limit", "1"), ("limit", "2")]).unwrap_err()[0].contains("only be used once"));
    }
}
