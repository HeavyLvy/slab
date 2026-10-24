//! Validated identifier newtypes. Constructing one is the point where invalid names are
//! rejected, so the rest of the AST can't hold a name that Minecraft would refuse.

use std::fmt;
use std::str::FromStr;

use super::error::{Error, Result, bail};

/// `namespace:path` (docs/mc/01-datapack-structure.md section 3).
/// Namespace: `[a-z0-9_.-]`; path: `[a-z0-9_.-/]`. Always rendered with the namespace.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ResourceLocation {
    namespace: String,
    path: String,
}

fn ns_char(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9' | '_' | '-' | '.')
}

impl ResourceLocation {
    pub fn new(namespace: &str, path: &str) -> Result<Self> {
        if namespace.is_empty() || !namespace.chars().all(ns_char) || namespace.chars().all(|c| c == '.') {
            bail!("invalid namespace {namespace:?}: use [a-z0-9_.-] (not only dots)");
        }
        if path.is_empty() {
            bail!("empty resource path in {namespace:?}");
        }
        for seg in path.split('/') {
            // Empty/dot segments would create odd or unsafe file paths.
            if seg.is_empty() || seg == "." || seg == ".." || !seg.chars().all(ns_char) {
                bail!("invalid resource path {path:?}: segments are [a-z0-9_.-]+ and not '.'/'..'");
            }
        }
        Ok(ResourceLocation { namespace: namespace.to_owned(), path: path.to_owned() })
    }

    /// Parses `ns:path`, or `path` which defaults to the `minecraft` namespace.
    pub fn parse(s: &str) -> Result<Self> {
        match s.split_once(':') {
            Some((ns, path)) => Self::new(ns, path),
            None => Self::new("minecraft", s),
        }
    }

    pub fn minecraft(path: &str) -> Result<Self> {
        Self::new("minecraft", path)
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for ResourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl FromStr for ResourceLocation {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Convenience for literals known at compile time. Panics on invalid input; the compiler
/// frontend must use [`ResourceLocation::parse`] on user input instead.
#[track_caller]
pub fn rl(s: &str) -> ResourceLocation {
    ResourceLocation::parse(s).unwrap_or_else(|e| panic!("{e}"))
}

/// An unquoted brigadier word: `[A-Za-z0-9_.+-]+`. Used for objective names, entity tags,
/// team names and block-state property names/values.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Word(String);

impl Word {
    pub fn new(s: &str) -> Result<Self> {
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+')) {
            bail!("invalid name {s:?}: only [A-Za-z0-9_.+-] are allowed");
        }
        Ok(Word(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub type Objective = Word;

/// Panicking convenience for literals, see [`rl`].
#[track_caller]
pub fn word(s: &str) -> Word {
    Word::new(s).unwrap_or_else(|e| panic!("{e}"))
}

/// A fake-player / literal score holder name (docs/mc/02-commands-core.md 2.2): an unquoted
/// word that is not a selector. Letters and digits (including non-ASCII) and `_ - . + : #`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct HolderName(String);

impl HolderName {
    pub fn new(s: &str) -> Result<Self> {
        let ok_char = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | ':' | '#');
        if s.is_empty() || !s.chars().all(ok_char) {
            bail!("invalid score holder name {s:?}: only letters, digits and _ - . + : # are allowed");
        }
        Ok(HolderName(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HolderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Panicking convenience for literals, see [`rl`].
#[track_caller]
pub fn holder(s: &str) -> HolderName {
    HolderName::new(s).unwrap_or_else(|e| panic!("{e}"))
}

/// A runtime-substituted value in a command: renders as the macro placeholder `$(hN)`. A command
/// containing holes becomes a macro line and must run in a function called with the values
/// (`function <f> with storage <id>`), stored under the keys [`Hole::key`]. Whoever builds the
/// command numbers its holes; they only need to be unique within that command.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Hole(pub u16);

impl Hole {
    /// The macro argument name: `h0`, `h1`, ...
    pub fn key(&self) -> String {
        format!("h{}", self.0)
    }
}

impl fmt::Display for Hole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "$(h{})", self.0)
    }
}

/// A resource location or a `#`-prefixed tag of the same registry.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum IdOrTag {
    Id(ResourceLocation),
    Tag(ResourceLocation),
}

impl fmt::Display for IdOrTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdOrTag::Id(id) => write!(f, "{id}"),
            IdOrTag::Tag(id) => write!(f, "#{id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_location_rules() {
        assert_eq!(rl("foo").to_string(), "minecraft:foo");
        assert_eq!(rl("a:b/c").to_string(), "a:b/c");
        assert!(ResourceLocation::parse("Foo:bar").is_err());
        assert!(ResourceLocation::parse("a:b//c").is_err());
        assert!(ResourceLocation::parse("a:../x").is_err());
        assert!(ResourceLocation::parse("a/b:c").is_err());
        assert!(ResourceLocation::parse("..:c").is_err());
        assert!(ResourceLocation::parse("a:").is_err());
    }

    #[test]
    fn words() {
        assert!(Word::new("my_obj-1.+").is_ok());
        assert!(Word::new("a:b").is_err());
        assert!(Word::new("").is_err());
        assert!(HolderName::new("#tmp").is_ok());
        assert!(HolderName::new("a b").is_err());
    }
}
