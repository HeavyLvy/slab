//! SNBT values and NBT paths (docs/mc/04-data-formats.md section 1, docs/mc/02-commands-core.md 2.4).

use std::fmt::{self, Write};

use super::error::{Check, Result, bail};
use super::names::Hole;

/// A compound whose entries keep insertion order (so output is deterministic).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Compound(Vec<(String, Nbt)>);

impl Compound {
    pub fn new() -> Self {
        Compound(Vec::new())
    }

    /// Builder form of [`Compound::insert`].
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Nbt>) -> Self {
        self.insert(key, value);
        self
    }

    /// Inserts, replacing an existing entry with the same key in place.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Nbt>) {
        let key = key.into();
        let value = value.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Nbt> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Nbt)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Nbt {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    List(Vec<Nbt>),
    Compound(Compound),
    ByteArray(Vec<i8>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Nbt {
    /// NBT has no boolean; `true`/`false` are the bytes `1b`/`0b`.
    pub fn bool(b: bool) -> Nbt {
        Nbt::Byte(b as i8)
    }

    pub fn str(s: impl Into<String>) -> Nbt {
        Nbt::String(s.into())
    }

    pub fn as_compound(&self) -> Option<&Compound> {
        match self {
            Nbt::Compound(c) => Some(c),
            _ => None,
        }
    }
}

impl From<i8> for Nbt {
    fn from(v: i8) -> Nbt {
        Nbt::Byte(v)
    }
}
impl From<i16> for Nbt {
    fn from(v: i16) -> Nbt {
        Nbt::Short(v)
    }
}
impl From<i32> for Nbt {
    fn from(v: i32) -> Nbt {
        Nbt::Int(v)
    }
}
impl From<i64> for Nbt {
    fn from(v: i64) -> Nbt {
        Nbt::Long(v)
    }
}
impl From<f32> for Nbt {
    fn from(v: f32) -> Nbt {
        Nbt::Float(v)
    }
}
impl From<f64> for Nbt {
    fn from(v: f64) -> Nbt {
        Nbt::Double(v)
    }
}
impl From<bool> for Nbt {
    fn from(v: bool) -> Nbt {
        Nbt::bool(v)
    }
}
impl From<&str> for Nbt {
    fn from(v: &str) -> Nbt {
        Nbt::String(v.to_owned())
    }
}
impl From<String> for Nbt {
    fn from(v: String) -> Nbt {
        Nbt::String(v)
    }
}
impl From<Compound> for Nbt {
    fn from(v: Compound) -> Nbt {
        Nbt::Compound(v)
    }
}
impl From<Vec<Nbt>> for Nbt {
    fn from(v: Vec<Nbt>) -> Nbt {
        Nbt::List(v)
    }
}

/// Keys are written bare when they only use `[A-Za-z0-9_+-]` (`.` is left quoted so the
/// same helper is safe in paths), otherwise quoted.
fn is_bare_key(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+'))
}

/// Writes a double-quoted SNBT string. Control characters are escaped so a value never
/// spans lines in a `.mcfunction` file.
pub(crate) fn write_quoted(out: &mut impl Write, s: &str) -> fmt::Result {
    out.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\n' => out.write_str("\\n")?,
            '\r' => out.write_str("\\r")?,
            '\t' => out.write_str("\\t")?,
            // a literal `$` must never form a macro placeholder `$(`, so it is always written as an escape
            '$' => out.write_str("\\u0024")?,
            c if (c as u32) < 0x20 || c as u32 == 0x7f => write!(out, "\\u{:04x}", c as u32)?,
            c => out.write_char(c)?,
        }
    }
    out.write_char('"')
}

fn write_key(out: &mut impl Write, key: &str) -> fmt::Result {
    if is_bare_key(key) { out.write_str(key) } else { write_quoted(out, key) }
}

fn write_list<T>(
    f: &mut fmt::Formatter<'_>,
    items: &[T],
    mut each: impl FnMut(&mut fmt::Formatter<'_>, &T) -> fmt::Result,
) -> fmt::Result {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_char(',')?;
        }
        each(f, item)?;
    }
    Ok(())
}

impl fmt::Display for Compound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('{')?;
        for (i, (k, v)) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_char(',')?;
            }
            write_key(f, k)?;
            f.write_char(':')?;
            write!(f, "{v}")?;
        }
        f.write_char('}')
    }
}

impl fmt::Display for Nbt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Nbt::Byte(v) => write!(f, "{v}b"),
            Nbt::Short(v) => write!(f, "{v}s"),
            Nbt::Int(v) => write!(f, "{v}"),
            Nbt::Long(v) => write!(f, "{v}L"),
            Nbt::Float(v) => write!(f, "{v}f"),
            Nbt::Double(v) => write!(f, "{v}d"),
            Nbt::String(s) => write_quoted(f, s),
            Nbt::List(items) => {
                f.write_char('[')?;
                write_list(f, items, |f, v| write!(f, "{v}"))?;
                f.write_char(']')
            }
            Nbt::Compound(c) => write!(f, "{c}"),
            Nbt::ByteArray(items) => {
                f.write_str("[B;")?;
                write_list(f, items, |f, v| write!(f, "{v}b"))?;
                f.write_char(']')
            }
            Nbt::IntArray(items) => {
                f.write_str("[I;")?;
                write_list(f, items, |f, v| write!(f, "{v}"))?;
                f.write_char(']')
            }
            Nbt::LongArray(items) => {
                f.write_str("[L;")?;
                write_list(f, items, |f, v| write!(f, "{v}L"))?;
                f.write_char(']')
            }
        }
    }
}

impl Check for Nbt {
    fn check(&self) -> Result<()> {
        match self {
            Nbt::Float(v) if !v.is_finite() => bail!("NBT float must be finite, got {v}"),
            Nbt::Double(v) if !v.is_finite() => bail!("NBT double must be finite, got {v}"),
            Nbt::List(items) => items.iter().try_for_each(Check::check),
            Nbt::Compound(c) => c.check(),
            _ => Ok(()),
        }
    }
}

impl Check for Compound {
    fn check(&self) -> Result<()> {
        self.0.iter().try_for_each(|(_, v)| v.check())
    }
}

/// One step of an NBT path.
#[derive(Clone, Debug, PartialEq)]
pub enum PathNode {
    /// `name` (quoted when needed).
    Key(String),
    /// `name{...}`: child compound that partially matches the filter.
    KeyFilter(String, Compound),
    /// `{...}` as the very first node: filter on the root itself.
    RootFilter(Compound),
    /// `[i]`; negative counts from the end.
    Index(i32),
    /// `[$(hN)]`: an index supplied at run time (a macro argument).
    IndexHole(Hole),
    /// `[]`: every element.
    All,
    /// `[{...}]`: every compound element matching the filter.
    ElementFilter(Compound),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct NbtPath(Vec<PathNode>);

impl NbtPath {
    pub fn new() -> Self {
        NbtPath(Vec::new())
    }

    pub fn key(self, name: impl Into<String>) -> Self {
        self.push(PathNode::Key(name.into()))
    }

    pub fn key_filter(self, name: impl Into<String>, filter: Compound) -> Self {
        self.push(PathNode::KeyFilter(name.into(), filter))
    }

    pub fn root_filter(self, filter: Compound) -> Self {
        self.push(PathNode::RootFilter(filter))
    }

    pub fn index(self, i: i32) -> Self {
        self.push(PathNode::Index(i))
    }

    pub fn index_hole(self, h: Hole) -> Self {
        self.push(PathNode::IndexHole(h))
    }

    pub fn all(self) -> Self {
        self.push(PathNode::All)
    }

    pub fn element_filter(self, filter: Compound) -> Self {
        self.push(PathNode::ElementFilter(filter))
    }

    pub fn push(mut self, node: PathNode) -> Self {
        self.0.push(node);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn nodes(&self) -> &[PathNode] {
        &self.0
    }
}

impl fmt::Display for NbtPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, node) in self.0.iter().enumerate() {
            let bracket = matches!(node, PathNode::Index(_) | PathNode::IndexHole(_) | PathNode::All | PathNode::ElementFilter(_));
            if i > 0 && !bracket {
                f.write_char('.')?;
            }
            match node {
                PathNode::Key(k) => write_key(f, k)?,
                PathNode::KeyFilter(k, c) => {
                    write_key(f, k)?;
                    write!(f, "{c}")?;
                }
                PathNode::RootFilter(c) => write!(f, "{c}")?,
                PathNode::Index(n) => write!(f, "[{n}]")?,
                PathNode::IndexHole(h) => write!(f, "[{h}]")?,
                PathNode::All => f.write_str("[]")?,
                PathNode::ElementFilter(c) => write!(f, "[{c}]")?,
            }
        }
        Ok(())
    }
}

impl Check for NbtPath {
    fn check(&self) -> Result<()> {
        for (i, node) in self.0.iter().enumerate() {
            match node {
                PathNode::Key(k) | PathNode::KeyFilter(k, _) if k.is_empty() => {
                    bail!("empty key in NBT path (not traversable since 1.21.5)")
                }
                PathNode::RootFilter(_) if i != 0 => bail!("a root filter {{...}} is only valid as the first path node"),
                PathNode::KeyFilter(_, c) | PathNode::RootFilter(c) | PathNode::ElementFilter(c) => c.check()?,
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snbt_rendering() {
        let c = Compound::new()
            .with("a", 1)
            .with("b", Nbt::Byte(1))
            .with("long key", "he said \"hi\"\n")
            .with("f", 1.5f32)
            .with("d", 2.0f64)
            .with("l", 5i64)
            .with("list", vec![Nbt::Int(1), Nbt::Int(2)])
            .with("ia", Nbt::IntArray(vec![1, 2]))
            .with("ba", Nbt::ByteArray(vec![1, -2]))
            .with("la", Nbt::LongArray(vec![3]));
        assert_eq!(
            c.to_string(),
            r#"{a:1,b:1b,"long key":"he said \"hi\"\n",f:1.5f,d:2d,l:5L,list:[1,2],ia:[I;1,2],ba:[B;1b,-2b],la:[L;3L]}"#
        );
        assert_eq!(Compound::new().to_string(), "{}");
        assert_eq!(Nbt::List(vec![]).to_string(), "[]");
    }

    #[test]
    fn dollar_signs_in_strings_never_look_like_macros() {
        let n = Nbt::str("cost $(5) or $x");
        assert_eq!(n.to_string(), r#""cost \u0024(5) or \u0024x""#);
        assert!(!n.to_string().contains("$("));
    }

    #[test]
    fn index_holes_render_as_placeholders() {
        let p = NbtPath::new().key("xs").index_hole(Hole(0)).key("a").index_hole(Hole(1));
        assert_eq!(p.to_string(), "xs[$(h0)].a[$(h1)]");
    }

    #[test]
    fn insert_replaces_in_place() {
        let c = Compound::new().with("a", 1).with("b", 2).with("a", 3);
        assert_eq!(c.to_string(), "{a:3,b:2}");
    }

    #[test]
    fn path_rendering() {
        let p = NbtPath::new().key("a").key("b.c").index(0).key("d").all().key("e").element_filter(Compound::new().with("id", "x"));
        assert_eq!(p.to_string(), r#"a."b.c"[0].d[].e[{id:"x"}]"#);
        let p = NbtPath::new().root_filter(Compound::new().with("k", 1)).key("z");
        assert_eq!(p.to_string(), "{k:1}.z");
        let p = NbtPath::new().key_filter("Items", Compound::new().with("Slot", Nbt::Byte(0)));
        assert_eq!(p.to_string(), "Items{Slot:0b}");
    }

    #[test]
    fn path_check() {
        assert!(NbtPath::new().key("a").root_filter(Compound::new()).check().is_err());
        assert!(NbtPath::new().key("").check().is_err());
        assert!(NbtPath::new().root_filter(Compound::new()).key("a").check().is_ok());
    }
}
