//! `Slab.toml`, the file that describes a project the way `Cargo.toml` does:
//!
//! ```toml
//! [project]
//! name = "my_pack"            # required; also the default namespace (lowercased)
//! version = "0.1.0"
//! description = "Shown by /datapack list"
//! namespace = "my_pack"       # optional: when the pack's namespace differs from its name
//! minecraft = "26.3"          # the game version the pack is for (default: the newest, 26.3)
//!
//! [build]
//! abort-on-unwrap = false     # a failed unwrap() also runs `datapack disable`
//! pack-id = "file/my_pack"    # the pack that command disables
//! command-limit = 1000000     # raise the game's limit of commands per tick (see docs/lang/codegen.md)
//! ```
//!
//! The source code lives in `src/`, and `src/main.slab` is the root module. Only the part of TOML
//! this needs is read: `[tables]`, and `key = "text" | true | false | 123` with `#` comments.
//! Anything else (an unknown key, an array) is an error, so a typo cannot be silently ignored.

use std::path::{Path, PathBuf};

pub const FILE: &str = "Slab.toml";
/// The directory of the source code, relative to `Slab.toml`.
pub const SOURCE_DIR: &str = "src";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub namespace: Option<String>,
    pub minecraft: Option<String>,
    pub abort_on_unwrap: Option<bool>,
    pub pack_id: Option<String>,
    pub command_limit: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Str(String),
    Bool(bool),
    Int(i64),
}

impl Value {
    fn kind(&self) -> &'static str {
        match self {
            Value::Str(_) => "a string",
            Value::Bool(_) => "true or false",
            Value::Int(_) => "a number",
        }
    }
}

impl Manifest {
    /// Reads the text of a `Slab.toml`. Errors name the line.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let mut m = Manifest::default();
        let mut table = String::new();
        let mut name: Option<String> = None;
        for (i, raw) in text.lines().enumerate() {
            let at = |msg: String| format!("{FILE}:{}: {msg}", i + 1);
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                let Some(t) = rest.strip_suffix(']') else { return Err(at("a table header ends with `]`".into())) };
                let t = t.trim();
                if !matches!(t, "project" | "build") {
                    return Err(at(format!("unknown table `[{t}]` (the tables are `[project]` and `[build]`)")));
                }
                table = t.to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { return Err(at(format!("expected `key = value`, found `{line}`"))) };
            let key = key.trim();
            let value = parse_value(value.trim()).map_err(at)?;
            let want = |v: Value, kind: &str| -> Result<(), String> { if v.kind() == kind { Ok(()) } else { Err(at(format!("`{key}` must be {kind}, not {}", v.kind()))) } };
            match (table.as_str(), key) {
                ("project", "name") => {
                    want(value.clone(), "a string")?;
                    if let Value::Str(s) = value {
                        name = Some(s);
                    }
                }
                ("project", "version") | ("project", "description") | ("project", "namespace") | ("project", "minecraft") | ("build", "pack-id") => {
                    want(value.clone(), "a string")?;
                    let Value::Str(s) = value else { unreachable!() };
                    match (table.as_str(), key) {
                        ("project", "version") => m.version = Some(s),
                        ("project", "description") => m.description = Some(s),
                        ("project", "namespace") => m.namespace = Some(s),
                        ("project", "minecraft") => m.minecraft = Some(s),
                        _ => m.pack_id = Some(s),
                    }
                }
                ("build", "command-limit") => {
                    let Value::Int(n) = value else { return Err(at(format!("`command-limit` must be a number, not {}", value.kind()))) };
                    // the game's own range for the rule is 0 and up; 0 would stop everything
                    if !(1..=i64::from(i32::MAX)).contains(&n) {
                        return Err(at("`command-limit` must be between 1 and 2147483647".into()));
                    }
                    m.command_limit = Some(n as i32);
                }
                ("build", "abort-on-unwrap") => {
                    want(value.clone(), "true or false")?;
                    let Value::Bool(b) = value else { unreachable!() };
                    m.abort_on_unwrap = Some(b);
                }
                ("", _) => return Err(at(format!("`{key}` must be inside a table such as `[project]`"))),
                (t, k) => return Err(at(format!("unknown key `{k}` in `[{t}]`"))),
            }
        }
        m.name = name.ok_or_else(|| format!("{FILE}: `[project]` needs a `name`"))?;
        if m.name.trim().is_empty() {
            return Err(format!("{FILE}: `name` is empty"));
        }
        Ok(m)
    }

    /// The pack's namespace: `namespace`, else the name with every character that is not a
    /// letter or digit turned into `_`.
    pub fn namespace(&self) -> String {
        match &self.namespace {
            Some(ns) => ns.clone(),
            None => sanitize(&self.name),
        }
    }
}

/// `My-Cool Game` -> `my_cool_game`.
pub fn sanitize(name: &str) -> String {
    let ns: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    if ns.is_empty() { "slab".to_owned() } else { ns }
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' if in_str => escaped = !escaped,
            '"' if !escaped => in_str = !in_str,
            '#' if !in_str => return &line[..i],
            _ => escaped = false,
        }
        if c != '\\' {
            escaped = false;
        }
    }
    line
}

fn parse_value(v: &str) -> Result<Value, String> {
    if let Some(rest) = v.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        loop {
            match chars.next() {
                None => return Err("the string is not closed".into()),
                Some('"') => break,
                Some('\\') => match chars.next() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    other => return Err(format!("unknown escape `\\{}`", other.map(String::from).unwrap_or_default())),
                },
                Some(c) => out.push(c),
            }
        }
        if !chars.as_str().trim().is_empty() {
            return Err(format!("unexpected `{}` after the string", chars.as_str().trim()));
        }
        return Ok(Value::Str(out));
    }
    match v {
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        "" => Err("expected a value after `=`".into()),
        _ => v.parse::<i64>().map(Value::Int).map_err(|_| format!("cannot read the value `{v}` (text goes in \"quotes\"; arrays and tables are not supported)")),
    }
}

/// The directory with a `Slab.toml`, searching `start` and then its parents (like Cargo).
pub fn find_dir(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let mut dir = if start.is_dir() { start } else { start.parent()?.to_path_buf() };
    loop {
        if dir.join(FILE).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Reads `<dir>/Slab.toml`.
pub fn load(dir: &Path) -> Result<Manifest, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Manifest::parse(&text).map_err(|m| format!("{}: {m}", path.display()))
}

/// The `Slab.toml` template `slab new` writes.
pub fn template(name: &str) -> String {
    format!("[project]\nname = \"{name}\"\nversion = \"0.1.0\"\ndescription = \"\"\nminecraft = \"26.3\"\n\n[build]\n# abort-on-unwrap = true\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fields() {
        let m = Manifest::parse("# a pack\n[project]\nname = \"My Pack\"  # the name\nversion = \"1.2.3\"\ndescription = \"say \\\"hi\\\" # not a comment\"\n\n[build]\nabort-on-unwrap = true\npack-id = \"file/x\"\n").unwrap();
        assert_eq!(m.name, "My Pack");
        assert_eq!(m.namespace(), "my_pack");
        assert_eq!(m.version.as_deref(), Some("1.2.3"));
        assert_eq!(m.description.as_deref(), Some("say \"hi\" # not a comment"));
        assert_eq!((m.abort_on_unwrap, m.pack_id.as_deref()), (Some(true), Some("file/x")));
        let m = Manifest::parse("[project]\nname = \"a\"\n[build]\ncommand-limit = 1000000\n").unwrap();
        assert_eq!(m.command_limit, Some(1_000_000));
        let m = Manifest::parse("[project]\nname = \"a\"\nnamespace = \"custom\"\nminecraft = \"26.2\"\n").unwrap();
        assert_eq!(m.namespace(), "custom");
        assert_eq!(m.minecraft.as_deref(), Some("26.2"));
    }

    #[test]
    fn mistakes_name_the_line() {
        let err = |t: &str| Manifest::parse(t).unwrap_err();
        assert!(err("[project]\nversion = \"1\"\n").contains("needs a `name`"));
        assert!(err("[project]\nname = \"a\"\nnmae = \"b\"\n").contains("Slab.toml:3: unknown key `nmae` in `[project]`"));
        assert!(err("[package]\n").contains("unknown table `[package]`"));
        assert!(err("name = \"a\"\n").contains("must be inside a table"));
        assert!(err("[project]\nname = 5\n").contains("`name` must be a string, not a number"));
        assert!(err("[build]\nabort-on-unwrap = \"yes\"\n[project]\nname=\"a\"").contains("must be true or false"));
        assert!(err("[project]\nname = \"a\n").contains("not closed"));
        assert!(err("[project]\nname = [\"a\"]\n").contains("arrays and tables are not supported"));
        assert!(err("[project\n").contains("ends with `]`"));
        assert!(err("[build]\ncommand-limit = 0\n[project]\nname=\"a\"").contains("between 1 and 2147483647"));
        assert!(err("[build]\ncommand-limit = \"big\"\n[project]\nname=\"a\"").contains("must be a number"));
    }
}
