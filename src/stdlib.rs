//! The standard library: `.slab` files baked into the compiler.
//!
//! `std` and `game` (with `game::tag` and `game::effect`) are ordinary modules; the project
//! loader adds them to every project. A function with no body marked `#[intrinsic("name")]` is
//! implemented by the compiler (`sema::builtins` lists the names it knows); everything else is
//! plain slab. See docs/lang/stdlib.md.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::syntax::{ast, parse};

pub struct StdModule {
    /// The module name as written in an import: `game::tag`.
    pub name: &'static str,
    /// Where it lives relative to the cache directory: `game/tag.slab`.
    pub path: &'static str,
    pub source: &'static str,
}

pub const MODULES: &[StdModule] = &[
    StdModule { name: "std", path: "std.slab", source: include_str!("stdlib/std.slab") },
    StdModule { name: "game", path: "game.slab", source: include_str!("stdlib/game.slab") },
    StdModule { name: "game::tag", path: "game/tag.slab", source: include_str!("stdlib/game/tag.slab") },
    StdModule { name: "game::effect", path: "game/effect.slab", source: include_str!("stdlib/game/effect.slab") },
];

/// The embedded modules, parsed once.
pub fn parsed() -> &'static [(&'static StdModule, ast::Program)] {
    static PARSED: OnceLock<Vec<(&'static StdModule, ast::Program)>> = OnceLock::new();
    PARSED.get_or_init(|| {
        MODULES
            .iter()
            .map(|m| (m, parse(m.source).unwrap_or_else(|e| panic!("the embedded module `{}` does not parse: {} (at {:?})", m.name, e.message, e.span))))
            .collect()
    })
}

/// Where the sources are written for editors to open (`write_cache`): a directory named after
/// the compiler version and the contents, so a new compiler never shows an old file.
pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    let mut hash: u64 = 0xcbf29ce484222325;
    for m in MODULES {
        for b in m.path.bytes().chain(m.source.bytes()) {
            hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
        }
    }
    base.join("slab").join(format!("std-{}-{:08x}", env!("CARGO_PKG_VERSION"), hash as u32))
}

/// Whether a path is a file of the standard library written by [`write_cache`], of this compiler
/// version or an older one (editors can still have those open).
pub fn is_cache_path(path: &std::path::Path) -> bool {
    let Some(root) = cache_dir().parent().map(std::path::Path::to_path_buf) else { return false };
    path.strip_prefix(&root).ok().and_then(|rel| rel.components().next()).is_some_and(|c| c.as_os_str().to_string_lossy().starts_with("std-"))
}

/// The path of an embedded module's source file (it only exists after [`write_cache`]).
pub fn file_path(m: &StdModule) -> PathBuf {
    cache_dir().join(m.path)
}

/// Writes the embedded sources (read-only) to [`cache_dir`] so that go-to-definition has a real
/// file to open. Safe to call again.
pub fn write_cache() -> std::io::Result<PathBuf> {
    let dir = cache_dir();
    for m in MODULES {
        let path = dir.join(m.path);
        if std::fs::read_to_string(&path).is_ok_and(|t| t == m.source) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, m.source)?;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms)?;
    }
    Ok(dir)
}

/// Whether `import <path>;` names an embedded module, or an item of one (`import std::print;`),
/// which the project loader must not look for on disk.
pub fn is_import_path(path: &str) -> bool {
    let is_module = |p: &str| MODULES.iter().any(|m| m.name == p);
    if is_module(path) {
        return true;
    }
    path.rsplit_once("::").is_some_and(|(parent, _)| is_module(parent))
}

/// Whether a module name belongs to the embedded library (`std`, `game`, `game::tag`, ...).
pub fn is_std_module(name: &str) -> bool {
    MODULES.iter().any(|m| m.name == name)
}

/// The public functions of the embedded modules as `(module, name)`, for editors.
pub fn function_names() -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for (m, program) in parsed() {
        for item in &program.items {
            if let ast::ItemKind::Fn(f) = &item.kind
                && item.public
            {
                out.push((m.name, f.name.name.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_modules_parse_and_every_function_is_an_intrinsic_or_has_a_body() {
        for (m, program) in parsed() {
            for item in &program.items {
                if let ast::ItemKind::Fn(f) = &item.kind {
                    let intrinsic = item.attrs.iter().any(|a| a.name.name == "intrinsic");
                    assert_eq!(f.has_body, !intrinsic, "{}::{}: a library function is either an intrinsic (no body) or written in slab (a body)", m.name, f.name.name);
                }
            }
        }
    }

    #[test]
    fn every_intrinsic_id_is_declared_exactly_once() {
        let mut declared: Vec<String> = Vec::new();
        for (_, program) in parsed() {
            for item in &program.items {
                for a in item.attrs.iter().filter(|a| a.name.name == "intrinsic") {
                    let Some(ast::ExprKind::Str(id)) = a.args.first().map(|e| &e.kind) else { panic!("an intrinsic without a name") };
                    declared.push(id.clone());
                }
            }
        }
        declared.sort();
        let mut known: Vec<String> = crate::sema::builtins::IDS.iter().map(|s| (*s).to_owned()).collect();
        known.sort();
        assert_eq!(declared, known, "the std sources and sema::builtins::IDS must list the same intrinsics");
    }

    #[test]
    fn every_method_intrinsic_id_is_declared_exactly_once() {
        let mut declared: Vec<String> = Vec::new();
        for (_, program) in parsed() {
            for item in &program.items {
                let ast::ItemKind::Impl(i) = &item.kind else { continue };
                for m in &i.methods {
                    let intrinsic = m.attrs.iter().any(|a| a.name.name == "intrinsic");
                    assert_eq!(m.decl.has_body, !intrinsic, "{}: a method of the library is either an intrinsic or has a body", m.decl.name.name);
                    for a in m.attrs.iter().filter(|a| a.name.name == "intrinsic") {
                        let Some(ast::ExprKind::Str(id)) = a.args.first().map(|e| &e.kind) else { panic!("an intrinsic without a name") };
                        declared.push(id.clone());
                    }
                }
            }
        }
        declared.sort();
        let mut known: Vec<String> = crate::sema::builtins::METHOD_IDS.iter().map(|s| (*s).to_owned()).collect();
        known.sort();
        assert_eq!(declared, known, "the std sources and sema::builtins::METHOD_IDS must list the same methods");
    }

    #[test]
    fn import_paths() {
        assert!(is_import_path("game"));
        assert!(is_import_path("game::tag"));
        assert!(is_import_path("std::print"));
        assert!(is_import_path("game::tag::add"));
        assert!(!is_import_path("util"));
        assert!(!is_import_path("maps::arena"));
    }
}
