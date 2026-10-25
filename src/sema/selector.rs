//! Validates the raw `key=value` selector arguments and converts them to the typed
//! `datapack::Selector`, so a bad selector is a compile error with a source span instead of a
//! runtime parse failure inside the game.

use super::ty::Ty;
use crate::datapack::{ArgPart, DynKey, FloatRange, Gamemode, Hole, IdOrTag, IntRange, Objective, ResourceLocation, Selector, SelectorArg, SelectorVar, Sort, Word};
use crate::diag::Diagnostic;
use crate::syntax::ast;

type R<T> = Result<T, String>;

/// A selector literal checked and converted. `holes[k]` is the expression (the `index`th hole of
/// argument `arg`) that fills `Hole(k)`, and the type it must have.
pub struct Converted {
    pub selector: Selector,
    pub holes: Vec<HoleSlot>,
}

pub struct HoleSlot {
    pub arg: usize,
    pub index: usize,
    pub ty: Ty,
}

pub fn convert(lit: &ast::SelectorLit) -> Result<Converted, Vec<Diagnostic>> {
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
    let mut holes = Vec::new();
    for (ai, a) in lit.args.iter().enumerate() {
        let converted = if a.value.contains("$(h") { dynamic(&a.key, &a.value, ai, &mut holes).map(|x| vec![x]) } else { arg(&a.key, &a.value) };
        match converted {
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
    if errors.is_empty() { Ok(Converted { selector: sel, holes }) } else { Err(errors) }
}

/// Splits a value containing `$(hN)` markers into literal text and holes. `ty` is the type the
/// hole expressions must have.
fn parts(v: &str, ty: &Ty, ai: usize, holes: &mut Vec<HoleSlot>) -> R<Vec<ArgPart>> {
    let mut out = Vec::new();
    let mut rest = v;
    while let Some(i) = rest.find("$(h") {
        if i > 0 {
            out.push(ArgPart::Lit(rest[..i].to_owned()));
        }
        let after = &rest[i + 3..];
        let end = after.find(')').ok_or("a broken hole marker")?;
        let index: usize = after[..end].parse().map_err(|_| "a broken hole marker".to_owned())?;
        holes.push(HoleSlot { arg: ai, index, ty: ty.clone() });
        out.push(ArgPart::Hole(Hole(holes.len() as u16 - 1)));
        rest = &after[end + 1..];
    }
    if !rest.is_empty() {
        out.push(ArgPart::Lit(rest.to_owned()));
    }
    Ok(out)
}

/// A range with possible holes at its ends: `..$r`, `$a..$b`, `3..`, or one value.
fn range_parts(v: &str, ai: usize, holes: &mut Vec<HoleSlot>) -> R<Vec<ArgPart>> {
    // split at the first `..` that is not inside a hole marker (markers contain no dots)
    let (lo, hi) = match v.split_once("..") {
        Some((a, b)) => (a, Some(b)),
        None => (v, None),
    };
    let mut out = Vec::new();
    for (i, side) in [Some(lo), hi].into_iter().enumerate() {
        let Some(side) = side else { continue };
        if i == 1 {
            out.push(ArgPart::Lit("..".into()));
        }
        if side.is_empty() {
            continue;
        }
        if side.starts_with("$(h") && side.ends_with(')') && side.matches("$(h").count() == 1 {
            out.extend(parts(side, &Ty::Int, ai, holes)?);
        } else if side.contains("$(h") {
            return Err(format!("a range end is a number or a single variable, found `{side}`"));
        } else {
            float(side)?;
            out.push(ArgPart::Lit(side.to_owned()));
        }
    }
    if out.is_empty() || out == [ArgPart::Lit("..".into())] {
        return Err("a range needs at least one bound".into());
    }
    Ok(out)
}

/// An option whose value has run-time parts (`tag=$t`, `distance=..$r`, `scores={kills=$n..}`).
fn dynamic(key: &str, value: &str, ai: usize, holes: &mut Vec<HoleSlot>) -> R<SelectorArg> {
    let named = |key: DynKey, negated: bool, value: Vec<ArgPart>| Ok(SelectorArg::Dynamic { key, negated, value });
    let text = |key: DynKey, v: &str, holes: &mut Vec<HoleSlot>| -> R<SelectorArg> {
        let (negated, v) = negation(v);
        let v = if key == DynKey::Name { strip_quotes(v)? } else { v.to_owned() };
        named(key, negated, parts(&v, &Ty::Str, ai, holes)?)
    };
    let single_number = |key: DynKey, v: &str, holes: &mut Vec<HoleSlot>| -> R<SelectorArg> {
        if !(v.starts_with("$(h") && v.ends_with(')') && v.matches("$(h").count() == 1) {
            return Err(format!("`{}` takes a number or a single variable, found `{v}`", key_name(key)));
        }
        named(key, false, parts(v, &Ty::Int, ai, holes)?)
    };
    match key {
        "tag" => text(DynKey::Tag, value, holes),
        "team" => text(DynKey::Team, value, holes),
        "name" => text(DynKey::Name, value, holes),
        "type" => text(DynKey::Type, value, holes),
        "gamemode" => text(DynKey::Gamemode, value, holes),
        "limit" => single_number(DynKey::Limit, value, holes),
        "x" => single_number(DynKey::X, value, holes),
        "y" => single_number(DynKey::Y, value, holes),
        "z" => single_number(DynKey::Z, value, holes),
        "dx" => single_number(DynKey::Dx, value, holes),
        "dy" => single_number(DynKey::Dy, value, holes),
        "dz" => single_number(DynKey::Dz, value, holes),
        "distance" => named(DynKey::Distance, false, range_parts(value, ai, holes)?),
        "x_rotation" => named(DynKey::XRotation, false, range_parts(value, ai, holes)?),
        "y_rotation" => named(DynKey::YRotation, false, range_parts(value, ai, holes)?),
        "level" => named(DynKey::Level, false, range_parts(value, ai, holes)?),
        "scores" => {
            let inner = value.strip_prefix('{').and_then(|v| v.strip_suffix('}')).ok_or("expected `{objective=range, ...}`")?;
            let mut entries = Vec::new();
            for part in inner.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let (name, range) = part.split_once('=').ok_or_else(|| format!("expected `objective=range`, found `{part}`"))?;
                entries.push((Objective::new(name.trim()).map_err(|e| e.message().to_owned())?, range_parts(range.trim(), ai, holes)?));
            }
            if entries.is_empty() {
                return Err("scores={} needs at least one objective".into());
            }
            Ok(SelectorArg::DynScores(entries))
        }
        "sort" | "predicate" | "nbt" | "advancements" => Err(format!("`{key}=` cannot take a variable")),
        other => Err(format!("unknown selector argument `{other}`")),
    }
}

fn key_name(key: DynKey) -> &'static str {
    match key {
        DynKey::Limit => "limit",
        DynKey::X => "x",
        DynKey::Y => "y",
        DynKey::Z => "z",
        DynKey::Dx => "dx",
        DynKey::Dy => "dy",
        _ => "dz",
    }
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
            args: args.iter().map(|(k, v)| A { key: (*k).into(), value: (*v).into(), span: Default::default(), holes: Vec::new() }).collect(),
        };
        convert(&lit).map(|c| c.selector.to_string()).map_err(|ds| ds.into_iter().map(|d| d.message).collect())
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

    /// Converts selector arguments whose values contain `$(hN)` markers, like the parser leaves them.
    fn dyn_sel(args: &[(&str, &str)]) -> Result<(String, Vec<String>), Vec<String>> {
        let lit = SelectorLit {
            var: 'e',
            args: args.iter().map(|(k, v)| A { key: (*k).into(), value: (*v).into(), span: Default::default(), holes: Vec::new() }).collect(),
        };
        convert(&lit)
            .map(|c| (c.selector.to_string(), c.holes.iter().map(|h| format!("{}.{}:{:?}", h.arg, h.index, h.ty)).collect()))
            .map_err(|ds| ds.into_iter().map(|d| d.message).collect())
    }

    #[test]
    fn converts_variable_options() {
        let (text, holes) = dyn_sel(&[("tag", "$(h0)"), ("distance", "..$(h0)"), ("scores", "{kills=$(h0).., deaths=..3}"), ("limit", "$(h0)")]).unwrap();
        assert_eq!(text, "@e[tag=$(h0),distance=..$(h1),scores={kills=$(h2)..,deaths=..3},limit=$(h3)]");
        // one slot per hole, in order, with the type it needs: strings for names, ints for numbers
        assert_eq!(holes, ["0.0:Str", "1.0:Int", "2.0:Int", "3.0:Int"]);
        let (text, _) = dyn_sel(&[("type", "!$(h0)"), ("name", "\"Mr $(h0)\""), ("tag", "pre_$(h0)_post"), ("distance", "$(h0)..$(h1)"), ("level", "5..$(h0)")]).unwrap();
        assert_eq!(text, "@e[type=!$(h0),name=\"Mr $(h1)\",tag=pre_$(h2)_post,distance=$(h3)..$(h4),level=5..$(h5)]");
    }

    #[test]
    fn rejects_bad_variable_options() {
        let msg = |args: &[(&str, &str)]| dyn_sel(args).unwrap_err().remove(0);
        assert!(msg(&[("sort", "$(h0)")]).contains("cannot take a variable"));
        assert!(msg(&[("limit", "1$(h0)")]).contains("a number or a single variable"));
        assert!(msg(&[("distance", "$(h0)$(h1)..3")]).contains("a number or a single variable"));
        assert!(msg(&[("distance", "x..$(h0)")]).contains("expected a number"));
        assert!(msg(&[("scores", "{}")]).contains("at least one objective"));
        assert!(msg(&[("typo", "$(h0)")]).contains("unknown selector argument"));
        // a literal piece that can never be valid is caught when the selector is checked
        assert!(msg(&[("tag", "a b$(h0)")]).contains("cannot be part of"));
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
