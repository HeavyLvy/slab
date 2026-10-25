//! Checking `match`: whether the arms cover every value, and which arms can never run.
//!
//! The classic pattern-matrix algorithm: a row is the patterns of one arm, a column is a
//! position inside the matched value. `useful(rows, q)` asks whether some value fits `q` but no
//! row; the match is exhaustive when a row of wildcards is not useful.

use super::hir::Pat;
use super::ty::{EnumId, Ty};

/// The variants of an enum: name and payload types (which may mention `Ty::Param`).
pub type Variants<'a> = &'a dyn Fn(EnumId) -> Vec<(String, Vec<Ty>)>;

#[derive(Clone, Debug, PartialEq)]
enum Ctor {
    Variant(usize),
    Bool(bool),
    Int(i32),
    Str(String),
}

fn ctor_of(p: &Pat) -> Option<Ctor> {
    match p {
        Pat::Variant { index, .. } => Some(Ctor::Variant(*index)),
        Pat::Bool(b) => Some(Ctor::Bool(*b)),
        Pat::Int(n) => Some(Ctor::Int(*n)),
        Pat::Str(s) => Some(Ctor::Str(s.clone())),
        Pat::Wild | Pat::Bind(_) | Pat::Or(_) => None,
    }
}

/// Every constructor of a type that has a finite set of them.
fn signature(ty: &Ty, v: Variants) -> Option<Vec<Ctor>> {
    match ty {
        Ty::Bool => Some(vec![Ctor::Bool(true), Ctor::Bool(false)]),
        Ty::Enum(id, _) => Some((0..v(*id).len()).map(Ctor::Variant).collect()),
        _ => None,
    }
}

/// The types of the values inside a constructor.
fn arity(ty: &Ty, c: &Ctor, v: Variants) -> Vec<Ty> {
    match (ty, c) {
        (Ty::Enum(id, targs), Ctor::Variant(i)) => v(*id).get(*i).map(|(_, fields)| fields.iter().map(|f| f.subst(targs)).collect()).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn wilds(n: usize) -> Vec<Pat> {
    vec![Pat::Wild; n]
}

/// Splits `A | B` in the first column into separate rows.
fn expand(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for r in rows {
        match r.first() {
            Some(Pat::Or(alts)) => {
                let split: Vec<Vec<Pat>> = alts
                    .iter()
                    .map(|a| {
                        let mut row = r.clone();
                        row[0] = a.clone();
                        row
                    })
                    .collect();
                out.extend(expand(&split));
            }
            _ => out.push(r.clone()),
        }
    }
    out
}

/// The rows that can match a value built with `c`, with the head replaced by its parts.
fn specialize(rows: &[Vec<Pat>], c: &Ctor, n: usize) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for r in rows {
        let head = &r[0];
        let parts = match (head, ctor_of(head)) {
            (Pat::Variant { fields, .. }, Some(rc)) if rc == *c => fields.clone(),
            (_, Some(rc)) if rc == *c => wilds(n),
            (Pat::Wild | Pat::Bind(_), _) => wilds(n),
            _ => continue,
        };
        let mut row = parts;
        row.extend_from_slice(&r[1..]);
        out.push(row);
    }
    out
}

/// The rows whose head matches anything.
fn default_rows(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    rows.iter().filter(|r| matches!(r[0], Pat::Wild | Pat::Bind(_))).map(|r| r[1..].to_vec()).collect()
}

fn used(rows: &[Vec<Pat>]) -> Vec<Ctor> {
    let mut out: Vec<Ctor> = Vec::new();
    for r in rows {
        if let Some(c) = ctor_of(&r[0])
            && !out.contains(&c)
        {
            out.push(c);
        }
    }
    out
}

/// Whether some value fits `q` and none of `rows`.
pub fn useful(rows: &[Vec<Pat>], q: &[Pat], tys: &[Ty], v: Variants) -> bool {
    let Some(head) = q.first() else { return rows.is_empty() };
    let rows = expand(rows);
    match head {
        Pat::Or(alts) => alts.iter().any(|a| {
            let mut q2 = q.to_vec();
            q2[0] = a.clone();
            useful(&rows, &q2, tys, v)
        }),
        Pat::Wild | Pat::Bind(_) => {
            let seen = used(&rows);
            match signature(&tys[0], v) {
                Some(all) if all.iter().all(|c| seen.contains(c)) => all.iter().any(|c| {
                    let sub = arity(&tys[0], c, v);
                    let mut q2 = wilds(sub.len());
                    q2.extend_from_slice(&q[1..]);
                    let mut t2 = sub.clone();
                    t2.extend_from_slice(&tys[1..]);
                    useful(&specialize(&rows, c, sub.len()), &q2, &t2, v)
                }),
                _ => useful(&default_rows(&rows), &q[1..], &tys[1..], v),
            }
        }
        _ => {
            let c = ctor_of(head).expect("a constructor pattern");
            let sub = arity(&tys[0], &c, v);
            let mut q2 = match head {
                Pat::Variant { fields, .. } => fields.clone(),
                _ => Vec::new(),
            };
            q2.extend_from_slice(&q[1..]);
            let mut t2 = sub.clone();
            t2.extend_from_slice(&tys[1..]);
            useful(&specialize(&rows, &c, sub.len()), &q2, &t2, v)
        }
    }
}

fn describe(ty: &Ty, c: &Ctor, args: &[String], v: Variants) -> String {
    match c {
        Ctor::Bool(b) => b.to_string(),
        Ctor::Int(n) => n.to_string(),
        Ctor::Str(s) => format!("{s:?}"),
        Ctor::Variant(i) => {
            let name = match ty {
                Ty::Enum(id, _) => v(*id).get(*i).map(|(n, _)| n.clone()).unwrap_or_default(),
                _ => String::new(),
            };
            if args.is_empty() { name } else { format!("{name}({})", args.join(", ")) }
        }
    }
}

/// A value no row matches, one description per column, or `None` when the rows cover everything.
pub fn witness(rows: &[Vec<Pat>], tys: &[Ty], v: Variants) -> Option<Vec<String>> {
    if tys.is_empty() {
        return rows.is_empty().then(Vec::new);
    }
    let rows = expand(rows);
    let seen = used(&rows);
    if let Some(all) = signature(&tys[0], v)
        && all.iter().all(|c| seen.contains(c))
    {
        for c in &all {
            let sub = arity(&tys[0], c, v);
            let mut t2 = sub.clone();
            t2.extend_from_slice(&tys[1..]);
            if let Some(w) = witness(&specialize(&rows, c, sub.len()), &t2, v) {
                let head = describe(&tys[0], c, &w[..sub.len()], v);
                let mut out = vec![head];
                out.extend_from_slice(&w[sub.len()..]);
                return Some(out);
            }
        }
        return None;
    }
    let rest = witness(&default_rows(&rows), &tys[1..], v)?;
    let head = match signature(&tys[0], v) {
        Some(all) => {
            let missing = all.iter().find(|c| !seen.contains(c)).expect("an incomplete signature");
            let n = arity(&tys[0], missing, v).len();
            describe(&tys[0], missing, &vec!["_".to_owned(); n], v)
        }
        None => "_".to_owned(),
    };
    let mut out = vec![head];
    out.extend(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option() -> Vec<(String, Vec<Ty>)> {
        vec![("None".into(), vec![]), ("Some".into(), vec![Ty::Param(0)])]
    }

    fn v(_: EnumId) -> Vec<(String, Vec<Ty>)> {
        option()
    }

    fn opt_int() -> Ty {
        Ty::Enum(EnumId(0), vec![Ty::Int])
    }

    fn some(p: Pat) -> Pat {
        Pat::Variant { index: 1, fields: vec![p] }
    }

    fn none() -> Pat {
        Pat::Variant { index: 0, fields: vec![] }
    }

    #[test]
    fn finds_the_missing_variant() {
        let rows = vec![vec![some(Pat::Wild)]];
        assert_eq!(witness(&rows, &[opt_int()], &v), Some(vec!["None".to_owned()]));
        let rows = vec![vec![some(Pat::Wild)], vec![none()]];
        assert_eq!(witness(&rows, &[opt_int()], &v), None);
    }

    #[test]
    fn literals_need_a_catch_all() {
        let rows = vec![vec![Pat::Int(1)], vec![Pat::Int(2)]];
        assert_eq!(witness(&rows, &[Ty::Int], &v), Some(vec!["_".to_owned()]));
        let rows = vec![vec![Pat::Int(1)], vec![Pat::Wild]];
        assert_eq!(witness(&rows, &[Ty::Int], &v), None);
        let rows = vec![vec![Pat::Bool(true)]];
        assert_eq!(witness(&rows, &[Ty::Bool], &v), Some(vec!["false".to_owned()]));
    }

    #[test]
    fn nested_patterns_are_covered_column_by_column() {
        let nested = Ty::Enum(EnumId(0), vec![opt_int()]);
        let rows = vec![vec![none()], vec![some(none())]];
        assert_eq!(witness(&rows, std::slice::from_ref(&nested), &v), Some(vec!["Some(Some(_))".to_owned()]));
        let rows = vec![vec![none()], vec![some(none())], vec![some(some(Pat::Wild))]];
        assert_eq!(witness(&rows, &[nested], &v), None);
    }

    #[test]
    fn an_arm_after_a_catch_all_is_useless() {
        let rows = vec![vec![Pat::Wild]];
        assert!(!useful(&rows, &[some(Pat::Wild)], &[opt_int()], &v));
        let rows = vec![vec![some(Pat::Int(1))]];
        assert!(useful(&rows, &[some(Pat::Int(2))], &[opt_int()], &v));
        assert!(!useful(&rows, &[some(Pat::Int(1))], &[opt_int()], &v));
    }

    #[test]
    fn alternatives_count_as_separate_rows() {
        let rows = vec![vec![Pat::Or(vec![none(), some(Pat::Wild)])]];
        assert_eq!(witness(&rows, &[opt_int()], &v), None);
    }
}
