//! Name resolution, type checking and semantic analysis: syntax trees -> `hir::Hir`.
//! Rules are documented in docs/lang/semantics.md.

use std::collections::{HashMap, HashSet};

use super::builtins::{self, BParam, Builtin};
use super::hir::*;
use super::selector;
use super::ty::{StructId, Ty};
use crate::datapack::{HolderName, ResourceLocation, Word};
use crate::diag::{Diagnostic, Span};
use crate::syntax::ast::{self, AssignOp, BinOp, UnOp};

pub struct Checked {
    pub hir: Hir,
    pub warnings: Vec<Diagnostic>,
}

pub struct Report {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// One parsed source file of the project.
pub struct ModuleInput<'a> {
    /// `"main"` for the root, `"maps::arena"` for `maps/arena.slab`.
    pub name: &'a str,
    /// Index into the project's `SourceMap`, stamped on every diagnostic from this module.
    pub file: usize,
    pub program: &'a ast::Program,
}

/// Checks a single-file program (module `main`).
pub fn check(program: &ast::Program) -> Result<Checked, Report> {
    check_modules(&[ModuleInput { name: "main", file: 0, program }])
}

/// Checks a whole project. Module 0 must be the root (`main.slab`). All errors found are
/// reported together, ordered by file and position.
pub fn check_modules(modules: &[ModuleInput]) -> Result<Checked, Report> {
    let mut c = Checker::new(modules);
    c.run();
    c.finish()
}

/// The result of [`check_modules_tolerant`]: everything the checker found, including whatever
/// HIR could be built even though the program has errors. The language server drives this: an
/// editor is often *between* states, and hover and completion should still work. Ids may be
/// missing from the HIR (`.get(id.0)`), and expressions can be `ExprKind::Error`; the regular
/// pipeline is unaffected.
pub struct Tolerant {
    pub hir: Option<Hir>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// [`check_modules`] without the all-or-nothing contract.
pub fn check_modules_tolerant(modules: &[ModuleInput]) -> Tolerant {
    let mut c = Checker::new(modules);
    c.run();
    let key = |d: &Diagnostic| (d.file, d.span.start);
    c.errors.sort_by_key(key);
    c.warnings.sort_by_key(key);
    Tolerant { hir: Some(c.build_hir()), errors: c.errors, warnings: c.warnings }
}

// ------------------------------------------------------------------- symbols

#[derive(Clone, Copy)]
enum ValueSym {
    Fn(FuncId),
    Global(GlobalId),
    Const(usize),
    Objective(ObjectiveId),
    /// A built-in function: an index into `builtins::BUILTINS`.
    Builtin(usize),
}

/// A module named in a path: a project file, or a built-in module (`std`, `game`, ...).
enum ModRef {
    User(usize),
    Builtin(String),
}

/// What a module can see: its own items, module aliases and imported items.
struct ModScope<'a> {
    name: String,
    file: usize,
    program: &'a ast::Program,
    /// own items: symbol and whether it is `pub`
    values: HashMap<String, (ValueSym, bool)>,
    types: HashMap<String, (StructId, bool)>,
    /// `import a::b;` -> alias `b` -> module index
    modules: HashMap<String, usize>,
    /// `import a::{x};` -> `x` usable unqualified
    imported_values: HashMap<String, ValueSym>,
    imported_types: HashMap<String, StructId>,
    /// `import game::tag;` -> alias `tag` -> built-in module path `game::tag`
    builtin_modules: HashMap<String, String>,
    /// `import game::Entity;` -> `Entity` usable unqualified
    imported_builtin_types: HashMap<String, Ty>,
}

struct StructInfo<'a> {
    name: String,
    module: usize,
    decl: &'a ast::StructDecl,
    fields: Vec<(String, Ty)>,
}

struct FnSig<'a> {
    decl: &'a ast::FnDecl,
    attrs: &'a [ast::Attr],
    module: usize,
    params: Vec<Ty>,
    ret: Ty,
    entry: Option<EntryKind>,
    span: Span,
}

#[derive(Clone)]
enum ConstState {
    Pending,
    Evaluating,
    Done(ConstVal),
    Failed,
}

struct ConstInfo<'a> {
    decl: &'a ast::VarDecl,
    module: usize,
    ty: Ty,
    state: ConstState,
}

struct FnCtx {
    locals: Vec<LocalInfo>,
    used: Vec<bool>,
    scopes: Vec<HashMap<String, LocalId>>,
    ret: Ty,
    loop_depth: usize,
    ctx_depth: usize,
}

struct Checker<'a> {
    mods: Vec<ModScope<'a>>,
    by_name: HashMap<String, usize>,
    /// The module whose code is being checked (names resolve in its scope).
    cur: usize,
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
    structs: Vec<StructInfo<'a>>,
    struct_names: Vec<String>,
    consts: Vec<ConstInfo<'a>>,
    globals: Vec<GlobalDef>,
    global_decls: Vec<(&'a ast::VarDecl, usize)>,
    objectives: Vec<ObjectiveDef>,
    fns: Vec<FnSig<'a>>,
    hir_fns: Vec<FnDef>,
    f: Option<FnCtx>,
}

fn fits_i32(n: i64) -> bool {
    i32::try_from(n).is_ok()
}

/// Floored division with the scoreboard's semantics; None on division by zero.
fn floor_div(a: i32, b: i32) -> Option<i32> {
    if b == 0 {
        return None;
    }
    let (a, b) = (a as i64, b as i64);
    let q = a / b;
    Some(if a % b != 0 && ((a < 0) != (b < 0)) { q - 1 } else { q } as i32)
}

impl<'a> Checker<'a> {
    fn new(modules: &[ModuleInput<'a>]) -> Self {
        let mods: Vec<ModScope<'a>> = modules
            .iter()
            .map(|m| ModScope {
                name: m.name.to_owned(),
                file: m.file,
                program: m.program,
                values: HashMap::new(),
                types: HashMap::new(),
                modules: HashMap::new(),
                imported_values: HashMap::new(),
                imported_types: HashMap::new(),
                builtin_modules: HashMap::new(),
                imported_builtin_types: HashMap::new(),
            })
            .collect();
        let by_name = mods.iter().enumerate().map(|(i, m)| (m.name.clone(), i)).collect();
        Checker {
            mods,
            by_name,
            cur: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
            structs: Vec::new(),
            struct_names: Vec::new(),
            consts: Vec::new(),
            globals: Vec::new(),
            global_decls: Vec::new(),
            objectives: Vec::new(),
            fns: Vec::new(),
            hir_fns: Vec::new(),
            f: None,
        }
    }

    fn file(&self) -> usize {
        self.mods[self.cur].file
    }

    fn error(&mut self, span: Span, msg: impl Into<String>) {
        let d = Diagnostic::new(msg, span).in_file(self.file());
        self.errors.push(d);
    }

    fn error_hint(&mut self, span: Span, msg: impl Into<String>, hint: impl Into<String>) {
        let d = Diagnostic::new(msg, span).hint(hint).in_file(self.file());
        self.errors.push(d);
    }

    fn warn(&mut self, span: Span, msg: impl Into<String>) {
        let d = Diagnostic::warning(msg, span).in_file(self.file());
        self.warnings.push(d);
    }

    fn tn(&self, t: &Ty) -> String {
        t.display(&self.struct_names).to_string()
    }

    fn mismatch(&mut self, span: Span, expected: &Ty, found: &Ty) {
        if expected.compatible(found) {
            return;
        }
        let msg = format!("mismatched types: expected `{}`, found `{}`", self.tn(expected), self.tn(found));
        self.error(span, msg);
    }

    fn finish(mut self) -> Result<Checked, Report> {
        let key = |d: &Diagnostic| (d.file, d.span.start);
        self.errors.sort_by_key(key);
        self.warnings.sort_by_key(key);
        if !self.errors.is_empty() {
            return Err(Report { errors: self.errors, warnings: self.warnings });
        }
        Ok(Checked { hir: self.build_hir(), warnings: self.warnings })
    }

    fn build_hir(&mut self) -> Hir {
        Hir {
            modules: self.mods.iter().map(|m| m.name.clone()).collect(),
            files: self.mods.iter().map(|m| m.file).collect(),
            structs: self.structs.iter().map(|s| StructDef { name: s.name.clone(), module: s.module, fields: s.fields.clone() }).collect(),
            globals: self.globals.clone(),
            objectives: self.objectives.clone(),
            fns: self.hir_fns.clone(),
        }
    }

    // ------------------------------------------------------------ name lookup

    fn lookup_value(&self, name: &str) -> Option<ValueSym> {
        let m = &self.mods[self.cur];
        m.values.get(name).map(|v| v.0).or_else(|| m.imported_values.get(name).copied())
    }

    fn lookup_struct(&self, name: &str) -> Option<StructId> {
        let m = &self.mods[self.cur];
        m.types.get(name).map(|v| v.0).or_else(|| m.imported_types.get(name).copied())
    }

    /// The module a path names: an imported project module, an imported alias of (or a full path
    /// to) a built-in one. Reports a missing module.
    fn module_ref(&mut self, alias: &ast::Ident) -> Option<ModRef> {
        let name = alias.name.as_str();
        let (first, rest) = match name.split_once("::") {
            Some((f, r)) => (f, Some(r)),
            None => (name, None),
        };
        let m = &self.mods[self.cur];
        if let Some(&u) = m.modules.get(name) {
            return Some(ModRef::User(u));
        }
        if let Some(base) = m.builtin_modules.get(first) {
            let full = match rest {
                Some(r) => format!("{base}::{r}"),
                None => base.clone(),
            };
            if builtins::is_module(&full) {
                return Some(ModRef::Builtin(full));
            }
        }
        if builtins::is_module(name) {
            return Some(ModRef::Builtin(name.to_owned()));
        }
        self.error_hint(alias.span, format!("cannot find module `{name}`"), format!("import it first: `import {name};`"));
        None
    }

    fn module_alias(&mut self, alias: &ast::Ident) -> Option<usize> {
        match self.module_ref(alias)? {
            ModRef::User(m) => Some(m),
            ModRef::Builtin(path) => {
                self.error(alias.span, format!("`{path}` is a built-in module here, not a project module"));
                None
            }
        }
    }

    /// A hint for a bare name that is a builtin which has not been imported.
    fn builtin_hint(&self, name: &str) -> Option<String> {
        let b = builtins::BUILTINS.iter().find(|b| b.name == name)?;
        Some(format!("call it as `{}(...)`, or import it: `import {}::{};`", b.path(), b.module, b.name))
    }

    /// `base` names a built-in module and is not shadowed by a variable or item (`game.say(..)`
    /// is a mistake: module paths use `::`).
    fn module_dot_misuse(&mut self, base: &ast::Expr) -> Option<String> {
        let ast::ExprKind::Ident(m) = &base.kind else { return None };
        if self.lookup_value_or_local(m).is_some() {
            return None;
        }
        let known = self.mods[self.cur].builtin_modules.get(m.as_str()).cloned().or_else(|| builtins::is_module(m).then(|| m.clone()));
        known.map(|_| m.clone())
    }

    fn private_item(&mut self, name: &ast::Ident, module: usize) {
        let msg = format!("`{}` is private to module `{}`", name.name, self.mods[module].name);
        self.error_hint(name.span, msg, "mark it `pub` to use it from other modules");
    }

    /// `module::name` as a value.
    fn qualified_value(&mut self, module: &ast::Ident, name: &ast::Ident) -> Option<ValueSym> {
        let m = match self.module_ref(module)? {
            ModRef::User(m) => m,
            ModRef::Builtin(path) => {
                return match builtins::member(&path, &name.name) {
                    Some(i) => Some(ValueSym::Builtin(i)),
                    None => {
                        self.error(name.span, format!("module `{path}` has no function `{}`", name.name));
                        None
                    }
                };
            }
        };
        match self.mods[m].values.get(&name.name).copied() {
            Some((_, false)) if m != self.cur => {
                self.private_item(name, m);
                None
            }
            Some((sym, _)) => Some(sym),
            None => {
                let msg = format!("module `{}` has no item `{}`", self.mods[m].name, name.name);
                self.error(name.span, msg);
                None
            }
        }
    }

    /// The built-in module path `m` names, if it is one and not a project module imported
    /// under that name. Reports nothing.
    fn builtin_path(&self, m: &ast::Ident) -> Option<String> {
        let name = m.name.as_str();
        if self.mods[self.cur].modules.contains_key(name) {
            return None;
        }
        let (first, rest) = name.split_once("::").map_or((name, None), |(f, r)| (f, Some(r)));
        let full = match self.mods[self.cur].builtin_modules.get(first) {
            Some(base) => rest.map_or_else(|| base.clone(), |r| format!("{base}::{r}")),
            None => name.to_owned(),
        };
        builtins::is_module(&full).then_some(full)
    }

    /// `module::Name` as a struct type.
    fn qualified_struct(&mut self, module: &ast::Ident, name: &ast::Ident) -> Option<StructId> {
        let m = self.module_alias(module)?;
        match self.mods[m].types.get(&name.name).copied() {
            Some((_, false)) if m != self.cur => {
                self.private_item(name, m);
                None
            }
            Some((id, _)) => Some(id),
            None => {
                let msg = format!("module `{}` has no struct `{}`", self.mods[m].name, name.name);
                self.error(name.span, msg);
                None
            }
        }
    }

    fn find_struct(&mut self, module: Option<&ast::Ident>, name: &ast::Ident) -> Option<StructId> {
        match module {
            Some(m) => {
                if let Some(path) = self.builtin_path(m) {
                    self.error(name.span, format!("`{path}::{}` cannot be built with a struct literal", name.name));
                    return None;
                }
                self.qualified_struct(m, name)
            }
            None => match self.lookup_struct(&name.name) {
                Some(id) => Some(id),
                None => {
                    self.error(name.span, format!("unknown struct `{}`", name.name));
                    None
                }
            },
        }
    }

    // ------------------------------------------------------------ item passes

    fn run(&mut self) {
        for mi in 0..self.mods.len() {
            self.cur = mi;
            self.register_items(mi);
        }
        self.resolve_imports();
        self.resolve_struct_fields();
        self.resolve_signatures();
        self.eval_globals();
        for i in 0..self.fns.len() {
            self.check_fn(i);
        }
    }

    /// Records the names of every item of module `mi` (types of fields/params come later, once
    /// imports are known).
    fn register_items(&mut self, mi: usize) {
        let program = self.mods[mi].program;
        for item in &program.items {
            self.reject_attrs_on_non_fn(item);
            match &item.kind {
                ast::ItemKind::Struct(s) => {
                    if self.mods[mi].types.contains_key(&s.name.name) {
                        self.error(s.name.span, format!("struct `{}` is defined twice", s.name.name));
                        continue;
                    }
                    let id = StructId(self.structs.len());
                    self.mods[mi].types.insert(s.name.name.clone(), (id, item.public));
                    self.struct_names.push(s.name.name.clone());
                    self.structs.push(StructInfo { name: s.name.name.clone(), module: mi, decl: s, fields: Vec::new() });
                }
                ast::ItemKind::Fn(f) => {
                    let id = FuncId(self.fns.len());
                    // Registered even when the name is a duplicate so bodies still get checked.
                    self.declare_value(&f.name, ValueSym::Fn(id), item.public);
                    self.fns.push(FnSig { decl: f, attrs: &item.attrs, module: mi, params: Vec::new(), ret: Ty::Unit, entry: None, span: item.span });
                }
                ast::ItemKind::Const(d) => {
                    let idx = self.consts.len();
                    if self.declare_value(&d.name, ValueSym::Const(idx), item.public) {
                        self.consts.push(ConstInfo { decl: d, module: mi, ty: Ty::Error, state: ConstState::Pending });
                    }
                }
                ast::ItemKind::Global(d) => {
                    let id = GlobalId(self.globals.len());
                    if self.declare_value(&d.name, ValueSym::Global(id), item.public) {
                        self.globals.push(GlobalDef { name: d.name.name.clone(), module: mi, ty: Ty::Error, init: ConstVal::Int(0) });
                        self.global_decls.push((d, mi));
                    }
                }
                ast::ItemKind::Objective { name, criteria } => {
                    let id = ObjectiveId(self.objectives.len());
                    if self.declare_value(name, ValueSym::Objective(id), item.public) {
                        let crit = match &criteria.kind {
                            ast::ExprKind::Str(s) => s.clone(),
                            _ => {
                                self.error(criteria.span, "an objective's criteria must be a string literal such as \"dummy\"");
                                String::new()
                            }
                        };
                        if Word::new(&name.name).is_err() {
                            self.error(name.span, "invalid objective name");
                        }
                        self.objectives.push(ObjectiveDef { name: name.name.clone(), module: mi, criteria: crit });
                    }
                }
                ast::ItemKind::Import(_) => {}
            }
        }
    }

    fn declare_value(&mut self, name: &ast::Ident, sym: ValueSym, public: bool) -> bool {
        if self.mods[self.cur].values.contains_key(&name.name) {
            self.error(name.span, format!("`{}` is defined more than once", name.name));
            return false;
        }
        self.mods[self.cur].values.insert(name.name.clone(), (sym, public));
        true
    }

    fn name_taken(&self, mi: usize, name: &str) -> bool {
        let m = &self.mods[mi];
        m.values.contains_key(name)
            || m.types.contains_key(name)
            || m.modules.contains_key(name)
            || m.imported_values.contains_key(name)
            || m.imported_types.contains_key(name)
            || m.builtin_modules.contains_key(name)
            || m.imported_builtin_types.contains_key(name)
    }

    fn resolve_imports(&mut self) {
        // glob imports go last, so an explicit import or definition wins over them
        for glob_pass in [false, true] {
            self.resolve_imports_pass(glob_pass);
        }
    }

    fn resolve_imports_pass(&mut self, glob_pass: bool) {
        for mi in 0..self.mods.len() {
            self.cur = mi;
            let program = self.mods[mi].program;
            for item in &program.items {
                let ast::ItemKind::Import(imp) = &item.kind else { continue };
                if imp.glob != glob_pass {
                    continue;
                }
                let key = imp.path.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join("::");
                let path_span = imp.path[0].span.to(imp.path[imp.path.len() - 1].span);
                if builtins::is_module(&key) {
                    self.import_builtin(mi, imp, &key);
                    continue;
                }
                if builtins::is_import_path(&key) {
                    // `import std::print;` brings one item in, under its own name or an alias
                    let (parent, last) = key.rsplit_once("::").expect("an item path");
                    let (name, kind) = builtins::module_items(parent).into_iter().find(|(n, _)| *n == last).expect("an item");
                    let bound = imp.alias.as_ref().unwrap_or(&imp.path[imp.path.len() - 1]);
                    if imp.glob || imp.items.is_some() {
                        self.error(path_span, format!("`{key}` is not a module, so it has no items to import"));
                    } else if self.name_taken(mi, &bound.name) {
                        self.error(bound.span, format!("the name `{}` is already used in this module", bound.name));
                    } else {
                        self.bind_builtin_as(mi, parent, name, kind, &bound.name);
                    }
                    continue;
                }
                let Some(&target) = self.by_name.get(&key) else {
                    self.error_hint(path_span, format!("cannot find module `{key}`"), format!("it would be the file `{}.slab` in the project", imp.path.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join("/")));
                    continue;
                };
                if target == mi {
                    self.error(path_span, "a module cannot import itself");
                    continue;
                }
                if imp.glob {
                    // `import util::*;` brings every public item in, unless the name is already taken
                    let values: Vec<(String, ValueSym)> = self.mods[target].values.iter().filter(|(_, (_, public))| *public).map(|(n, (s, _))| (n.clone(), *s)).collect();
                    let types: Vec<(String, StructId)> = self.mods[target].types.iter().filter(|(_, (_, public))| *public).map(|(n, (s, _))| (n.clone(), *s)).collect();
                    for (n, sym) in values {
                        if !self.name_taken(mi, &n) {
                            self.mods[mi].imported_values.insert(n, sym);
                        }
                    }
                    for (n, id) in types {
                        if !self.name_taken(mi, &n) {
                            self.mods[mi].imported_types.insert(n, id);
                        }
                    }
                    continue;
                }
                match &imp.items {
                    None => {
                        let alias = imp.alias.as_ref().unwrap_or(&imp.path[imp.path.len() - 1]);
                        if self.name_taken(mi, &alias.name) {
                            self.error(alias.span, format!("the name `{}` is already used in this module", alias.name));
                        } else {
                            self.mods[mi].modules.insert(alias.name.clone(), target);
                        }
                    }
                    Some(names) => {
                        for n in names {
                            let value = self.mods[target].values.get(&n.name).copied();
                            let ty = self.mods[target].types.get(&n.name).copied();
                            if value.is_none() && ty.is_none() {
                                self.error(n.span, format!("module `{key}` has no item `{}`", n.name));
                                continue;
                            }
                            if self.name_taken(mi, &n.name) {
                                self.error(n.span, format!("the name `{}` is already used in this module", n.name));
                                continue;
                            }
                            let mut any = false;
                            if let Some((sym, public)) = value
                                && public {
                                    self.mods[mi].imported_values.insert(n.name.clone(), sym);
                                    any = true;
                                }
                            if let Some((id, public)) = ty
                                && public {
                                    self.mods[mi].imported_types.insert(n.name.clone(), id);
                                    any = true;
                                }
                            if !any {
                                self.private_item(n, target);
                            }
                        }
                    }
                }
            }
        }
    }

    /// `import std::*;`, `import game::{summon, Entity};`, `import game::tag;`
    fn import_builtin(&mut self, mi: usize, imp: &'a ast::ImportDecl, key: &str) {
        let items = builtins::module_items(key);
        if imp.glob {
            for (name, kind) in items {
                if self.name_taken(mi, name) {
                    continue;
                }
                self.bind_builtin(mi, key, name, kind);
            }
            return;
        }
        match &imp.items {
            None => {
                let alias = imp.alias.as_ref().unwrap_or(&imp.path[imp.path.len() - 1]);
                if self.name_taken(mi, &alias.name) {
                    self.error(alias.span, format!("the name `{}` is already used in this module", alias.name));
                } else {
                    self.mods[mi].builtin_modules.insert(alias.name.clone(), key.to_owned());
                }
            }
            Some(names) => {
                for n in names {
                    let Some((name, kind)) = items.iter().find(|(i, _)| *i == n.name).copied() else {
                        self.error(n.span, format!("module `{key}` has no item `{}`", n.name));
                        continue;
                    };
                    if self.name_taken(mi, &n.name) {
                        self.error(n.span, format!("the name `{}` is already used in this module", n.name));
                        continue;
                    }
                    self.bind_builtin(mi, key, name, kind);
                }
            }
        }
    }

    fn bind_builtin(&mut self, mi: usize, module: &str, name: &'static str, kind: builtins::ItemKind) {
        self.bind_builtin_as(mi, module, name, kind, name);
    }

    /// Makes the built-in item `module::name` usable as `bound`.
    fn bind_builtin_as(&mut self, mi: usize, module: &str, name: &str, kind: builtins::ItemKind, bound: &str) {
        match kind {
            builtins::ItemKind::Fn(i) => {
                self.mods[mi].imported_values.insert(bound.to_owned(), ValueSym::Builtin(i));
            }
            builtins::ItemKind::Type => {
                if let Some(ty) = builtins::builtin_type(module, name) {
                    self.mods[mi].imported_builtin_types.insert(bound.to_owned(), ty);
                }
            }
            builtins::ItemKind::Module => {
                self.mods[mi].builtin_modules.insert(bound.to_owned(), format!("{module}::{name}"));
            }
        }
    }

    fn resolve_struct_fields(&mut self) {
        for i in 0..self.structs.len() {
            self.cur = self.structs[i].module;
            let decl = self.structs[i].decl;
            let mut seen = HashSet::new();
            let mut fields = Vec::new();
            for f in &decl.fields {
                if !seen.insert(f.name.name.clone()) {
                    self.error(f.name.span, format!("field `{}` is declared twice in `{}`", f.name.name, decl.name.name));
                    continue;
                }
                let ty = self.resolve_ty(&f.ty);
                if !ty.is_storable() {
                    self.error(f.ty.span, format!("a field cannot have type `{}`", self.tn(&ty)));
                }
                fields.push((f.name.name.clone(), ty));
            }
            self.structs[i].fields = fields;
        }
        // a struct cannot contain itself, directly or through other structs
        for i in 0..self.structs.len() {
            let id = StructId(i);
            // report each cycle once, on its first-declared struct
            let in_cycle = self.struct_reaches(id, id, &mut HashSet::new());
            let earlier_member = (0..i).any(|j| {
                let y = StructId(j);
                self.struct_reaches(id, y, &mut HashSet::new()) && self.struct_reaches(y, id, &mut HashSet::new())
            });
            if in_cycle && !earlier_member {
                self.cur = self.structs[i].module;
                let decl = self.structs[i].decl;
                self.error_hint(decl.name.span, format!("struct `{}` contains itself", decl.name.name), "a struct cannot be part of its own fields (use a `list` for repeated nesting)");
            }
        }
    }

    fn struct_reaches(&self, from: StructId, target: StructId, seen: &mut HashSet<StructId>) -> bool {
        for (_, ty) in &self.structs[from.0].fields {
            if let Ty::Struct(next) = ty {
                if *next == target {
                    return true;
                }
                if seen.insert(*next) && self.struct_reaches(*next, target, seen) {
                    return true;
                }
            }
        }
        false
    }

    fn reject_attrs_on_non_fn(&mut self, item: &ast::Item) {
        if !matches!(item.kind, ast::ItemKind::Fn(_))
            && let Some(a) = item.attrs.first()
        {
            self.error(a.span, "attributes can only be applied to functions");
        }
    }

    fn resolve_ty(&mut self, t: &ast::Type) -> Ty {
        let name = t.name.name.as_str();
        let no_args = |this: &mut Self, ty: Ty| {
            if !t.args.is_empty() {
                this.error(t.span, format!("type `{name}` does not take type arguments"));
            }
            ty
        };
        if let Some(m) = &t.module {
            if let Some(path) = self.builtin_path(m) {
                return match builtins::builtin_type(&path, name) {
                    Some(ty) => no_args(self, ty),
                    None => {
                        self.error(t.name.span, format!("module `{path}` has no type `{name}`"));
                        Ty::Error
                    }
                };
            }
            return match self.qualified_struct(m, &t.name) {
                Some(id) => no_args(self, Ty::Struct(id)),
                None => Ty::Error,
            };
        }
        if let Some(ty) = self.mods[self.cur].imported_builtin_types.get(name).cloned() {
            return no_args(self, ty);
        }
        match name {
            "int" => no_args(self, Ty::Int),
            "bool" => no_args(self, Ty::Bool),
            "str" => no_args(self, Ty::Str),
            "list" => {
                if t.args.len() != 1 {
                    self.error(t.span, "`list` takes exactly one type argument, as in `list<int>`");
                    return Ty::Error;
                }
                let inner = self.resolve_ty(&t.args[0]);
                if !inner.is_storable() {
                    self.error(t.args[0].span, format!("a list cannot hold `{}`", self.tn(&inner)));
                    return Ty::Error;
                }
                Ty::List(Box::new(inner))
            }
            _ => match self.lookup_struct(name) {
                Some(id) => no_args(self, Ty::Struct(id)),
                None => {
                    self.error_hint(t.span, format!("unknown type `{name}`"), "types are int, bool, str, list<T>, game::Entity and your structs");
                    Ty::Error
                }
            },
        }
    }

    fn resolve_signatures(&mut self) {
        // consts and globals get their declared types
        for i in 0..self.consts.len() {
            self.cur = self.consts[i].module;
            let d = self.consts[i].decl;
            let ty = self.resolve_ty(&d.ty);
            if !ty.is_storable() {
                self.error(d.ty.span, format!("a constant cannot have type `{}`", self.tn(&ty)));
            }
            self.consts[i].ty = ty;
        }
        for i in 0..self.global_decls.len() {
            let (d, m) = self.global_decls[i];
            self.cur = m;
            let ty = self.resolve_ty(&d.ty);
            if !ty.is_storable() {
                self.error(d.ty.span, format!("a global cannot have type `{}`", self.tn(&ty)));
            }
            self.globals[i].ty = ty;
        }
        // function signatures (entry attributes need constants, so this runs after the above)
        for fi in 0..self.fns.len() {
            self.cur = self.fns[fi].module;
            let f = self.fns[fi].decl;
            let attrs = self.fns[fi].attrs;
            let mut params = Vec::new();
            let mut seen = HashSet::new();
            for p in &f.params {
                if !seen.insert(p.name.name.clone()) {
                    self.error(p.name.span, format!("parameter `{}` is declared twice", p.name.name));
                }
                let ty = self.resolve_ty(&p.ty);
                if !ty.is_storable() {
                    self.error(p.ty.span, format!("a parameter cannot have type `{}`", self.tn(&ty)));
                }
                params.push(ty);
            }
            let ret = match &f.ret {
                Some(t) => {
                    let ty = self.resolve_ty(t);
                    if !ty.is_storable() {
                        self.error(t.span, format!("a function cannot return `{}`", self.tn(&ty)));
                    }
                    ty
                }
                None => Ty::Unit,
            };
            let entry = self.entry_of(attrs, f, !params.is_empty(), &ret);
            self.fns[fi].params = params;
            self.fns[fi].ret = ret;
            self.fns[fi].entry = entry;
        }
    }

    fn entry_of(&mut self, attrs: &'a [ast::Attr], f: &'a ast::FnDecl, has_params: bool, ret: &Ty) -> Option<EntryKind> {
        let mut found: Option<(EntryKind, Span)> = None;
        for a in attrs {
            let kind = match a.name.name.as_str() {
                "load" | "tick" => {
                    if !a.args.is_empty() {
                        self.error(a.span, format!("#[{}] takes no arguments", a.name.name));
                    }
                    if a.name.name == "load" { EntryKind::Load } else { EntryKind::Tick }
                }
                "every" => {
                    if a.args.len() != 1 {
                        self.error(a.span, "#[every(N)] takes exactly one argument: a number of ticks such as `20` or `1s`");
                        continue;
                    }
                    match self.const_expr(&a.args[0], Some(&Ty::Int)) {
                        Some((ConstVal::Int(n), _)) if n >= 1 => EntryKind::Every(n as u32),
                        Some((ConstVal::Int(_), _)) => {
                            self.error(a.args[0].span, "#[every(N)] needs N of at least 1 tick");
                            continue;
                        }
                        Some(_) => {
                            self.error(a.args[0].span, "#[every(N)] needs an integer number of ticks");
                            continue;
                        }
                        None => continue,
                    }
                }
                other => {
                    self.error_hint(a.name.span, format!("unknown attribute `{other}`"), "the attributes are #[load], #[tick] and #[every(N)]");
                    continue;
                }
            };
            if found.is_some() {
                self.error(a.span, "a function can have only one of #[load], #[tick] and #[every]");
            } else {
                found = Some((kind, a.span));
            }
        }
        let (kind, span) = found?;
        if has_params {
            self.error(span, format!("function `{}` runs automatically, so it cannot take parameters", f.name.name));
        }
        if *ret != Ty::Unit {
            self.error(span, format!("function `{}` runs automatically, so it cannot return a value", f.name.name));
        }
        Some(kind)
    }

    fn eval_globals(&mut self) {
        // force every const, even unused ones, so mistakes in them are reported
        for i in 0..self.consts.len() {
            self.cur = self.consts[i].module;
            self.force_const(i, self.consts[i].decl.name.span);
        }
        for gi in 0..self.global_decls.len() {
            let (d, m) = self.global_decls[gi];
            self.cur = m;
            let ty = self.globals[gi].ty.clone();
            if let Some((v, vt)) = self.const_expr(&d.init, Some(&ty)) {
                if vt.compatible(&ty) {
                    self.globals[gi].init = v;
                } else {
                    self.mismatch(d.init.span, &ty, &vt);
                }
            }
        }
    }

    // --------------------------------------------------- constant evaluation

    fn const_of_sym(&mut self, sym: Option<ValueSym>, name: &str, span: Span) -> Option<(ConstVal, Ty)> {
        match sym {
            Some(ValueSym::Const(ci)) => self.force_const(ci, span),
            Some(ValueSym::Global(_)) => {
                self.error_hint(span, format!("`{name}` is a global, not a constant"), "only `const` values can be used here");
                None
            }
            _ => {
                self.error(span, format!("`{name}` is not a constant"));
                None
            }
        }
    }

    fn force_const(&mut self, ci: usize, use_span: Span) -> Option<(ConstVal, Ty)> {
        let ty = self.consts[ci].ty.clone();
        match self.consts[ci].state.clone() {
            ConstState::Done(v) => Some((v, ty)),
            ConstState::Failed => None,
            ConstState::Evaluating => {
                let name = self.consts[ci].decl.name.name.clone();
                self.error(use_span, format!("constant `{name}` depends on itself"));
                self.consts[ci].state = ConstState::Failed;
                None
            }
            ConstState::Pending => {
                self.consts[ci].state = ConstState::Evaluating;
                let decl = self.consts[ci].decl;
                // names in the initialiser resolve in the module that declares the constant
                let saved = std::mem::replace(&mut self.cur, self.consts[ci].module);
                let result = self.const_expr(&decl.init, Some(&ty));
                self.cur = saved;
                match result {
                    Some((v, t)) if t.compatible(&ty) => {
                        self.consts[ci].state = ConstState::Done(v.clone());
                        Some((v, ty))
                    }
                    Some((_, t)) => {
                        let saved = std::mem::replace(&mut self.cur, self.consts[ci].module);
                        self.mismatch(decl.init.span, &ty, &t);
                        self.cur = saved;
                        self.consts[ci].state = ConstState::Failed;
                        None
                    }
                    None => {
                        self.consts[ci].state = ConstState::Failed;
                        None
                    }
                }
            }
        }
    }

    /// Evaluates a compile-time expression. Reports an error and returns None if it is not one.
    fn const_expr(&mut self, e: &'a ast::Expr, expected: Option<&Ty>) -> Option<(ConstVal, Ty)> {
        use ast::ExprKind as K;
        match &e.kind {
            K::Int(n) => {
                if !fits_i32(*n) {
                    self.error(e.span, "integer literal is out of range for `int` (-2147483648 to 2147483647)");
                    return None;
                }
                Some((ConstVal::Int(*n as i32), Ty::Int))
            }
            K::Bool(b) => Some((ConstVal::Bool(*b), Ty::Bool)),
            K::Str(s) => Some((ConstVal::Str(s.clone()), Ty::Str)),
            K::Float(_) => {
                self.error(e.span, "floating point numbers are not supported yet");
                None
            }
            K::Ident(name) => {
                let sym = self.lookup_value(name);
                self.const_of_sym(sym, name, e.span)
            }
            K::Path { module, name } => {
                let sym = self.qualified_value(module, name)?;
                self.const_of_sym(Some(sym), &name.name, e.span)
            }
            K::Unary { op: UnOp::Neg, expr } => {
                // `-2147483648` is a valid literal even though 2147483648 alone is not
                if let K::Int(n) = &expr.kind
                    && fits_i32(-*n) {
                        return Some((ConstVal::Int((-*n) as i32), Ty::Int));
                    }
                let (v, t) = self.const_expr(expr, None)?;
                match v {
                    ConstVal::Int(n) => match n.checked_neg() {
                        Some(r) => Some((ConstVal::Int(r), Ty::Int)),
                        None => {
                            self.error(e.span, "constant overflows `int`");
                            None
                        }
                    },
                    _ => {
                        self.error(e.span, format!("cannot negate `{}`", self.tn(&t)));
                        None
                    }
                }
            }
            K::Unary { op: UnOp::Not, expr } => {
                let (v, t) = self.const_expr(expr, None)?;
                match v {
                    ConstVal::Bool(b) => Some((ConstVal::Bool(!b), Ty::Bool)),
                    _ => {
                        self.error(e.span, format!("`!` needs a `bool`, found `{}`", self.tn(&t)));
                        None
                    }
                }
            }
            K::Binary { op, lhs, rhs } => {
                let (l, lt) = self.const_expr(lhs, None)?;
                let (r, rt) = self.const_expr(rhs, None)?;
                self.const_binary(*op, l, &lt, r, &rt, e.span)
            }
            K::List(items) => {
                let elem_expected = match expected {
                    Some(Ty::List(t)) => Some((**t).clone()),
                    _ => None,
                };
                let mut vals = Vec::new();
                let mut elem_ty = elem_expected;
                for item in items {
                    let (v, t) = self.const_expr(item, elem_ty.as_ref())?;
                    match &elem_ty {
                        Some(want) if !want.compatible(&t) => {
                            self.mismatch(item.span, want, &t);
                            return None;
                        }
                        None => elem_ty = Some(t),
                        _ => {}
                    }
                    vals.push(v);
                }
                match elem_ty {
                    Some(t) => Some((ConstVal::List(vals), Ty::List(Box::new(t)))),
                    None => {
                        self.error_hint(e.span, "cannot infer the type of an empty list", "add a type annotation, as in `const XS: list<int> = [];`");
                        None
                    }
                }
            }
            K::StructLit { module, name, fields } => {
                let id = self.find_struct(module.as_ref(), name)?;
                self.const_struct(id, fields, e.span)
            }
            K::Compound(fields) => match expected {
                Some(Ty::Struct(id)) => self.const_struct(*id, fields, e.span),
                _ => {
                    self.error(e.span, "an anonymous `{ ... }` needs a struct type from its context");
                    None
                }
            },
            _ => {
                self.error(e.span, "this is not a constant expression");
                None
            }
        }
    }

    fn const_struct(&mut self, id: StructId, fields: &'a [ast::FieldInit], span: Span) -> Option<(ConstVal, Ty)> {
        let decl: Vec<(String, Ty)> = self.structs[id.0].fields.clone();
        let mut values: Vec<Option<ConstVal>> = vec![None; decl.len()];
        for init in fields {
            let Some(idx) = decl.iter().position(|(n, _)| *n == init.name) else {
                self.error(init.name_span, format!("struct `{}` has no field `{}`", self.structs[id.0].name, init.name));
                return None;
            };
            if values[idx].is_some() {
                self.error(init.name_span, format!("field `{}` is set twice", init.name));
                return None;
            }
            let (v, t) = self.const_expr(&init.value, Some(&decl[idx].1))?;
            if !decl[idx].1.compatible(&t) {
                self.mismatch(init.value.span, &decl[idx].1, &t);
                return None;
            }
            values[idx] = Some(v);
        }
        let missing: Vec<&str> = decl.iter().zip(&values).filter(|(_, v)| v.is_none()).map(|((n, _), _)| n.as_str()).collect();
        if !missing.is_empty() {
            self.error(span, format!("missing field(s) in `{}`: {}", self.structs[id.0].name, missing.join(", ")));
            return None;
        }
        Some((ConstVal::Struct(id, values.into_iter().map(Option::unwrap).collect()), Ty::Struct(id)))
    }

    fn const_binary(&mut self, op: BinOp, l: ConstVal, lt: &Ty, r: ConstVal, rt: &Ty, span: Span) -> Option<(ConstVal, Ty)> {
        use ConstVal::*;
        let bad = |this: &mut Self| {
            let msg = format!("cannot apply `{}` to `{}` and `{}`", op_str(op), this.tn(lt), this.tn(rt));
            this.error(span, msg);
            None
        };
        match (l, r) {
            (Int(a), Int(b)) => match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul => {
                    let v = match op {
                        BinOp::Add => a.checked_add(b),
                        BinOp::Sub => a.checked_sub(b),
                        _ => a.checked_mul(b),
                    };
                    match v {
                        Some(v) => Some((Int(v), Ty::Int)),
                        None => {
                            self.error(span, "constant overflows `int`");
                            None
                        }
                    }
                }
                BinOp::Div | BinOp::Rem => match floor_div(a, b) {
                    Some(q) => {
                        let v = if op == BinOp::Div { q } else { (a as i64 - q as i64 * b as i64) as i32 };
                        Some((Int(v), Ty::Int))
                    }
                    None => {
                        self.error(span, "division by zero in a constant");
                        None
                    }
                },
                BinOp::Eq => Some((Bool(a == b), Ty::Bool)),
                BinOp::Ne => Some((Bool(a != b), Ty::Bool)),
                BinOp::Lt => Some((Bool(a < b), Ty::Bool)),
                BinOp::Le => Some((Bool(a <= b), Ty::Bool)),
                BinOp::Gt => Some((Bool(a > b), Ty::Bool)),
                BinOp::Ge => Some((Bool(a >= b), Ty::Bool)),
                BinOp::And | BinOp::Or => bad(self),
            },
            (Str(a), Str(b)) if op == BinOp::Add => Some((Str(a + &b), Ty::Str)),
            (Bool(a), Bool(b)) => match op {
                BinOp::And => Some((Bool(a && b), Ty::Bool)),
                BinOp::Or => Some((Bool(a || b), Ty::Bool)),
                BinOp::Eq => Some((Bool(a == b), Ty::Bool)),
                BinOp::Ne => Some((Bool(a != b), Ty::Bool)),
                _ => bad(self),
            },
            _ => bad(self),
        }
    }

    fn const_to_expr(&self, v: &ConstVal, ty: &Ty, span: Span) -> Expr {
        let kind = match v {
            ConstVal::Int(n) => ExprKind::Int(*n),
            ConstVal::Bool(b) => ExprKind::Bool(*b),
            ConstVal::Str(s) => ExprKind::Str(s.clone()),
            ConstVal::List(vs) => {
                let elem = match ty {
                    Ty::List(t) => (**t).clone(),
                    _ => Ty::Error,
                };
                ExprKind::List(vs.iter().map(|v| self.const_to_expr(v, &elem, span)).collect())
            }
            ConstVal::Struct(id, vs) => {
                let tys: Vec<Ty> = self.structs[id.0].fields.iter().map(|(_, t)| t.clone()).collect();
                ExprKind::Struct { id: *id, fields: vs.iter().zip(&tys).map(|(v, t)| self.const_to_expr(v, t, span)).collect() }
            }
        };
        Expr { kind, ty: ty.clone(), span }
    }

    // -------------------------------------------------------------- functions

    fn fctx(&mut self) -> &mut FnCtx {
        self.f.as_mut().expect("inside a function")
    }

    fn check_fn(&mut self, idx: usize) {
        self.cur = self.fns[idx].module;
        let decl: &'a ast::FnDecl = self.fns[idx].decl;
        let sig_params = self.fns[idx].params.clone();
        let ret = self.fns[idx].ret.clone();
        self.f = Some(FnCtx { locals: Vec::new(), used: Vec::new(), scopes: vec![HashMap::new()], ret: ret.clone(), loop_depth: 0, ctx_depth: 0 });
        let mut params = Vec::new();
        for (p, ty) in decl.params.iter().zip(sig_params) {
            // parameters are the function's own copies, so they may be assigned
            params.push(self.declare_local(&p.name, ty, true, true));
        }
        let body = self.block(&decl.body);
        if ret != Ty::Unit && !ret.is_error() && !body.diverges() {
            self.error_hint(decl.name.span, format!("function `{}` does not return a value on every path", decl.name.name), "end it with a `return`, or make sure every branch returns");
        }
        let ctx = self.f.take().expect("function context");
        for (i, l) in ctx.locals.iter().enumerate() {
            if !ctx.used[i] && !l.name.starts_with('_') {
                let d = Diagnostic::warning(format!("unused variable `{}`", l.name), l.span).hint("prefix the name with `_` to silence this").in_file(self.file());
                self.warnings.push(d);
            }
        }
        self.hir_fns.push(FnDef {
            name: decl.name.name.clone(),
            module: self.fns[idx].module,
            params,
            ret,
            locals: ctx.locals,
            body,
            entry: self.fns[idx].entry,
            span: self.fns[idx].span,
        });
    }

    fn declare_local(&mut self, name: &ast::Ident, ty: Ty, mutable: bool, is_param: bool) -> LocalId {
        let ctx = self.fctx();
        let id = LocalId(ctx.locals.len());
        ctx.locals.push(LocalInfo { name: name.name.clone(), ty, mutable, span: name.span });
        ctx.used.push(is_param);
        ctx.scopes.last_mut().expect("scope").insert(name.name.clone(), id);
        id
    }

    fn lookup_local(&mut self, name: &str) -> Option<LocalId> {
        let ctx = self.fctx();
        let id = ctx.scopes.iter().rev().find_map(|s| s.get(name).copied())?;
        ctx.used[id.0] = true;
        Some(id)
    }

    fn block(&mut self, b: &'a ast::Block) -> Block {
        self.fctx().scopes.push(HashMap::new());
        let out = self.stmts(&b.stmts);
        self.fctx().scopes.pop();
        out
    }

    fn stmts(&mut self, stmts: &'a [ast::Stmt]) -> Block {
        let mut out = Vec::new();
        let mut dead = false;
        let mut warned = false;
        for s in stmts {
            if dead && !warned {
                self.warn(s.span, "unreachable statement");
                warned = true;
            }
            let hs = self.stmt(s);
            dead |= hs.diverges();
            out.push(hs);
        }
        Block { stmts: out }
    }

    fn expect_bool(&mut self, e: &'a ast::Expr) -> Expr {
        let c = self.expr(e, Some(&Ty::Bool));
        self.mismatch(c.span, &Ty::Bool, &c.ty);
        c
    }

    fn stmt(&mut self, s: &'a ast::Stmt) -> Stmt {
        use ast::StmtKind as K;
        match &s.kind {
            K::Let { name, mutable, ty, init } => {
                let declared = ty.as_ref().map(|t| self.resolve_ty(t));
                let mut value = self.expr(init, declared.as_ref());
                if declared.is_none() && value.ty == Ty::Selector {
                    // `let me = @p;` holds the entity, not the query
                    value = self.coerce_to_entity(value);
                }
                let ty = match declared {
                    Some(t) => {
                        self.mismatch(value.span, &t, &value.ty);
                        t
                    }
                    None => value.ty.clone(),
                };
                if !ty.is_storable() {
                    let msg = if ty == Ty::Unit { "this expression has no value to store".to_owned() } else { format!("a value of type `{}` cannot be stored in a variable", self.tn(&ty)) };
                    self.error(init.span, msg);
                }
                let local = self.declare_local(name, ty, *mutable, false);
                Stmt::Let { local, init: value }
            }
            K::Assign { target, op, value } => {
                let t = self.expr(target, None);
                self.check_place(&t);
                let v = self.expr(value, Some(&t.ty));
                match op {
                    AssignOp::Assign => self.mismatch(v.span, &t.ty, &v.ty),
                    AssignOp::Add if t.ty == Ty::Str => self.mismatch(v.span, &Ty::Str, &v.ty),
                    _ => {
                        if !t.ty.is_error() && t.ty != Ty::Int {
                            // one mistake, one error: don't also complain about the value
                            self.error(s.span, format!("`{}` needs an `int` target, found `{}`", assign_str(*op), self.tn(&t.ty)));
                        } else {
                            self.mismatch(v.span, &Ty::Int, &v.ty);
                        }
                    }
                }
                Stmt::Assign { target: t, op: *op, value: v }
            }
            K::If { cond, then, els } => {
                let cond = self.expect_bool(cond);
                let then = self.block(then);
                let els = match els.as_deref() {
                    None => Block::default(),
                    Some(ast::Else::Block(b)) => self.block(b),
                    Some(ast::Else::If(st)) => Block { stmts: vec![self.stmt(st)] },
                };
                Stmt::If { cond, then, els }
            }
            K::While { cond, body } => {
                let cond = self.expect_bool(cond);
                self.fctx().loop_depth += 1;
                let body = self.block(body);
                self.fctx().loop_depth -= 1;
                Stmt::While { cond, body }
            }
            K::Loop { body } => {
                self.fctx().loop_depth += 1;
                let body = self.block(body);
                self.fctx().loop_depth -= 1;
                Stmt::Loop { body }
            }
            K::For { var, iter, body } => self.for_stmt(var, iter, body),
            K::Return(value) => self.return_stmt(value.as_ref(), s.span),
            K::Break | K::Continue => {
                let what = if matches!(s.kind, K::Break) { "break" } else { "continue" };
                let ctx = self.fctx();
                if ctx.loop_depth == 0 {
                    let in_ctx = ctx.ctx_depth > 0;
                    let msg = if in_ctx { format!("`{what}` cannot leave a context block") } else { format!("`{what}` outside of a loop") };
                    self.error(s.span, msg);
                }
                if what == "break" { Stmt::Break } else { Stmt::Continue }
            }
            K::Context { clauses, body } => self.context_stmt(clauses, body),
            K::Block(b) => Stmt::Block(self.block(b)),
            K::Expr(e) => {
                let hir = self.expr(e, None);
                if !matches!(hir.kind, ExprKind::Call { .. } | ExprKind::Builtin { .. } | ExprKind::Method { .. } | ExprKind::Error) {
                    self.warn(e.span, "this expression has no effect");
                }
                Stmt::Expr(hir)
            }
        }
    }

    fn return_stmt(&mut self, value: Option<&'a ast::Expr>, span: Span) -> Stmt {
        if self.fctx().ctx_depth > 0 {
            self.error_hint(span, "`return` is not allowed inside a context block", "a context block may run once per entity, so it cannot end the function; move the `return` outside it");
        }
        let ret = self.fctx().ret.clone();
        match value {
            Some(e) => {
                let v = self.expr(e, Some(&ret));
                if ret == Ty::Unit {
                    self.error(e.span, "this function returns nothing, so `return` takes no value");
                } else {
                    self.mismatch(v.span, &ret, &v.ty);
                }
                Stmt::Return(Some(v))
            }
            None => {
                if ret != Ty::Unit && !ret.is_error() {
                    let msg = format!("`return` needs a value of type `{}`", self.tn(&ret));
                    self.error(span, msg);
                }
                Stmt::Return(None)
            }
        }
    }

    fn for_stmt(&mut self, var: &'a ast::Ident, iter: &'a ast::Expr, body: &'a ast::Block) -> Stmt {
        // `for e in @a { ... }` iterates a selector: the body runs once per match (like
        // `as @a { ... }`), with `e` a fresh immutable `entity` local bound to the current
        // entity at the top of each run.
        if let ast::ExprKind::Selector(_) = &iter.kind {
            let sel = self.expr(iter, Some(&Ty::Selector));
            self.fctx().scopes.push(HashMap::new());
            let id = self.declare_local(var, Ty::Entity, false, false);
            self.fctx().ctx_depth += 1;
            let mut body = self.block(body);
            self.fctx().ctx_depth -= 1;
            // declared (and bound) at the top of every run; unused names (`for _ in ..`) get no binding at all
            if !var.name.starts_with('_') {
                body.stmts.insert(0, Stmt::Let { local: id, init: self.current_entity(var.span) });
            }
            self.fctx().scopes.pop();
            return Stmt::Context { clauses: vec![CtxClause::As(sel)], body };
        }
        if let ast::ExprKind::Range { start, end, inclusive } = &iter.kind {
            let (Some(start), Some(end)) = (start, end) else {
                self.error(iter.span, "a `for` range needs both a start and an end, as in `0..10`");
                return Stmt::Expr(Expr::error(iter.span));
            };
            let s = self.expr(start, Some(&Ty::Int));
            let e = self.expr(end, Some(&Ty::Int));
            self.mismatch(s.span, &Ty::Int, &s.ty);
            self.mismatch(e.span, &Ty::Int, &e.ty);
            self.fctx().scopes.push(HashMap::new());
            let id = self.declare_local(var, Ty::Int, false, false);
            self.fctx().loop_depth += 1;
            let body = self.block(body);
            self.fctx().loop_depth -= 1;
            self.fctx().scopes.pop();
            return Stmt::ForRange { var: id, start: s, end: e, inclusive: *inclusive, body };
        }
        let list = self.expr(iter, None);
        let elem = match &list.ty {
            Ty::List(t) => (**t).clone(),
            Ty::Error => Ty::Error,
            Ty::Entity => {
                self.error_hint(iter.span, "cannot iterate over an `Entity`", "an entity is exactly one: use it directly (`tp(name, ...)`) or act on it with `as`");
                Ty::Error
            }
            other => {
                let msg = format!("cannot iterate over `{}`", self.tn(other));
                self.error_hint(iter.span, msg, "loop over a range (`0..n`), a list, or a selector (`for x in @e[type=zombie]`)");
                Ty::Error
            }
        };
        self.fctx().scopes.push(HashMap::new());
        let id = self.declare_local(var, elem, false, false);
        self.fctx().loop_depth += 1;
        let body = self.block(body);
        self.fctx().loop_depth -= 1;
        self.fctx().scopes.pop();
        Stmt::ForList { var: id, list, body }
    }

    fn context_stmt(&mut self, clauses: &'a [ast::CtxClause], body: &'a ast::Block) -> Stmt {
        let mut out = Vec::new();
        for c in clauses {
            if let Some(h) = self.ctx_clause(c) {
                out.push(h);
            }
        }
        let saved_loops = std::mem::take(&mut self.fctx().loop_depth);
        self.fctx().ctx_depth += 1;
        let body = self.block(body);
        self.fctx().ctx_depth -= 1;
        self.fctx().loop_depth = saved_loops;
        Stmt::Context { clauses: out, body }
    }

    /// A target: a selector, or an entity value.
    fn selector_operand(&mut self, e: &'a ast::Expr) -> Expr {
        let h = self.expr(e, None);
        if !matches!(h.ty, Ty::Selector | Ty::Entity | Ty::Error) {
            let msg = format!("expected a `selector` or an `Entity`, found `{}`", self.tn(&h.ty));
            self.error(h.span, msg);
        }
        h
    }

    fn coord_operand(&mut self, e: &'a ast::Expr) -> Expr {
        let h = self.expr(e, Some(&Ty::Int));
        if !matches!(h.ty, Ty::Int | Ty::Coord | Ty::Error) {
            let msg = format!("expected a coordinate (an `int` or `~`/`^`), found `{}`", self.tn(&h.ty));
            self.error(h.span, msg);
        }
        h
    }

    fn ctx_clause(&mut self, c: &'a ast::CtxClause) -> Option<CtxClause> {
        use ast::CtxKind as K;
        Some(match &c.kind {
            K::As(e) => CtxClause::As(self.selector_operand(e)),
            K::At(e) => CtxClause::At(self.selector_operand(e)),
            K::In(e) => {
                let (v, _) = self.const_expr(e, Some(&Ty::Str))?;
                let ConstVal::Str(s) = v else {
                    self.error(e.span, "`in` needs a dimension name such as \"minecraft:the_nether\"");
                    return None;
                };
                if let Err(err) = ResourceLocation::parse(&s) {
                    self.error(e.span, format!("invalid dimension id: {}", err.message()));
                    return None;
                }
                CtxClause::In(s)
            }
            K::Positioned([x, y, z]) => CtxClause::Positioned([self.coord_operand(x), self.coord_operand(y), self.coord_operand(z)]),
            K::Facing([x, y, z]) => CtxClause::Facing([self.coord_operand(x), self.coord_operand(y), self.coord_operand(z)]),
            K::Rotated([a, b]) => CtxClause::Rotated([self.coord_operand(a), self.coord_operand(b)]),
            K::Align(axes) => {
                let s = axes.name.as_str();
                let valid = !s.is_empty() && s.chars().all(|ch| matches!(ch, 'x' | 'y' | 'z')) && s.chars().collect::<HashSet<_>>().len() == s.len();
                if !valid {
                    self.error_hint(axes.span, format!("invalid axes `{s}`"), "use a combination of x, y and z without repeats, such as `xz` or `xyz`");
                    return None;
                }
                CtxClause::Align(['x', 'y', 'z'].iter().filter(|ax| s.contains(**ax)).collect())
            }
            K::Anchored(a) => {
                if a.name != "eyes" && a.name != "feet" {
                    self.error(a.span, format!("`anchored` takes `eyes` or `feet`, found `{}`", a.name));
                    return None;
                }
                CtxClause::Anchored(a.name.clone())
            }
        })
    }

    /// Verifies that `e` can be assigned to, reporting why not.
    fn check_place(&mut self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Local(id) => {
                let info = self.f.as_ref().expect("fn").locals[id.0].clone();
                if !info.mutable {
                    self.error_hint(e.span, format!("cannot assign to `{}` because it is not mutable", info.name), format!("declare it with `let mut {}`", info.name));
                    return false;
                }
                true
            }
            ExprKind::Global(_) | ExprKind::Score { .. } => true,
            ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => self.check_place(base),
            ExprKind::Error => false,
            _ => {
                self.error(e.span, "cannot assign to this expression");
                false
            }
        }
    }

    // ------------------------------------------------------------ expressions

    /// Checks an expression. A selector that matches one entity becomes an entity value wherever an
    /// `entity` is expected (annotation, argument, return, field, list element, assignment).
    fn expr(&mut self, e: &'a ast::Expr, expected: Option<&Ty>) -> Expr {
        let h = self.expr_inner(e, expected);
        if matches!(expected, Some(Ty::Entity)) && h.ty == Ty::Selector {
            return self.coerce_to_entity(h);
        }
        h
    }

    fn coerce_to_entity(&mut self, h: Expr) -> Expr {
        match &h.kind {
            ExprKind::Selector(sel) if sel.is_single() => {
                let span = h.span;
                Expr { kind: ExprKind::ToEntity(Box::new(h)), ty: Ty::Entity, span }
            }
            ExprKind::Selector(sel) => {
                let msg = format!("`{sel}` can match several entities, so it cannot be one entity value");
                self.error_hint(h.span, msg, "use a selector for one entity (`@s`, `@p`, `@r`, `@n`, or `limit=1`), or act on all of them with `as`");
                Expr::error(h.span)
            }
            _ => h,
        }
    }

    fn expr_inner(&mut self, e: &'a ast::Expr, expected: Option<&Ty>) -> Expr {
        use ast::ExprKind as K;
        let span = e.span;
        let mk = |kind: ExprKind, ty: Ty| Expr { kind, ty, span };
        match &e.kind {
            K::Int(n) => {
                if !fits_i32(*n) {
                    self.error(span, "integer literal is out of range for `int` (-2147483648 to 2147483647)");
                    return Expr::error(span);
                }
                mk(ExprKind::Int(*n as i32), Ty::Int)
            }
            K::Float(_) => {
                self.error(span, "floating point numbers are not supported yet");
                Expr::error(span)
            }
            K::Bool(b) => mk(ExprKind::Bool(*b), Ty::Bool),
            K::Str(s) => mk(ExprKind::Str(s.clone()), Ty::Str),
            K::Format(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        ast::FormatPart::Lit(s) => out.push(FormatPart::Lit(s.clone())),
                        ast::FormatPart::Expr(inner) => {
                            let h = self.expr(inner, None);
                            if !h.ty.is_storable() {
                                let msg = format!("cannot format a value of type `{}`", self.tn(&h.ty));
                                self.error(h.span, msg);
                            }
                            out.push(FormatPart::Expr(h));
                        }
                    }
                }
                if out.iter().all(|p| matches!(p, FormatPart::Lit(_))) {
                    // nothing to compute: an ordinary string
                    let text = out.iter().map(|p| if let FormatPart::Lit(s) = p { s.as_str() } else { "" }).collect::<String>();
                    return mk(ExprKind::Str(text), Ty::Str);
                }
                mk(ExprKind::Format(out), Ty::Str)
            }
            K::Ident(name) => self.ident(name, span),
            K::List(items) => self.list_lit(items, expected, span),
            K::Compound(fields) => match expected {
                Some(Ty::Struct(id)) => self.struct_lit(*id, fields, span),
                _ => {
                    self.error(span, "an anonymous `{ ... }` needs a struct type from its context");
                    Expr::error(span)
                }
            },
            K::StructLit { module, name, fields } => match self.find_struct(module.as_ref(), name) {
                Some(id) => self.struct_lit(id, fields, span),
                None => {
                    self.args_for_errors_fields(fields);
                    Expr::error(span)
                }
            },
            K::Path { module, name } => match self.qualified_value(module, name) {
                Some(sym) => self.sym_expr(Some(sym), &name.name, span),
                None => Expr::error(span),
            },
            K::Selector(lit) => match selector::convert(lit) {
                Ok(sel) => mk(ExprKind::Selector(sel), Ty::Selector),
                Err(ds) => {
                    let file = self.file();
                    self.errors.extend(ds.into_iter().map(|d| d.in_file(file)));
                    Expr::error(span)
                }
            },
            K::Rel { kind, offset } => {
                if let Some(o) = offset
                    && o.parse::<f64>().is_err() {
                        self.error(span, "invalid coordinate offset");
                    }
                mk(ExprKind::Rel { kind: *kind, offset: offset.clone() }, Ty::Coord)
            }
            K::Unary { op, expr } => self.unary(*op, expr, span),
            K::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, span),
            K::Range { .. } => {
                self.error_hint(span, "a range is only allowed as the subject of a `for` loop", "write `for i in 0..n { ... }`");
                Expr::error(span)
            }
            K::Call { callee, args } => self.call(callee, args, span),
            K::Field { base, name } => self.field(base, name, span),
            K::Index { base, index } => self.index(base, index, span),
        }
    }

    fn ident(&mut self, name: &str, span: Span) -> Expr {
        if let Some(id) = self.lookup_local(name) {
            let ty = self.fctx().locals[id.0].ty.clone();
            return Expr { kind: ExprKind::Local(id), ty, span };
        }
        let sym = self.lookup_value(name);
        self.sym_expr(sym, name, span)
    }

    /// The entity the body of a `for e in @a { }` runs as, as a handle: an `entity`-typed
    /// read of the id score of the current `@s` (`docs/lang/entities.md`).
    fn current_entity(&mut self, span: Span) -> Expr {
        let sel = Expr { kind: ExprKind::Selector(crate::datapack::Selector::new(crate::datapack::SelectorVar::S)), ty: Ty::Selector, span };
        Expr { kind: ExprKind::ToEntity(Box::new(sel)), ty: Ty::Entity, span }
    }

    fn sym_expr(&mut self, sym: Option<ValueSym>, name: &str, span: Span) -> Expr {
        match sym {
            Some(ValueSym::Global(id)) => Expr { kind: ExprKind::Global(id), ty: self.globals[id.0].ty.clone(), span },
            Some(ValueSym::Const(ci)) => match self.force_const(ci, span) {
                Some((v, ty)) => self.const_to_expr(&v, &ty, span),
                None => Expr::error(span),
            },
            Some(ValueSym::Fn(_) | ValueSym::Builtin(_)) => {
                self.error_hint(span, format!("function `{name}` is used as a value"), format!("call it: `{name}(...)`"));
                Expr::error(span)
            }
            Some(ValueSym::Objective(_)) => {
                self.error_hint(span, format!("objective `{name}` needs an index"), format!("read or write a score with `{name}[@s]`"));
                Expr::error(span)
            }
            None => {
                if builtins::is_module(name) || self.mods[self.cur].builtin_modules.contains_key(name) {
                    self.error_hint(span, format!("`{name}` is a module, not a value"), format!("use one of its items: `{name}::item`"));
                } else if let Some(hint) = self.builtin_hint(name) {
                    self.error_hint(span, format!("cannot find `{name}` in this scope"), hint);
                } else {
                    self.error(span, format!("cannot find `{name}` in this scope"));
                }
                Expr::error(span)
            }
        }
    }

    fn list_lit(&mut self, items: &'a [ast::Expr], expected: Option<&Ty>, span: Span) -> Expr {
        let mut elem = match expected {
            Some(Ty::List(t)) => Some((**t).clone()),
            _ => None,
        };
        let mut out = Vec::new();
        for item in items {
            let h = self.expr(item, elem.as_ref());
            match &elem {
                Some(want) => self.mismatch(h.span, want, &h.ty),
                None => elem = Some(h.ty.clone()),
            }
            out.push(h);
        }
        match elem {
            Some(t) => {
                if !t.is_storable() {
                    let msg = format!("a list cannot hold `{}`", self.tn(&t));
                    self.error(span, msg);
                }
                Expr { kind: ExprKind::List(out), ty: Ty::List(Box::new(t)), span }
            }
            None => {
                self.error_hint(span, "cannot infer the type of an empty list", "add a type annotation, as in `let xs: list<int> = [];`");
                Expr::error(span)
            }
        }
    }

    fn struct_lit(&mut self, id: StructId, fields: &'a [ast::FieldInit], span: Span) -> Expr {
        let decl: Vec<(String, Ty)> = self.structs[id.0].fields.clone();
        let sname = self.structs[id.0].name.clone();
        let mut values: Vec<Option<Expr>> = (0..decl.len()).map(|_| None).collect();
        for init in fields {
            let Some(idx) = decl.iter().position(|(n, _)| *n == init.name) else {
                self.error(init.name_span, format!("struct `{sname}` has no field `{}`", init.name));
                continue;
            };
            let v = self.expr(&init.value, Some(&decl[idx].1));
            self.mismatch(v.span, &decl[idx].1, &v.ty);
            if values[idx].is_some() {
                self.error(init.name_span, format!("field `{}` is set twice", init.name));
            }
            values[idx] = Some(v);
        }
        let missing: Vec<&str> = decl.iter().zip(&values).filter(|(_, v)| v.is_none()).map(|((n, _), _)| n.as_str()).collect();
        if !missing.is_empty() {
            self.error(span, format!("missing field(s) in `{sname}`: {}", missing.join(", ")));
        }
        let fields = values.into_iter().map(|v| v.unwrap_or_else(|| Expr::error(span))).collect();
        Expr { kind: ExprKind::Struct { id, fields }, ty: Ty::Struct(id), span }
    }

    fn unary(&mut self, op: UnOp, inner: &'a ast::Expr, span: Span) -> Expr {
        match op {
            UnOp::Neg => {
                // `-2147483648`
                if let ast::ExprKind::Int(n) = &inner.kind
                    && fits_i32(-*n) {
                        return Expr { kind: ExprKind::Int((-*n) as i32), ty: Ty::Int, span };
                    }
                let e = self.expr(inner, Some(&Ty::Int));
                self.operand(&e, &Ty::Int, "-");
                Expr { kind: ExprKind::Unary { op, expr: Box::new(e) }, ty: Ty::Int, span }
            }
            UnOp::Not => {
                let e = self.expr(inner, Some(&Ty::Bool));
                self.operand(&e, &Ty::Bool, "!");
                Expr { kind: ExprKind::Unary { op, expr: Box::new(e) }, ty: Ty::Bool, span }
            }
        }
    }

    fn operand(&mut self, e: &Expr, want: &Ty, op: &str) {
        if !want.compatible(&e.ty) {
            let msg = format!("`{op}` needs `{}`, found `{}`", self.tn(want), self.tn(&e.ty));
            self.error(e.span, msg);
        }
    }

    fn binary(&mut self, op: BinOp, lhs: &'a ast::Expr, rhs: &'a ast::Expr, span: Span) -> Expr {
        let l = self.expr(lhs, None);
        let r = self.expr(rhs, None);
        let poisoned = l.ty.is_error() || r.ty.is_error();
        let result = match op {
            BinOp::Add if !poisoned && l.ty == Ty::Str && r.ty == Ty::Str => {
                // two literals join at compile time
                if let (ExprKind::Str(a), ExprKind::Str(b)) = (&l.kind, &r.kind) {
                    return Expr { kind: ExprKind::Str(format!("{a}{b}")), ty: Ty::Str, span };
                }
                Ty::Str
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                if !poisoned && (l.ty != Ty::Int || r.ty != Ty::Int) {
                    self.bin_error(op, &l.ty, &r.ty, span);
                }
                Ty::Int
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                if !poisoned && (l.ty != Ty::Int || r.ty != Ty::Int) {
                    self.bin_error(op, &l.ty, &r.ty, span);
                }
                Ty::Bool
            }
            BinOp::Eq | BinOp::Ne => {
                if !poisoned {
                    if l.ty != r.ty {
                        self.bin_error(op, &l.ty, &r.ty, span);
                    } else if !matches!(l.ty, Ty::Int | Ty::Bool | Ty::Entity) {
                        let msg = format!("`{}` on `{}` values is not supported yet (only `int`, `bool` and `Entity`)", op_str(op), self.tn(&l.ty));
                        self.error(span, msg);
                    }
                }
                Ty::Bool
            }
            BinOp::And | BinOp::Or => {
                if !poisoned && (l.ty != Ty::Bool || r.ty != Ty::Bool) {
                    self.bin_error(op, &l.ty, &r.ty, span);
                }
                Ty::Bool
            }
        };
        Expr { kind: ExprKind::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }, ty: if poisoned { Ty::Error } else { result }, span }
    }

    fn bin_error(&mut self, op: BinOp, l: &Ty, r: &Ty, span: Span) {
        let msg = format!("cannot apply `{}` to `{}` and `{}`", op_str(op), self.tn(l), self.tn(r));
        self.error(span, msg);
    }

    fn field(&mut self, base: &'a ast::Expr, name: &'a ast::Ident, span: Span) -> Expr {
        if let Some(m) = self.module_dot_misuse(base) {
            self.error_hint(span, format!("`{m}` is a module: its items are reached with `::`"), format!("write `{m}::{}`", name.name));
            return Expr::error(span);
        }
        let b = self.expr(base, None);
        match &b.ty {
            Ty::Struct(id) => match self.structs[id.0].fields.iter().position(|(n, _)| *n == name.name) {
                Some(i) => {
                    let ty = self.structs[id.0].fields[i].1.clone();
                    Expr { kind: ExprKind::Field { base: Box::new(b), index: i, name: name.name.clone() }, ty, span }
                }
                None => {
                    let msg = format!("struct `{}` has no field `{}`", self.structs[id.0].name, name.name);
                    self.error(name.span, msg);
                    // The base keeps its type, so a language server can offer field
                    // completions for it; lowering never runs while an error exists.
                    Expr { kind: ExprKind::Field { base: Box::new(b), index: usize::MAX, name: name.name.clone() }, ty: Ty::Error, span }
                }
            },
            Ty::Error => Expr::error(span),
            other => {
                let hint = if name.name == "len" { "did you mean the method `len()`?" } else { "only structs have fields" };
                let msg = format!("`{}` has no field `{}`", self.tn(other), name.name);
                self.error_hint(name.span, msg, hint);
                Expr::error(span)
            }
        }
    }

    fn lookup_value_or_local(&mut self, name: &str) -> Option<()> {
        if self.f.as_ref().is_some_and(|c| c.scopes.iter().any(|s| s.contains_key(name))) || self.lookup_value(name).is_some() {
            Some(())
        } else {
            None
        }
    }

    fn index(&mut self, base: &'a ast::Expr, index: &'a ast::Expr, span: Span) -> Expr {
        // `objective[holder]`, also `module::objective[holder]`
        let objective = match &base.kind {
            ast::ExprKind::Ident(name) => {
                let is_local = self.f.as_ref().is_some_and(|c| c.scopes.iter().any(|s| s.contains_key(name.as_str())));
                match (is_local, self.lookup_value(name)) {
                    (false, Some(ValueSym::Objective(obj))) => Some(obj),
                    _ => None,
                }
            }
            ast::ExprKind::Path { module, name } => match self.qualified_value(module, name) {
                Some(ValueSym::Objective(obj)) => Some(obj),
                _ => None,
            },
            _ => None,
        };
        {
            if let Some(obj) = objective {
                {
                    let holder = self.expr(index, None);
                    if !matches!(holder.ty, Ty::Selector | Ty::Entity | Ty::Str | Ty::Error) {
                        let msg = format!("a score holder is a selector or a name (`str`), found `{}`", self.tn(&holder.ty));
                        self.error(holder.span, msg);
                    }
                    if let ExprKind::Str(s) = &holder.kind
                        && let Err(e) = HolderName::new(s) {
                            self.error(holder.span, e.message().to_owned());
                        }
                    return Expr { kind: ExprKind::Score { objective: obj, holder: Box::new(holder) }, ty: Ty::Int, span };
                }
            }
        }
        let b = self.expr(base, None);
        let i = self.expr(index, Some(&Ty::Int));
        self.mismatch(i.span, &Ty::Int, &i.ty);
        match &b.ty {
            Ty::List(t) => {
                let ty = (**t).clone();
                Expr { kind: ExprKind::Index { base: Box::new(b), index: Box::new(i) }, ty, span }
            }
            Ty::Error => Expr::error(span),
            other => {
                let msg = format!("cannot index into `{}`", self.tn(other));
                self.error(span, msg);
                Expr::error(span)
            }
        }
    }

    // ------------------------------------------------------------------ calls

    fn call(&mut self, callee: &'a ast::Expr, args: &'a [ast::Expr], span: Span) -> Expr {
        match &callee.kind {
            ast::ExprKind::Ident(name) => {
                if self.lookup_value_or_local(name).is_some() && !matches!(self.lookup_value(name), Some(ValueSym::Fn(_) | ValueSym::Builtin(_))) {
                    self.error(callee.span, format!("`{name}` is not a function"));
                    self.args_for_errors(args);
                    return Expr::error(span);
                }
                match self.lookup_value(name) {
                    Some(ValueSym::Fn(id)) => self.user_call(id, args, span),
                    Some(ValueSym::Builtin(i)) => self.builtin_call(&builtins::BUILTINS[i], args, span),
                    _ => {
                        match self.builtin_hint(name) {
                            Some(hint) => self.error_hint(callee.span, format!("cannot find function `{name}`"), hint),
                            None => self.error(callee.span, format!("cannot find function `{name}`")),
                        }
                        self.args_for_errors(args);
                        Expr::error(span)
                    }
                }
            }
            ast::ExprKind::Path { module, name } => match self.qualified_value(module, name) {
                Some(ValueSym::Fn(id)) => self.user_call(id, args, span),
                Some(ValueSym::Builtin(i)) => self.builtin_call(&builtins::BUILTINS[i], args, span),
                Some(_) => {
                    self.error(callee.span, format!("`{}::{}` is not a function", module.name, name.name));
                    self.args_for_errors(args);
                    Expr::error(span)
                }
                None => {
                    self.args_for_errors(args);
                    Expr::error(span)
                }
            },
            ast::ExprKind::Field { base, name } => {
                if let Some(m) = self.module_dot_misuse(base) {
                    self.error_hint(callee.span, format!("`{m}` is a module: its functions are called with `::`"), format!("write `{m}::{}(...)`", name.name));
                    self.args_for_errors(args);
                    return Expr::error(span);
                }
                self.method_call(base, name, args, span)
            }
            _ => {
                self.error(callee.span, "this expression cannot be called");
                self.args_for_errors(args);
                Expr::error(span)
            }
        }
    }

    fn args_for_errors_fields(&mut self, fields: &'a [ast::FieldInit]) {
        for f in fields {
            self.expr(&f.value, None);
        }
    }

    /// Checks the arguments of a call that failed to resolve so errors inside them still show up.
    fn args_for_errors(&mut self, args: &'a [ast::Expr]) {
        for a in args {
            self.expr(a, None);
        }
    }

    fn arity_error(&mut self, what: &str, takes: String, got: usize, span: Span) {
        let plural = if got == 1 { "was" } else { "were" };
        self.error(span, format!("{what} takes {takes} but {got} {plural} supplied"));
    }

    fn user_call(&mut self, id: FuncId, args: &'a [ast::Expr], span: Span) -> Expr {
        let params = self.fns[id.0].params.clone();
        let ret = self.fns[id.0].ret.clone();
        let name = self.fns[id.0].decl.name.name.clone();
        if args.len() != params.len() {
            let takes = format!("{} argument{}", params.len(), if params.len() == 1 { "" } else { "s" });
            self.arity_error(&format!("function `{name}`"), takes, args.len(), span);
            self.args_for_errors(args);
            return Expr { kind: ExprKind::Error, ty: ret, span };
        }
        let mut hargs = Vec::new();
        for (a, ty) in args.iter().zip(&params) {
            let h = self.expr(a, Some(ty));
            self.mismatch(h.span, ty, &h.ty);
            hargs.push(h);
        }
        Expr { kind: ExprKind::Call { func: id, args: hargs }, ty: ret, span }
    }

    fn builtin_call(&mut self, b: &'static Builtin, args: &'a [ast::Expr], span: Span) -> Expr {
        let ret = b.ret.ty();
        if args.len() < b.min_args || args.len() > b.params.len() {
            let takes = if b.min_args == b.params.len() {
                format!("{} argument{}", b.params.len(), if b.params.len() == 1 { "" } else { "s" })
            } else {
                format!("{} to {} arguments", b.min_args, b.params.len())
            };
            self.arity_error(&format!("`{}`", b.path()), takes, args.len(), span);
            self.args_for_errors(args);
            return Expr { kind: ExprKind::Error, ty: ret, span };
        }
        let mut hargs = Vec::new();
        let mut ok = true;
        for (a, p) in args.iter().zip(b.params) {
            let h = match p {
                BParam::Printable => {
                    let h = self.expr(a, None);
                    if !h.ty.is_storable() {
                        let msg = format!("cannot print a value of type `{}`", self.tn(&h.ty));
                        self.error(h.span, msg);
                    }
                    h
                }
                BParam::Selector => self.selector_operand(a),
                BParam::Coord => self.coord_operand(a),
                BParam::Str => {
                    let h = self.expr(a, Some(&Ty::Str));
                    self.mismatch(h.span, &Ty::Str, &h.ty);
                    h
                }
                BParam::ConstStr => match self.const_expr(a, Some(&Ty::Str)) {
                    Some((ConstVal::Str(s), _)) => Expr { kind: ExprKind::Str(s), ty: Ty::Str, span: a.span },
                    Some((_, t)) => {
                        self.mismatch(a.span, &Ty::Str, &t);
                        Expr::error(a.span)
                    }
                    None => Expr::error(a.span),
                },
                BParam::Int => {
                    let h = self.expr(a, Some(&Ty::Int));
                    self.mismatch(h.span, &Ty::Int, &h.ty);
                    h
                }
                BParam::ConstInt => match self.const_expr(a, Some(&Ty::Int)) {
                    Some((ConstVal::Int(n), _)) => Expr { kind: ExprKind::Int(n), ty: Ty::Int, span: a.span },
                    Some((_, t)) => {
                        self.mismatch(a.span, &Ty::Int, &t);
                        Expr::error(a.span)
                    }
                    None => Expr::error(a.span),
                },
            };
            ok &= !h.ty.is_error();
            hargs.push(h);
        }
        if ok {
            self.builtin_semantics(b.id, &hargs);
        }
        Expr { kind: ExprKind::Builtin { name: b.id, args: hargs }, ty: ret, span }
    }

    /// Extra checks on constant arguments (valid ids, ranges).
    fn builtin_semantics(&mut self, name: &str, args: &[Expr]) {
        let s = |i: usize| match args.get(i).map(|a| &a.kind) {
            Some(ExprKind::Str(s)) => Some((s.clone(), args[i].span)),
            _ => None,
        };
        let n = |i: usize| match args.get(i).map(|a| &a.kind) {
            Some(ExprKind::Int(n)) => Some((*n, args[i].span)),
            _ => None,
        };
        match name {
            "say" => {
                if let Some((m, sp)) = s(0)
                    && (m.contains('\n') || m.contains('\r') || m.is_empty()) {
                        self.error(sp, "a `say` message must be one non-empty line");
                    }
            }
            "tag.add" | "tag.remove" | "tag.has" => {
                if let Some((t, sp)) = s(1)
                    && Word::new(&t).is_err() {
                        self.error_hint(sp, format!("invalid tag name {t:?}"), "tags may only contain letters, digits and _ - . +");
                    }
            }
            "effect.give" | "summon" | "setblock" => {
                let idx = if name == "effect.give" { 1 } else if name == "summon" { 0 } else { 3 };
                if let Some((id, sp)) = s(idx)
                    && let Err(e) = ResourceLocation::parse(&id) {
                        self.error(sp, format!("invalid id: {}", e.message()));
                    }
                if name == "effect.give" {
                    // the duration is in ticks (so `10s` means ten seconds); the game takes whole seconds
                    if let Some((ticks, sp)) = n(2)
                        && !(1..=20_000_000).contains(&ticks) {
                            self.error(sp, "the duration must be between 1 and 20000000 ticks");
                        }
                    if let Some((amp, sp)) = n(3)
                        && !(0..=255).contains(&amp) {
                            self.error(sp, "the amplifier must be between 0 and 255");
                        }
                }
            }
            _ => {}
        }
    }

    fn method_call(&mut self, base: &'a ast::Expr, name: &'a ast::Ident, args: &'a [ast::Expr], span: Span) -> Expr {
        let recv = self.expr(base, None);
        if recv.ty.is_error() {
            self.args_for_errors(args);
            return Expr::error(span);
        }
        let arity = |this: &mut Self, want: usize| -> bool {
            if args.len() == want {
                true
            } else {
                let takes = format!("{want} argument{}", if want == 1 { "" } else { "s" });
                this.arity_error(&format!("method `{}`", name.name), takes, args.len(), span);
                false
            }
        };
        match (&recv.ty, name.name.as_str()) {
            (Ty::List(elem), "push") => {
                let elem = (**elem).clone();
                if !arity(self, 1) {
                    self.args_for_errors(args);
                    return Expr::error(span);
                }
                self.check_place(&recv);
                let v = self.expr(&args[0], Some(&elem));
                self.mismatch(v.span, &elem, &v.ty);
                Expr { kind: ExprKind::Method { kind: MethodKind::ListPush, recv: Box::new(recv), args: vec![v] }, ty: Ty::Unit, span }
            }
            (Ty::List(_), "len") | (Ty::Str, "len") => {
                if !arity(self, 0) {
                    return Expr::error(span);
                }
                let kind = if matches!(recv.ty, Ty::Str) { MethodKind::StrLen } else { MethodKind::ListLen };
                Expr { kind: ExprKind::Method { kind, recv: Box::new(recv), args: Vec::new() }, ty: Ty::Int, span }
            }
            (other, m) => {
                let msg = format!("`{}` has no method `{m}`", self.tn(other));
                self.error(name.span, msg);
                self.args_for_errors(args);
                Expr::error(span)
            }
        }
    }
}

fn op_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn assign_str(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Assign => "=",
        AssignOp::Add => "+=",
        AssignOp::Sub => "-=",
        AssignOp::Mul => "*=",
        AssignOp::Div => "/=",
        AssignOp::Rem => "%=",
    }
}
