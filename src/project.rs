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
    /// `"main"` for the root, otherwise the import path: `"game::arena"`.
    pub name: String,
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

pub fn load(root_file: &Path) -> Result<Project, LoadError> {
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
        let text = std::fs::read_to_string(&path).map_err(|e| LoadError::Io(format!("cannot read {}: {e}", path.display())))?;
        let file = sources.add(SourceFile::new(path.display().to_string(), text));
        let program = match parse(&sources.files[file].text) {
            Ok(p) => p,
            Err(d) => {
                diagnostics.push(d.in_file(file));
                // keep module indices stable even though this one has no usable tree
                modules.push(Module { name, file, program: ast::Program { items: Vec::new() } });
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
            if loaded.contains_key(&key) {
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
        modules.push(Module { name, file, program });
    }

    if diagnostics.is_empty() {
        Ok(Project { root, sources, modules })
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
        p.modules.iter().map(|m| m.name.as_str()).collect()
    }

    #[test]
    fn loads_imports_transitively_once() {
        let d = Dir::new(
            "ok",
            &[
                ("main.slab", "import util; import game::arena; fn main() {}"),
                ("util.slab", "import game::arena; pub fn f() {}"),
                ("game/arena.slab", "import util; pub fn g() {}"), // cycle: allowed
            ],
        );
        let p = load(&d.0.join("main.slab")).ok().expect("loads");
        assert_eq!(names(&p), ["main", "util", "game::arena"]);
        assert_eq!(p.sources.files.len(), 3);
        assert!(p.sources.files[2].name.ends_with("arena.slab"));
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
