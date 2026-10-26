//! The data pack container: functions, tags and other JSON resources, with whole-pack
//! validation and output to a directory or a zip.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use super::command::{CallArgs, FunctionCall, FunctionRef};
use super::error::{Check, Error, Result, bail};
use super::function::{Function, Line};
use super::json::Json;
use super::names::{IdOrTag, ResourceLocation};
use super::target::Target;
use super::zip::write_zip;

/// Data pack format of the default target (26.3 = 121.0); see [`Target`] for the others.
pub const PACK_FORMAT: u32 = 121;

/// An entry of a tag file.
#[derive(Clone, Debug, PartialEq)]
pub struct TagEntry {
    pub id: IdOrTag,
    /// `false` lets the tag load even if the entry doesn't exist.
    pub required: bool,
}

impl TagEntry {
    pub fn required(id: IdOrTag) -> Self {
        TagEntry { id, required: true }
    }
}

/// A tag file, `data/<ns>/tags/<registry>/<path>.json`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Tag {
    /// Discard entries contributed by lower-priority packs.
    pub replace: bool,
    pub values: Vec<TagEntry>,
}

impl Tag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, id: IdOrTag) -> Self {
        self.values.push(TagEntry::required(id));
        self
    }

    fn to_json(&self) -> Json {
        let mut obj = Vec::new();
        if self.replace {
            obj.push(("replace".to_owned(), Json::Bool(true)));
        }
        let values = self
            .values
            .iter()
            .map(|e| {
                let id = Json::str(e.id.to_string());
                if e.required {
                    id
                } else {
                    Json::object([("id", id), ("required", Json::Bool(false))])
                }
            })
            .collect();
        obj.push(("values".to_owned(), Json::Array(values)));
        Json::Object(obj)
    }
}

/// One problem found while validating a pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// e.g. `function ns:main line 4`
    pub location: String,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

/// Why a pack could not be built.
#[derive(Debug)]
pub enum BuildError {
    Invalid(Vec<Diagnostic>),
    Io(io::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Invalid(ds) => {
                write!(f, "data pack is invalid ({} problem{}):", ds.len(), if ds.len() == 1 { "" } else { "s" })?;
                for d in ds {
                    write!(f, "\n  {d}")?;
                }
                Ok(())
            }
            BuildError::Io(e) => write!(f, "i/o error: {e}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<io::Error> for BuildError {
    fn from(e: io::Error) -> Self {
        BuildError::Io(e)
    }
}

#[derive(Clone, Debug)]
pub struct DataPack {
    description: String,
    target: Target,
    functions: BTreeMap<ResourceLocation, Function>,
    /// registry folder (`function`, `block`, ...) + id -> tag
    tags: BTreeMap<(String, ResourceLocation), Tag>,
    /// registry folder (`predicate`, `advancement`, ...) + id -> JSON file
    resources: BTreeMap<(String, ResourceLocation), Json>,
}

fn check_registry(registry: &str) -> Result<()> {
    let ok = !registry.is_empty()
        && registry.split('/').all(|s| !s.is_empty() && s.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_')));
    if !ok {
        bail!("invalid registry folder {registry:?}");
    }
    Ok(())
}

impl DataPack {
    pub fn new(description: impl Into<String>) -> Self {
        DataPack { description: description.into(), target: Target::default(), functions: BTreeMap::new(), tags: BTreeMap::new(), resources: BTreeMap::new() }
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// The game version the pack declares in `pack.mcmeta` (default: the newest, 26.3).
    pub fn set_target(&mut self, target: Target) {
        self.target = target;
    }

    pub fn target(&self) -> Target {
        self.target
    }

    /// Adds a function, `data/<ns>/function/<path>.mcfunction`.
    pub fn add_function(&mut self, id: ResourceLocation, function: Function) -> Result<()> {
        if self.functions.contains_key(&id) {
            bail!("function {id} is defined twice");
        }
        self.functions.insert(id, function);
        Ok(())
    }

    pub fn function(&self, id: &ResourceLocation) -> Option<&Function> {
        self.functions.get(id)
    }

    pub fn function_mut(&mut self, id: &ResourceLocation) -> Option<&mut Function> {
        self.functions.get_mut(id)
    }

    pub fn functions(&self) -> impl Iterator<Item = (&ResourceLocation, &Function)> {
        self.functions.iter()
    }

    /// Adds a tag file for `registry` (`function` for function tags).
    pub fn add_tag(&mut self, registry: &str, id: ResourceLocation, tag: Tag) -> Result<()> {
        check_registry(registry)?;
        let key = (registry.to_owned(), id);
        if self.tags.contains_key(&key) {
            bail!("tag {registry}/{} is defined twice", key.1);
        }
        self.tags.insert(key, tag);
        Ok(())
    }

    fn append_function_tag(&mut self, tag_id: &str, function: ResourceLocation) {
        let key = ("function".to_owned(), ResourceLocation::new("minecraft", tag_id).expect("static id"));
        let tag = self.tags.entry(key).or_default();
        let entry = TagEntry::required(IdOrTag::Id(function));
        if !tag.values.contains(&entry) {
            tag.values.push(entry);
        }
    }

    /// Runs `function` whenever the pack loads (`#minecraft:load`), including every `/reload`.
    /// It runs before any player is present and must be idempotent.
    pub fn on_load(&mut self, function: ResourceLocation) {
        self.append_function_tag("load", function);
    }

    /// Runs `function` at the start of every game tick (`#minecraft:tick`).
    pub fn on_tick(&mut self, function: ResourceLocation) {
        self.append_function_tag("tick", function);
    }

    /// Adds a JSON file such as a predicate or advancement:
    /// `data/<ns>/<registry>/<path>.json`. For 26.3 JSON shapes see docs/mc/04-data-formats.md.
    pub fn add_resource(&mut self, registry: &str, id: ResourceLocation, json: Json) -> Result<()> {
        check_registry(registry)?;
        let key = (registry.to_owned(), id);
        if self.resources.contains_key(&key) {
            bail!("resource {}/{} is defined twice", key.0, key.1);
        }
        self.resources.insert(key, json);
        Ok(())
    }

    /// Checks every command and the references between functions and tags.
    /// An empty result means the pack is valid.
    pub fn validate(&self) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let mut err = |location: String, e: Error| out.push(Diagnostic { location, message: e.message().to_owned() });

        // Namespaces this pack defines functions/tags in. References into other namespaces
        // (vanilla or other packs) can't be checked here.
        let owned: BTreeSet<&str> = self
            .functions
            .keys()
            .chain(self.tags.iter().filter(|((reg, _), _)| reg == "function").map(|((_, id), _)| id))
            .map(ResourceLocation::namespace)
            .filter(|ns| *ns != "minecraft")
            .collect();

        let owned_resources: BTreeSet<&str> = owned.iter().copied().chain(self.resources.keys().map(|(_, id)| id.namespace())).collect();

        let macro_vars: BTreeMap<&ResourceLocation, BTreeSet<String>> =
            self.functions.iter().map(|(id, f)| (id, f.macro_vars())).collect();

        for (id, func) in &self.functions {
            for (idx, line) in func.lines().iter().enumerate() {
                let line_no = idx + 1;
                let Line::Command(cmd) = line else { continue };
                let at = || format!("function {id} line {line_no}");
                if let Err(e) = cmd.check() {
                    err(at(), e);
                    continue;
                }
                let mut predicates = Vec::new();
                cmd.predicate_refs(&mut predicates);
                for pred in predicates {
                    if owned_resources.contains(pred.namespace()) && !self.resources.contains_key(&("predicate".to_owned(), pred.clone())) {
                        err(at(), Error::new(format!("`execute if predicate {pred}` needs that predicate to exist when the function loads")));
                    }
                }
                let mut calls: Vec<FunctionCall> = Vec::new();
                cmd.function_calls(&mut calls);
                for call in calls {
                    match &call.target {
                        FunctionRef::Id(target) if owned.contains(target.namespace()) => match self.functions.get(target) {
                            None => err(at(), Error::new(format!("calls unknown function {target}"))),
                            Some(_) => {
                                let needed = &macro_vars[target];
                                if needed.is_empty() {
                                    continue;
                                }
                                match &call.args {
                                    CallArgs::None => err(
                                        at(),
                                        Error::new(format!("{target} is a macro function (uses {:?}) but is called without arguments", needed)),
                                    ),
                                    CallArgs::Inline(keys) => {
                                        let missing: Vec<_> = needed.iter().filter(|v| !keys.contains(v)).collect();
                                        if !missing.is_empty() {
                                            err(at(), Error::new(format!("call to {target} is missing macro arguments {missing:?}")));
                                        }
                                    }
                                    CallArgs::With => {}
                                }
                            }
                        },
                        FunctionRef::Tag(tag) if owned.contains(tag.namespace())
                            && !self.tags.contains_key(&("function".to_owned(), tag.clone())) => {
                                err(at(), Error::new(format!("references unknown function tag #{tag}")));
                            }
                        _ => {}
                    }
                }
            }
        }

        for ((registry, id), tag) in &self.tags {
            if registry != "function" {
                continue;
            }
            for e in tag.values.iter().filter(|e| e.required) {
                let missing = match &e.id {
                    IdOrTag::Id(f) => owned.contains(f.namespace()) && !self.functions.contains_key(f),
                    IdOrTag::Tag(t) => owned.contains(t.namespace()) && !self.tags.contains_key(&("function".to_owned(), t.clone())),
                };
                if missing {
                    err(format!("function tag #{id}"), Error::new(format!("lists {} which doesn't exist", e.id)));
                }
            }
        }
        out
    }

    /// The pack as `path -> bytes`, with `/`-separated paths relative to the pack root.
    /// Fails with every validation problem found.
    pub fn files(&self) -> std::result::Result<BTreeMap<String, Vec<u8>>, BuildError> {
        let problems = self.validate();
        if !problems.is_empty() {
            return Err(BuildError::Invalid(problems));
        }
        let mut files = BTreeMap::new();
        let mcmeta = Json::object([(
            "pack",
            Json::object([
                ("description", Json::str(self.description.clone())),
                ("min_format", Json::Array(vec![Json::Int(self.target.format.0 as i64), Json::Int(self.target.format.1 as i64)])),
                ("max_format", Json::Int(self.target.format.0 as i64)),
            ]),
        )]);
        files.insert("pack.mcmeta".to_owned(), mcmeta.pretty().into_bytes());
        for (id, f) in &self.functions {
            files.insert(format!("data/{}/function/{}.mcfunction", id.namespace(), id.path()), f.render().into_bytes());
        }
        for ((registry, id), tag) in &self.tags {
            files.insert(format!("data/{}/tags/{registry}/{}.json", id.namespace(), id.path()), tag.to_json().pretty().into_bytes());
        }
        for ((registry, id), json) in &self.resources {
            files.insert(format!("data/{}/{registry}/{}.json", id.namespace(), id.path()), json.pretty().into_bytes());
        }
        Ok(files)
    }

    /// Writes the pack into `dir` as a folder pack. With `clean`, first removes the pack's
    /// previous output (`pack.mcmeta` and `data/`), but only if `dir` already looks like a
    /// pack, so a wrong path can't delete unrelated files.
    pub fn write_dir(&self, dir: &Path, clean: bool) -> std::result::Result<(), BuildError> {
        let files = self.files()?;
        if clean {
            let data = dir.join("data");
            let mcmeta = dir.join("pack.mcmeta");
            if mcmeta.is_file() {
                fs::remove_file(&mcmeta)?;
                if data.is_dir() {
                    fs::remove_dir_all(&data)?;
                }
            } else if data.exists() {
                return Err(BuildError::Io(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("refusing to clean {}: it has a data/ folder but no pack.mcmeta", dir.display()),
                )));
            }
        }
        for (rel, bytes) in &files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, bytes)?;
        }
        Ok(())
    }

    /// Writes the pack as a `.zip` (pack.mcmeta at the archive root, as the game requires).
    pub fn write_zip(&self, path: &Path) -> std::result::Result<(), BuildError> {
        let files = self.files()?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, write_zip(&files))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datapack::command::{Command, FnArgs, MacroCommand};
    use crate::datapack::names::rl;
    use crate::datapack::nbt::Compound;

    fn simple() -> DataPack {
        let mut p = DataPack::new("test");
        let mut load = Function::new();
        load.push(Command::say("loaded"));
        p.add_function(rl("t:load"), load).unwrap();
        p.on_load(rl("t:load"));
        p
    }

    #[test]
    fn layout_and_mcmeta() {
        let files = simple().files().unwrap();
        let keys: Vec<_> = files.keys().map(String::as_str).collect();
        assert_eq!(keys, ["data/minecraft/tags/function/load.json", "data/t/function/load.mcfunction", "pack.mcmeta"]);
        assert_eq!(
            String::from_utf8_lossy(&files["data/minecraft/tags/function/load.json"]),
            "{\n  \"values\": [\n    \"t:load\"\n  ]\n}\n"
        );
        assert_eq!(String::from_utf8_lossy(&files["data/t/function/load.mcfunction"]), "say loaded\n");
        let mcmeta = String::from_utf8_lossy(&files["pack.mcmeta"]).into_owned();
        assert!(mcmeta.contains("\"min_format\": [\n      121,\n      0\n    ]"), "{mcmeta}");
        assert!(mcmeta.contains("\"max_format\": 121"));
        // another target changes the declared formats
        let mut p = simple();
        p.set_target(Target::parse("26.1").unwrap());
        let mcmeta = String::from_utf8_lossy(&p.files().unwrap()["pack.mcmeta"]).into_owned();
        assert!(mcmeta.contains("\"min_format\": [\n      101,\n      1\n    ]") && mcmeta.contains("\"max_format\": 101"), "{mcmeta}");
    }

    #[test]
    fn duplicate_function_is_rejected() {
        let mut p = simple();
        assert!(p.add_function(rl("t:load"), Function::new()).is_err());
    }

    #[test]
    fn unknown_function_reference() {
        let mut p = simple();
        let mut f = Function::new();
        f.push(Command::call(rl("t:missing"))).push(Command::call(rl("other:fine")));
        p.add_function(rl("t:main"), f).unwrap();
        let problems = p.validate();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].location, "function t:main line 1");
        assert!(problems[0].message.contains("t:missing"));
    }

    #[test]
    fn tag_entries_must_exist() {
        let mut p = DataPack::new("x");
        p.on_tick(rl("t:gone"));
        // t:gone isn't defined, and the `t` namespace has no functions at all, so it isn't
        // "owned"; define one function to own it.
        p.add_function(rl("t:other"), Function::new()).unwrap();
        let problems = p.validate();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].message.contains("t:gone"));
    }

    #[test]
    fn macro_call_arguments_are_checked() {
        let mut p = DataPack::new("x");
        let mut mac = Function::new();
        mac.push(Command::Macro(MacroCommand::new().text("say ").var("who")));
        p.add_function(rl("t:mac"), mac).unwrap();

        let mut caller = Function::new();
        caller.push(Command::call(rl("t:mac")));
        caller.push(Command::Function { function: FunctionRef::Id(rl("t:mac")), args: FnArgs::Inline(Compound::new().with("nope", 1)) });
        caller.push(Command::Function { function: FunctionRef::Id(rl("t:mac")), args: FnArgs::Inline(Compound::new().with("who", "me")) });
        p.add_function(rl("t:main"), caller).unwrap();
        let problems = p.validate();
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].message.contains("without arguments"));
        assert!(problems[1].message.contains("missing macro arguments"));
    }

    #[test]
    fn predicate_refs_must_exist() {
        let mut p = simple();
        let mut f = Function::new();
        f.push(Command::execute().if_(crate::datapack::Condition::Predicate(rl("t:p"))).run(Command::say("x")));
        p.add_function(rl("t:main"), f).unwrap();
        assert_eq!(p.validate().len(), 1);
        p.add_resource("predicate", rl("t:p"), Json::object([("type", Json::str("minecraft:random_chance"))])).unwrap();
        assert!(p.validate().is_empty());
    }

    #[test]
    fn invalid_commands_block_output() {
        let mut p = simple();
        let mut f = Function::new();
        f.push(Command::Raw("/say bad".into()));
        p.add_function(rl("t:bad"), f).unwrap();
        assert!(matches!(p.files(), Err(BuildError::Invalid(_))));
    }

    #[test]
    fn write_dir_and_clean_guard() {
        let tmp = std::env::temp_dir().join(format!("slab-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let p = simple();
        p.write_dir(&tmp, true).unwrap();
        assert!(tmp.join("pack.mcmeta").is_file());
        assert!(tmp.join("data/t/function/load.mcfunction").is_file());
        // stale files are removed on rebuild
        fs::write(tmp.join("data/t/function/stale.mcfunction"), "say old").unwrap();
        p.write_dir(&tmp, true).unwrap();
        assert!(!tmp.join("data/t/function/stale.mcfunction").exists());
        // a directory with data/ but no pack.mcmeta is not touched
        fs::remove_file(tmp.join("pack.mcmeta")).unwrap();
        assert!(p.write_dir(&tmp, true).is_err());
        assert!(tmp.join("data/t/function/load.mcfunction").is_file());
        fs::remove_dir_all(&tmp).unwrap();
    }
}
