//! The workspace: open documents with their overlay texts, and one analysis per project.
//! A "project" is whatever `main.slab`-rooted module tree a document belongs to; a document
//! under no `main.slab` gets a standalone project rooted at itself (its imports still load).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use slab::diag::{Diagnostic, SourceMap};
use slab::project as proj;
use slab::sema::{self, ModuleInput};
use slab::syntax::ast;

use crate::defs::DefIndex;
use crate::lines::{Enc, LineIndex};

pub struct Doc {
    pub uri: String,
    pub path: PathBuf,
    pub text: String,
    pub version: i32,
    pub lines: LineIndex,
}

pub struct ModuleInfo {
    /// Project-level module name: `main`, `maps::arena`.
    pub name: String,
    pub file_idx: usize,
    pub program: ast::Program,
    pub defs: DefIndex,
}

/// Everything the language server knows about one project, rebuilt on every change.
pub struct ProjectState {
    /// The project root directory; for a standalone file, the file itself.
    pub key: PathBuf,
    pub root: PathBuf,
    /// The file loading started from: `main.slab` for projects, the file itself otherwise.
    pub root_file: PathBuf,
    /// True when the file is under no `main.slab` and is used as the root module itself.
    pub standalone: bool,
    pub sources: SourceMap,
    pub modules: Vec<ModuleInfo>,
    pub lines: Vec<LineIndex>,
    /// `Some` even when errors exist ([`sema::check_modules_tolerant`]): access ids with
    /// `.get()`, expect `ExprKind::Error` nodes, and never rely on full name resolution.
    pub hir: Option<sema::hir::Hir>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// Set when the project could not be read at all (e.g. no main.slab on disk).
    pub io_error: Option<String>,
}

impl ProjectState {
    /// Which source file (if any) contains this path.
    pub fn file_of(&self, path: &Path) -> Option<usize> {
        self.sources.files.iter().position(|f| Path::new(f.name.as_str()) == path)
    }

    pub fn module_of_file(&self, file_idx: usize) -> Option<usize> {
        self.modules.iter().position(|m| m.file_idx == file_idx)
    }
}

/// A diagnostics broadcast after a change: a full replacement for one document.
#[derive(Debug)]
pub struct Publish {
    pub uri: String,
    pub version: Option<i32>,
    /// The project the diagnostics belong to (for line indexes / rendering).
    pub key: PathBuf,
    pub diagnostics: Vec<Diag>,
}

/// An LSP-shaped diagnostic, built with the owning file's line index.
#[derive(Debug)]
pub struct Diag {
    pub range: crate::lines::Range,
    /// 1 = error, 2 = warning (LSP `DiagnosticSeverity`).
    pub severity: u8,
    pub message: String,
}

/// Compiler severity as the LSP number.
pub fn lsp_severity(d: &Diagnostic) -> u8 {
    match d.severity {
        slab::diag::Severity::Error => 1,
        slab::diag::Severity::Warning => 2,
    }
}

/// The workspace: open documents with their overlay texts, and one analysis per project.
#[derive(Default)]
pub struct Workspace {
    docs: HashMap<String, Doc>,
    projects: HashMap<PathBuf, ProjectState>,
    /// The negotiated position encoding; set once after `initialize`.
    pub enc: Enc,
}

impl Workspace {
    pub fn open(&mut self, uri: &str, text: &str, version: i32) -> Vec<Publish> {
        self.sync(uri, Some((text.to_owned(), version)))
    }

    pub fn change(&mut self, uri: &str, text: &str, version: i32) -> Vec<Publish> {
        self.sync(uri, Some((text.to_owned(), version)))
    }

    pub fn close(&mut self, uri: &str) -> Vec<Publish> {
        self.sync(uri, None)
    }

    /// The state of the project a document belongs to.
    pub fn project(&self, key: &Path) -> Option<&ProjectState> {
        self.projects.get(key)
    }

    /// The HIR of the project with one byte range of one document replaced: completion asks
    /// for this because `p.` or `p.x` half-typed stops the parser before they'd check.
    pub fn hir_with_replacement(&self, uri: &str, start: usize, end: usize, replacement: &str) -> Option<sema::hir::Hir> {
        let (_, doc, _) = self.locate(uri)?;
        let (_key, root_file, _standalone) = root_for(&doc.path);
        let mut overlay = proj::Overlay::default();
        for d in self.docs.values() {
            let mut text = d.text.clone();
            if d.uri == uri {
                let s = start.min(text.len());
                let e = end.min(text.len()).max(s);
                text.replace_range(s..e, replacement);
            }
            overlay.insert(d.path.clone(), text);
        }
        let (project, _) = proj::load_tolerant(&root_file, &overlay).ok()?;
        let inputs: Vec<ModuleInput> =
            project.modules.iter().map(|m| ModuleInput { name: &m.name, file: m.file, program: &m.program, std: m.std }).collect();
        sema::check_modules_tolerant(&inputs).hir
    }

    /// The document, its project and (when the file was parsed) its module.
    pub fn locate(&self, uri: &str) -> Option<(&ProjectState, &Doc, Option<usize>)> {
        let doc = self.docs.get(uri)?;
        let (key, _, _) = root_for(&doc.path);
        let state = self.projects.get(&key)?;
        let module = state.file_of(&doc.path).and_then(|fi| state.module_of_file(fi));
        Some((state, doc, module))
    }

    fn sync(&mut self, uri: &str, update: Option<(String, i32)>) -> Vec<Publish> {
        let Some(path) = uri_to_path(uri) else { return Vec::new() };
        let (key, root_file, standalone) = root_for(&path);
        match update {
            Some((text, version)) => {
                let lines = LineIndex::new(&text);
                self.docs.insert(uri.to_owned(), Doc { uri: uri.to_owned(), path, text, version, lines });
                let built = build_project(key.clone(), &root_file, standalone, &self.docs);
                let publishes = publishes_for(&built, &self.docs, self.enc);
                self.projects.insert(key, built);
                publishes
            }
            None => {
                let was_standalone = standalone;
                let Some(_) = self.docs.remove(uri) else { return Vec::new() };
                if was_standalone {
                    self.projects.remove(&key);
                } else {
                    // The file is back to its on-disk text; rebuild so the others stay fresh.
                    let built = build_project(key.clone(), &root_file, standalone, &self.docs);
                    let publishes = publishes_for(&built, &self.docs, self.enc);
                    self.projects.insert(key.clone(), built);
                    return publishes;
                }
                vec![Publish { uri: uri.to_owned(), version: None, key, diagnostics: Vec::new() }]
            }
        }
    }
}

/// (project key, root file, standalone?)
pub fn root_for(path: &Path) -> (PathBuf, PathBuf, bool) {
    match find_root(path) {
        Some(root) => (root.clone(), root.join(proj::ROOT_FILE), false),
        None => (path.to_path_buf(), path.to_path_buf(), true),
    }
}

/// Nearest directory above `path` that contains a `main.slab`.
fn find_root(path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent()?.to_path_buf();
    loop {
        if dir.join(proj::ROOT_FILE).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The analysis for a root: load files (with editor overlays), parse, check — tolerantly,
/// because an editor sits between states most of the time.
fn build_project(key: PathBuf, root_file: &Path, standalone: bool, docs: &HashMap<String, Doc>) -> ProjectState {
    let root_file = root_file.to_path_buf();
    let mut overlay = proj::Overlay::default();
    for d in docs.values() {
        overlay.insert(d.path.clone(), d.text.clone());
    }
    match proj::load_tolerant(&root_file, &overlay) {
        Ok((project, load_diags)) => {
            let inputs: Vec<ModuleInput> =
                project.modules.iter().map(|m| ModuleInput { name: &m.name, file: m.file, program: &m.program, std: m.std }).collect();
            let t = sema::check_modules_tolerant(&inputs);
            let modules = project
                .modules
                .into_iter()
                .map(|m| ModuleInfo {
                    name: m.name.clone(),
                    file_idx: m.file,
                    defs: {
                        let mut defs = DefIndex::build(&m.program);
                        defs.attach_docs(&project.sources.files[m.file].text);
                        defs
                    },
                    program: m.program,
                })
                .collect();
            let mut errors = load_diags;
            errors.extend(t.errors);
            errors.sort_by_key(|d| (d.file, d.span.start));
            let mut warnings = t.warnings;
            warnings.sort_by_key(|d| (d.file, d.span.start));
            let sources = project.sources;
            let root = project.root;
            let lines = sources.files.iter().map(|f| LineIndex::new(&f.text)).collect();
            ProjectState { key, root, root_file: root_file.clone(), standalone, sources, modules, lines, hir: t.hir, errors, warnings, io_error: None }
        }
        Err(message) => ProjectState {
            key,
            root: root_file.parent().map(Path::to_path_buf).unwrap_or_default(),
            root_file,
            standalone,
            sources: SourceMap::default(),
            modules: Vec::new(),
            lines: Vec::new(),
            hir: None,
            errors: Vec::new(),
            warnings: Vec::new(),
            io_error: Some(message),
        },
    }
}

/// Diagnostics of a project grouped into one publish per open document, already converted
/// to LSP-shaped values here where the line indexes are at hand.
fn publishes_for(state: &ProjectState, docs: &HashMap<String, Doc>, enc: Enc) -> Vec<Publish> {
    let mut out = Vec::new();
    for doc in docs.values() {
        let diagnostics = match state.file_of(&doc.path) {
            Some(file_idx) => {
                let index = state.lines.get(file_idx);
                let text = &state.sources.files[file_idx].text;
                state.errors
                    .iter()
                    .chain(state.warnings.iter())
                    .filter(|d| d.file == file_idx)
                    .map(|d| to_diag(d, index, text, enc))
                    .collect()
            }
            // An unreadable project: report the reason on its root document (no line index).
            None if doc.path == state.root_file => {
                vec![Diag { range: zero_range(), severity: 1, message: state.io_error.clone().unwrap_or_default() }]
            }
            None => continue,
        };
        out.push(Publish { uri: doc.uri.clone(), version: Some(doc.version), key: state.key.clone(), diagnostics });
    }
    out
}

/// A compiler diagnostic as the LSP shape, with any hint appended.
pub fn to_diag(d: &Diagnostic, index: Option<&LineIndex>, text: &str, enc: Enc) -> Diag {
    let mut message = d.message.clone();
    if let Some(hint) = &d.hint {
        message.push_str("\nhint: ");
        message.push_str(hint);
    }
    Diag {
        range: index.map_or_else(zero_range, |l| l.range(text, d.span, enc)),
        severity: lsp_severity(d),
        message,
    }
}

fn zero_range() -> crate::lines::Range {
    let p = crate::lines::Position { line: 0, character: 0 };
    crate::lines::Range { start: p, end: p }
}

// ------------------------------------------------------------------ URIs

/// `file:///a/b%20c.slab` -> `/a/b c.slab` (percent-decoded). Other schemes: None.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(rest)))
}

fn percent_decode(s: &str) -> String {
    fn hex(b: u8) -> u8 {
        (b as char).to_digit(16).map_or(0, |d| d as u8)
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            out.push(hex(bytes[i + 1]) << 4 | hex(bytes[i + 2]));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reverse of [`uri_to_path`], conservative percent-encoding.
pub fn path_to_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    let text = path.to_string_lossy();
    for b in text.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}