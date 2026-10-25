//! Loading a multi-file project: `main.slab` is the root module, every other `.slab` file is a
//! module that is loaded when something imports it. `import a::b;` means the file `a/b.slab`
//! relative to the directory of `main.slab` (always from the project root, never relative to the
//! importing file).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::diag::{Diagnostic, SourceFile, SourceMap};
use crate::syntax::ast::{self, ItemKind};
use crate::syntax::parse;

pub const ROOT_FILE: &str = "main.slab";

pub struct Module {
    /// `"main"` for the root, otherwise the import path: `"maps::arena"`.
    pub name: String,
    /// Part of the embedded standard library (`std`, `game`, ...), not a file of the project.
    pub std: bool,
    /// Index into [`Project::sources`].
    pub file: usize,
    pub program: ast::Program,
}

pub struct Project {
    /// The directory containing `main.slab`.
    pub root: PathBuf,
    pub sources: SourceMap,
    /// `modules[0]` is `main`.
    pub modules: Vec<Module>,
}

pub enum LoadError {
    /// The project could not be read at all (no `main.slab`, unreadable file).
    Io(String),
    /// Problems in the source: syntax errors, imports of files that don't exist.
    Invalid { sources: SourceMap, diagnostics: Vec<Diagnostic> },
}

/// Resolves what the user passed to `slab build`: a directory containing `main.slab`, or the
/// path of a `main.slab` itself.
pub fn find_root_file(path: &Path) -> Result<PathBuf, String> {
    let file = if path.is_dir() { path.join(ROOT_FILE) } else { path.to_path_buf() };
    if !file.is_file() {
        return Err(if path.is_dir() {
            format!("no `{ROOT_FILE}` in {}", path.display())
        } else {
            format!("cannot read {}", path.display())
        });
    }
    if file.file_name().is_none_or(|n| n != ROOT_FILE) {
        return Err(format!("the project entry point must be named `{ROOT_FILE}`, not {}", file.display()));
    }
    Ok(file)
}

/// Where the module `a::b` lives.
pub fn module_file(root: &Path, path: &[&str]) -> PathBuf {
    let mut p = root.to_path_buf();
    for seg in path {
        p.push(seg);
    }
    p.set_extension("slab");
    p
}

/// In-memory replacements for files on disk: the language server puts buffers that are still
/// being edited here so checks see unsaved text. Keys are the exact paths `load_with`/`load_tolerant`
/// visit (absolute, as built from the project root).
pub type Overlay = HashMap<PathBuf, String>;

pub fn load(root_file: &Path) -> Result<Project, LoadError> {
    load_inner(root_file, &Overlay::default(), false).map(|(p, _)| p)
}

pub fn load_with(root_file: &Path, overlay: &Overlay) -> Result<Project, LoadError> {
    load_inner(root_file, overlay, false).map(|(p, _)| p)
}

/// Like [`load_with`] but never bails entirely: what could be loaded, plus every diagnostic
/// from broken files and imports. The language server drives this; the strict [`load`] stays
/// the compiler's own path.
pub fn load_tolerant(root_file: &Path, overlay: &Overlay) -> Result<(Project, Vec<Diagnostic>), String> {
    load_inner(root_file, overlay, true).map_err(|e| match e {
        LoadError::Io(m) => m,
        LoadError::Invalid { .. } => "unreachable".to_owned(),
    })
}

fn load_inner(root_file: &Path, overlay: &Overlay, tolerant: bool) -> Result<(Project, Vec<Diagnostic>), LoadError> {
    let root = root_file.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut sources = SourceMap::default();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut modules: Vec<Module> = Vec::new();
    let mut loaded: HashMap<String, usize> = HashMap::new();
    // (module name, file path)
    let mut queue: VecDeque<(String, PathBuf)> = VecDeque::new();
    queue.push_back(("main".to_owned(), root_file.to_path_buf()));
    loaded.insert("main".to_owned(), 0);

    while let Some((name, path)) = queue.pop_front() {
        let text = match overlay.get(&path) {
            Some(t) => t.clone(),
            None => std::fs::read_to_string(&path).map_err(|e| {
                if path == root_file {
                    LoadError::Io(format!("cannot read {ROOT_FILE}: {e}"))
                } else {
                    LoadError::Io(format!("cannot read {}: {e}", path.display()))
                }
            })?,
        };
        let file = sources.add(SourceFile::new(path.display().to_string(), text));
        let program = match parse(&sources.files[file].text) {
            Ok(p) => p,
            Err(d) => {
                diagnostics.push(d.in_file(file));
                // keep module indices stable even though this one has no usable tree
                modules.push(Module { name, std: false, file, program: ast::Program { items: Vec::new() } });
                continue;
            }
        };
        for item in &program.items {
            let ItemKind::Import(imp) = &item.kind else { continue };
            let segs: Vec<&str> = imp.path.iter().map(|i| i.name.as_str()).collect();
            let key = segs.join("::");
            let span = imp.path[0].span.to(imp.path[imp.path.len() - 1].span);
            if key == "main" {
                diagnostics.push(Diagnostic::new("the root module `main` cannot be imported", span).in_file(file));
                continue;
            }
            if loaded.contains_key(&key) || crate::stdlib::is_import_path(&key) {
                continue;
            }
            let target = module_file(&root, &segs);
            if !target.is_file() {
                let rel = format!("{}.slab", segs.join("/"));
                diagnostics.push(Diagnostic::new(format!("cannot find module `{key}`"), span).hint(format!("create the file `{rel}` next to `{ROOT_FILE}`")).in_file(file));
                continue;
            }
            // module numbers follow discovery order: main first, then in order of first import
            loaded.insert(key.clone(), loaded.len());
            queue.push_back((key, target));
        }
        modules.push(Module { name, std: false, file, program });
    }
    // the standard library is part of every project
    for (m, program) in crate::stdlib::parsed() {
        let file = sources.add(SourceFile::new(crate::stdlib::file_path(m).display().to_string(), m.source.to_owned()));
        modules.push(Module { name: m.name.to_owned(), std: true, file, program: program.clone() });
    }

    if tolerant || diagnostics.is_empty() {
        Ok((Project { root, sources, modules }, diagnostics))
    } else {
        Err(LoadError::Invalid { sources, diagnostics })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A throwaway project directory.
    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str, files: &[(&str, &str)]) -> Dir {
            let dir = std::env::temp_dir().join(format!("slab-proj-{}-{tag}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            for (name, text) in files {
                let path = dir.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, text).unwrap();
            }
            Dir(dir)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn names(p: &Project) -> Vec<&str> {
        p.modules.iter().filter(|m| !m.std).map(|m| m.name.as_str()).collect()
    }

    #[test]
    fn loads_imports_transitively_once() {
        let d = Dir::new(
            "ok",
            &[
                ("main.slab", "import util; import maps::arena; fn main() {}"),
                ("util.slab", "import maps::arena; pub fn f() {}"),
                ("maps/arena.slab", "import util; pub fn g() {}"), // cycle: allowed
            ],
        );
        let p = load(&d.0.join("main.slab")).ok().expect("loads");
        assert_eq!(names(&p), ["main", "util", "maps::arena"]);
        assert_eq!(p.sources.files.len(), 3 + crate::stdlib::MODULES.len(), "three project files, then the standard library");
        assert!(p.sources.files[2].name.ends_with("arena.slab"));
    }

    #[test]
    fn overlay_replaces_the_file_on_disk() {
        let d = Dir::new("overlay", &[("main.slab", "import util; fn main() {}"), ("util.slab", "pub fn f() {}")]);
        let mut overlay = Overlay::default();
        overlay.insert(d.0.join("util.slab"), "pub fn g() {}".to_owned());
        let p = load_with(&d.0.join("main.slab"), &overlay).ok().expect("loads");
        // the overlay has the new function and no longer `f`
        assert!(p.sources.files[1].text.contains("fn g"), "{}", p.sources.files[1].text);
        assert!(!p.modules[1].program.items.iter().any(|i| matches!(i.kind, ItemKind::Fn(ref f) if f.name.name == "f")));
        assert_eq!(p.modules[1].program.items.len(), 1);
    }

    #[test]
    fn missing_module_is_reported_at_the_import() {
        let d = Dir::new("missing", &[("main.slab", "import nope::deep;\nfn main() {}")]);
        let Err(LoadError::Invalid { sources, diagnostics }) = load(&d.0.join("main.slab")) else { panic!() };
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("cannot find module `nope::deep`"));
        assert!(diagnostics[0].hint.as_deref().unwrap().contains("nope/deep.slab"));
        let r = sources.render(&diagnostics[0]);
        assert!(r.contains("main.slab:1:8"), "{r}");
    }

    #[test]
    fn syntax_errors_carry_their_file() {
        let d = Dir::new("syntax", &[("main.slab", "import broken; fn main() {}"), ("broken.slab", "fn f( {")]);
        let Err(LoadError::Invalid { sources, diagnostics }) = load(&d.0.join("main.slab")) else { panic!() };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].file, 1);
        assert!(sources.render(&diagnostics[0]).contains("broken.slab"));
    }

    #[test]
    fn several_broken_files_are_all_reported() {
        let d = Dir::new("several", &[("main.slab", "import a; import b; import c;"), ("a.slab", "fn ("), ("b.slab", "fn g() {}"), ("c.slab", "let")]);
        let Err(LoadError::Invalid { diagnostics, .. }) = load(&d.0.join("main.slab")) else { panic!() };
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn built_in_modules_are_not_files() {
        // `std` and `game` need no file, for the module, its submodules and its items alike
        let d = Dir::new("builtin", &[("main.slab", "import std::print; import game::*; import game::tag; import game::tag::{add}; import game::Entity as E; fn main() {}")]);
        let p = load(&d.0.join("main.slab")).ok().expect("loads");
        assert_eq!(names(&p), ["main"]);
    }

    #[test]
    fn root_cannot_be_imported() {
        let d = Dir::new("root", &[("main.slab", "import main;")]);
        let Err(LoadError::Invalid { diagnostics, .. }) = load(&d.0.join("main.slab")) else { panic!() };
        assert!(diagnostics[0].message.contains("cannot be imported"));
    }

    #[test]
    fn entry_point_lookup() {
        let d = Dir::new("entry", &[("main.slab", ""), ("other.slab", "")]);
        assert!(find_root_file(&d.0).unwrap().ends_with("main.slab"));
        assert!(find_root_file(&d.0.join("main.slab")).is_ok());
        assert!(find_root_file(&d.0.join("other.slab")).unwrap_err().contains("must be named `main.slab`"));
        assert!(find_root_file(&d.0.join("sub")).unwrap_err().contains("cannot read"));
        let empty = Dir::new("empty", &[("x/readme.txt", "")]);
        assert!(find_root_file(&empty.0.join("x")).unwrap_err().contains("no `main.slab`"));
    }
}
