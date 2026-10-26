//! Name resolution, type checking and semantic analysis: syntax trees -> `hir::Hir`.
//! Rules are documented in docs/lang/semantics.md.

use std::collections::{HashMap, HashSet};

use super::builtins::{self, BParam, BRet, Intrinsic};
use super::hir::*;
use super::matching;
use super::selector;
use super::ty::{EnumId, StructId, Ty};
use crate::datapack::{Compound, HolderName, Nbt, ResourceLocation, Word};
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
    /// A module of the embedded standard library (`src/stdlib`): the only place `#[intrinsic]`
    /// and `#[lang]` are allowed.
    pub std: bool,
}

/// File indices of the embedded standard library when it is added by [`check`] or
/// [`std_inputs`] (a real project gets real indices from its source map).
pub const STD_FILE_BASE: usize = 1_000_000;

/// The embedded standard library as checker inputs, numbering the files from `first_file`.
pub fn std_inputs(first_file: usize) -> Vec<ModuleInput<'static>> {
    crate::stdlib::parsed().iter().enumerate().map(|(i, (m, program))| ModuleInput { name: m.name, file: first_file + i, program, std: true }).collect()
}

/// Checks a single-file program (module `main`) together with the standard library.
pub fn check(program: &ast::Program) -> Result<Checked, Report> {
    let mut inputs = vec![ModuleInput { name: "main", file: 0, program, std: false }];
    inputs.extend(std_inputs(STD_FILE_BASE));
    check_modules(&inputs)
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
    /// A function the compiler implements: an index into `Checker::intrinsics`.
    Builtin(usize),
    /// A variant of an enum, usable as a value or a constructor (`None`, `Some(x)`): only the
    /// variants of enums in the standard library are names of their own; the others are written
    /// `Enum::Variant`.
    Variant(EnumId, usize),
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
    /// Types the compiler provides (`#[lang("entity")]`): own ones, and imported ones
    lang_types: HashMap<String, (Ty, bool)>,
    imported_lang_types: HashMap<String, Ty>,
    /// Part of the embedded standard library.
    std: bool,
}

struct StructInfo<'a> {
    name: String,
    module: usize,
    decl: &'a ast::StructDecl,
    fields: Vec<(String, Ty)>,
    /// `#[decimals(N)]` (standard library): a fixed-point number `{ whole, frac }` that decimal
    /// literals with up to N digits can build, and whose operators call its methods.
    decimals: Option<u32>,
}

/// One arm as the checker sees it, whether it comes from a `match` or an `if let`.
struct ArmIn<'a> {
    /// `None`: matches anything (the `else` of an `if let`).
    pat: Option<&'a ast::Pattern>,
    guard: Option<&'a ast::Expr>,
    body: ArmSrc<'a>,
    span: Span,
}

enum ArmSrc<'a> {
    Block(&'a ast::Block),
    Expr(&'a ast::Expr),
    /// `else if ..` of an `if let`
    ElseIf(&'a ast::Stmt),
    /// no `else`
    Empty,
}

struct EnumInfo<'a> {
    name: String,
    module: usize,
    decl: &'a ast::EnumDecl,
    generics: Vec<String>,
    /// Variant names and payload types (which may mention `Ty::Param`), in declaration order.
    variants: Vec<(String, Vec<Ty>)>,
}

struct FnSig<'a> {
    decl: &'a ast::FnDecl,
    attrs: &'a [ast::Attr],
    module: usize,
    params: Vec<Ty>,
    ret: Ty,
    entry: Option<EntryKind>,
    span: Span,
    /// For a method: the type it belongs to (what `Self` means in its signature and body).
    self_ty: Option<Ty>,
    /// For a method: its name qualified by the type, `Point::sum`.
    qual: Option<String>,
}

/// What a method can be attached to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TypeKey {
    Struct(StructId),
    Enum(EnumId),
    /// The entity handle (`game::Entity`).
    Entity,
}

/// An `impl` block.
struct ImplInfo<'a> {
    module: usize,
    decl: &'a ast::ImplDecl,
    /// The type the block is for, with the block's own type parameters as `Ty::Param`.
    self_ty: Ty,
    methods: Vec<usize>,
}

enum MethodBody {
    /// A function written in slab, checked like any other.
    User(FuncId),
    /// Implemented by the compiler, matched by id in code generation.
    Intrinsic(&'static str),
}

struct MethodInfo<'a> {
    name: String,
    module: usize,
    public: bool,
    impl_idx: usize,
    decl: &'a ast::FnDecl,
    takes_self: bool,
    body: MethodBody,
    /// Intrinsics only: the parameter types after `self` and the return type, which may mention
    /// the block's type parameters.
    params: Vec<Ty>,
    ret: Ty,
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
    enums: Vec<EnumInfo<'a>>,
    enum_names: Vec<String>,
    /// The type parameters in scope while the declaration of a generic enum is resolved.
    generic_scope: Vec<String>,
    /// Names every module sees without importing them (`Option`, `Some`, ...), from `std`.
    prelude_values: HashMap<String, ValueSym>,
    /// Checking the call of a `start` statement (the only place a task may be called).
    starting: bool,
    prelude_types: HashMap<String, Ty>,
    impls: Vec<ImplInfo<'a>>,
    methods: Vec<MethodInfo<'a>>,
    method_index: HashMap<(TypeKey, String), usize>,
    /// What `Self` means while an `impl` item is being checked.
    self_ty: Option<Ty>,
    consts: Vec<ConstInfo<'a>>,
    globals: Vec<GlobalDef>,
    global_decls: Vec<(&'a ast::VarDecl, usize)>,
    objectives: Vec<ObjectiveDef>,
    fns: Vec<FnSig<'a>>,
    hir_fns: Vec<FnDef>,
    /// The functions the compiler implements (`#[intrinsic]` declarations of the std modules).
    intrinsics: Vec<Intrinsic>,
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
                lang_types: HashMap::new(),
                imported_lang_types: HashMap::new(),
                std: m.std,
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
            enums: Vec::new(),
            enum_names: Vec::new(),
            generic_scope: Vec::new(),
            prelude_values: HashMap::new(),
            starting: false,
            prelude_types: HashMap::new(),
            impls: Vec::new(),
            methods: Vec::new(),
            method_index: HashMap::new(),
            self_ty: None,
            consts: Vec::new(),
            globals: Vec::new(),
            global_decls: Vec::new(),
            objectives: Vec::new(),
            fns: Vec::new(),
            hir_fns: Vec::new(),
            intrinsics: Vec::new(),
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
        t.display(&self.struct_names, &self.enum_names).to_string()
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
            enums: self
                .enums
                .iter()
                .map(|e| EnumDef { name: e.name.clone(), module: e.module, generics: e.generics.clone(), variants: e.variants.iter().map(|(n, f)| VariantDef { name: n.clone(), fields: f.clone() }).collect() })
                .collect(),
            globals: self.globals.clone(),
            objectives: self.objectives.clone(),
            fns: self.hir_fns.clone(),
        }
    }

    // ------------------------------------------------------------ name lookup

    fn lookup_value(&self, name: &str) -> Option<ValueSym> {
        let m = &self.mods[self.cur];
        m.values.get(name).map(|v| v.0).or_else(|| m.imported_values.get(name).copied()).or_else(|| self.prelude_values.get(name).copied())
    }

    fn lookup_struct(&self, name: &str) -> Option<StructId> {
        let m = &self.mods[self.cur];
        m.types.get(name).map(|v| v.0).or_else(|| m.imported_types.get(name).copied())
    }

    /// The module a path names: an imported module (or alias), a path whose first segment is
    /// one, or the full path of a standard-library module (`game::tag`).
    fn find_module(&self, name: &str) -> Option<usize> {
        let m = &self.mods[self.cur];
        if let Some(&u) = m.modules.get(name) {
            return Some(u);
        }
        if let Some((first, rest)) = name.split_once("::")
            && let Some(&base) = m.modules.get(first)
            && let Some(&i) = self.by_name.get(&format!("{}::{rest}", self.mods[base].name))
        {
            return Some(i);
        }
        self.by_name.get(name).copied().filter(|&i| self.mods[i].std)
    }

    /// [`Self::find_module`], reporting a missing module.
    fn module_ref(&mut self, alias: &ast::Ident) -> Option<usize> {
        let found = self.find_module(&alias.name);
        if found.is_none() {
            self.error_hint(alias.span, format!("cannot find module `{}`", alias.name), format!("import it first: `import {};`", alias.name));
        }
        found
    }

    /// A hint for a bare name that is a standard-library function which has not been imported.
    fn builtin_hint(&self, name: &str) -> Option<String> {
        let b = self.intrinsics.iter().find(|b| b.name == name)?;
        Some(format!("call it as `{}(...)`, or import it: `import {}::{};`", b.path(), b.module, b.name))
    }

    /// `base` names a module and is not shadowed by a variable or item (`game.say(..)` is a
    /// mistake: module paths use `::`).
    fn module_dot_misuse(&mut self, base: &ast::Expr) -> Option<String> {
        let ast::ExprKind::Ident(m) = &base.kind else { return None };
        if self.lookup_value_or_local(m).is_some() {
            return None;
        }
        self.find_module(m).map(|_| m.clone())
    }

    fn private_item(&mut self, name: &ast::Ident, module: usize) {
        let msg = format!("`{}` is private to module `{}`", name.name, self.mods[module].name);
        self.error_hint(name.span, msg, "mark it `pub` to use it from other modules");
    }

    /// `module::name` as a value.
    fn qualified_value(&mut self, module: &ast::Ident, name: &ast::Ident) -> Option<ValueSym> {
        let m = self.module_ref(module)?;
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

    /// `module::Name` as a type: a struct, or a type the compiler provides.
    fn qualified_type(&mut self, module: &ast::Ident, name: &ast::Ident) -> Option<Ty> {
        let m = self.module_ref(module)?;
        if let Some((ty, public)) = self.mods[m].lang_types.get(&name.name).cloned() {
            if !public && m != self.cur {
                self.private_item(name, m);
                return None;
            }
            return Some(ty);
        }
        match self.mods[m].types.get(&name.name).copied() {
            Some((_, false)) if m != self.cur => {
                self.private_item(name, m);
                None
            }
            Some((id, _)) => Some(Ty::Struct(id)),
            None => {
                let msg = format!("module `{}` has no type `{}`", self.mods[m].name, name.name);
                self.error(name.span, msg);
                None
            }
        }
    }

    /// `module::Name` as a struct type.
    fn qualified_struct(&mut self, module: &ast::Ident, name: &ast::Ident) -> Option<StructId> {
        match self.qualified_type(module, name)? {
            Ty::Struct(id) => Some(id),
            _ => {
                self.error(name.span, format!("`{}::{}` cannot be built with a struct literal", module.name, name.name));
                None
            }
        }
    }

    fn find_struct(&mut self, module: Option<&ast::Ident>, name: &ast::Ident) -> Option<StructId> {
        match module {
            Some(m) => self.qualified_struct(m, name),
            None => match self.lookup_struct(&name.name) {
                Some(id) => Some(id),
                None => {
                    if name.name == "Self"
                        && let Some(Ty::Struct(id)) = &self.self_ty
                    {
                        return Some(*id);
                    }
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
        self.build_prelude();
        self.resolve_imports();
        self.resolve_enums();
        self.resolve_struct_fields();
        self.resolve_impls();
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
                    if let Some(attr) = item.attrs.iter().find(|a| a.name.name == "lang") {
                        self.register_lang_type(mi, s, attr, item.public);
                        continue;
                    }
                    if self.mods[mi].types.contains_key(&s.name.name) {
                        self.error(s.name.span, format!("struct `{}` is defined twice", s.name.name));
                        continue;
                    }
                    let id = StructId(self.structs.len());
                    self.mods[mi].types.insert(s.name.name.clone(), (id, item.public));
                    self.struct_names.push(s.name.name.clone());
                    let decimals = self.decimals_attr(mi, item);
                    self.structs.push(StructInfo { name: s.name.name.clone(), module: mi, decl: s, fields: Vec::new(), decimals });
                }
                ast::ItemKind::Enum(d) => {
                    if self.mods[mi].types.contains_key(&d.name.name) || self.mods[mi].lang_types.contains_key(&d.name.name) {
                        self.error(d.name.span, format!("`{}` is defined twice", d.name.name));
                        continue;
                    }
                    let id = EnumId(self.enums.len());
                    self.mods[mi].lang_types.insert(d.name.name.clone(), (Ty::Enum(id, Vec::new()), item.public));
                    self.enum_names.push(d.name.name.clone());
                    self.enums.push(EnumInfo { name: d.name.name.clone(), module: mi, decl: d, generics: d.generics.iter().map(|g| g.name.clone()).collect(), variants: Vec::new() });
                    if self.mods[mi].std {
                        // the standard library's variants (`Some`, `Ok`, ...) are names of their own
                        for (vi, v) in d.variants.iter().enumerate() {
                            self.declare_value(&v.name, ValueSym::Variant(id, vi), item.public);
                        }
                    }
                }
                ast::ItemKind::Impl(d) => self.register_impl(mi, d, item.span),
                ast::ItemKind::Fn(f) => {
                    if let Some(attr) = item.attrs.iter().find(|a| a.name.name == "intrinsic") {
                        self.register_intrinsic(mi, f, attr, item.public);
                        continue;
                    }
                    if !f.has_body {
                        self.error_hint(f.name.span, format!("function `{}` has no body", f.name.name), "write `{ ... }` after the signature");
                    }
                    let id = FuncId(self.fns.len());
                    // Registered even when the name is a duplicate so bodies still get checked.
                    self.declare_value(&f.name, ValueSym::Fn(id), item.public);
                    self.fns.push(FnSig { decl: f, attrs: &item.attrs, module: mi, params: Vec::new(), ret: Ty::Unit, entry: None, span: item.span, self_ty: None, qual: None });
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
            || m.lang_types.contains_key(name)
            || m.imported_lang_types.contains_key(name)
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
                let Some(&target) = self.by_name.get(&key) else {
                    // `import std::print;`: one item of a module, under its own name or an alias
                    if let Some((parent, last)) = key.rsplit_once("::")
                        && let Some(&pm) = self.by_name.get(parent)
                    {
                        let name = &imp.path[imp.path.len() - 1];
                        if imp.glob || imp.items.is_some() {
                            self.error(path_span, format!("`{key}` is not a module, so it has no items to import"));
                        } else if pm == mi {
                            self.error(path_span, "a module cannot import from itself");
                        } else {
                            let bound = imp.alias.as_ref().unwrap_or(name);
                            let _ = last;
                            self.import_item(mi, pm, name, &bound.name, bound.span);
                        }
                        continue;
                    }
                    self.error_hint(path_span, format!("cannot find module `{key}`"), format!("it would be the file `{}.slab` in the project", imp.path.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join("/")));
                    continue;
                };
                if target == mi {
                    self.error(path_span, "a module cannot import itself");
                    continue;
                }
                if imp.glob {
                    self.import_all(mi, target);
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
                            self.import_item(mi, target, n, &n.name, n.span);
                        }
                    }
                }
            }
        }
    }

    /// `import util::*;`: every public item and submodule of `target`, unless the name is
    /// already taken (an explicit import or a definition wins).
    fn import_all(&mut self, mi: usize, target: usize) {
        let values: Vec<(String, ValueSym)> = self.mods[target].values.iter().filter(|(_, (_, public))| *public).map(|(n, (s, _))| (n.clone(), *s)).collect();
        let types: Vec<(String, StructId)> = self.mods[target].types.iter().filter(|(_, (_, public))| *public).map(|(n, (s, _))| (n.clone(), *s)).collect();
        let lang: Vec<(String, Ty)> = self.mods[target].lang_types.iter().filter(|(_, (_, public))| *public).map(|(n, (t, _))| (n.clone(), t.clone())).collect();
        let prefix = format!("{}::", self.mods[target].name);
        let subs: Vec<(String, usize)> = self.by_name.iter().filter_map(|(n, &i)| n.strip_prefix(&prefix).filter(|r| !r.contains("::")).map(|r| (r.to_owned(), i))).collect();
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
        for (n, ty) in lang {
            if !self.name_taken(mi, &n) {
                self.mods[mi].imported_lang_types.insert(n, ty);
            }
        }
        for (n, i) in subs {
            if !self.name_taken(mi, &n) {
                self.mods[mi].modules.insert(n, i);
            }
        }
    }

    /// Brings the item `name` of module `target` into module `mi` as `bound`: a function,
    /// constant, global, objective, struct, compiler type, or a submodule.
    fn import_item(&mut self, mi: usize, target: usize, name: &ast::Ident, bound: &str, bound_span: Span) {
        let value = self.mods[target].values.get(&name.name).copied();
        let ty = self.mods[target].types.get(&name.name).copied();
        let lang = self.mods[target].lang_types.get(&name.name).cloned();
        let sub = self.by_name.get(&format!("{}::{}", self.mods[target].name, name.name)).copied();
        if value.is_none() && ty.is_none() && lang.is_none() && sub.is_none() {
            let msg = format!("module `{}` has no item `{}`", self.mods[target].name, name.name);
            self.error(name.span, msg);
            return;
        }
        if self.name_taken(mi, bound) {
            self.error(bound_span, format!("the name `{bound}` is already used in this module"));
            return;
        }
        let mut any = sub.is_some();
        if let Some((sym, public)) = value
            && public
        {
            self.mods[mi].imported_values.insert(bound.to_owned(), sym);
            any = true;
        }
        if let Some((id, public)) = ty
            && public
        {
            self.mods[mi].imported_types.insert(bound.to_owned(), id);
            any = true;
        }
        if let Some((t, public)) = lang
            && public
        {
            self.mods[mi].imported_lang_types.insert(bound.to_owned(), t);
            any = true;
        }
        if let Some(i) = sub {
            self.mods[mi].modules.insert(bound.to_owned(), i);
        }
        if !any {
            self.private_item(name, target);
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
        let lang_struct = matches!(item.kind, ast::ItemKind::Struct(_));
        for a in &item.attrs {
            if matches!(item.kind, ast::ItemKind::Fn(_)) || (lang_struct && matches!(a.name.name.as_str(), "lang" | "decimals")) {
                continue;
            }
            self.error(a.span, "attributes can only be applied to functions");
            return;
        }
    }

    /// `#[decimals(4)] pub struct NearDouble { whole: int, frac: int }` in the standard library.
    fn decimals_attr(&mut self, mi: usize, item: &ast::Item) -> Option<u32> {
        let attr = item.attrs.iter().find(|a| a.name.name == "decimals")?;
        if !self.mods[mi].std {
            self.error(attr.span, "`#[decimals]` is only for the standard library");
            return None;
        }
        match attr.args.first().map(|a| &a.kind) {
            Some(ast::ExprKind::Int(n)) if (1..=9).contains(n) => Some(*n as u32),
            _ => {
                self.error(attr.span, "`#[decimals]` needs the number of decimal digits, 1 to 9");
                None
            }
        }
    }

    /// The fixed-point struct an expected type stands for, with its digit count.
    fn decimal_type(&self, expected: Option<&Ty>) -> Option<(StructId, u32)> {
        let Some(Ty::Struct(id)) = expected else { return None };
        self.structs[id.0].decimals.map(|d| (*id, d))
    }

    /// `1.5` / `-` already folded: the whole part and the fraction scaled to `digits` digits.
    fn decimal_parts(&mut self, text: &str, digits: u32, name: &str, span: Span) -> Option<(i32, i32)> {
        let (whole, frac) = text.split_once('.').unwrap_or((text, ""));
        let clean = |t: &str| t.chars().filter(|c| *c != '_').collect::<String>();
        let (whole, frac) = (clean(whole), clean(frac));
        if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
            self.error_hint(span, format!("`{text}` is not a plain decimal number"), "write digits, a point and digits, like `1.25` (no exponent)");
            return None;
        }
        if frac.len() > digits as usize {
            self.error_hint(span, format!("`{name}` keeps {digits} decimal digits, `{text}` has {}", frac.len()), format!("write at most {digits} digits after the point, or use the type with more digits"));
            return None;
        }
        let Ok(w) = whole.parse::<i32>() else {
            self.error(span, format!("`{text}` is too large"));
            return None;
        };
        let scaled = format!("{frac:0<width$}", width = digits as usize);
        Some((w, scaled.parse::<i32>().unwrap_or(0)))
    }

    /// A decimal literal (or an integer) where a fixed-point number is expected.
    fn decimal_literal(&mut self, id: StructId, digits: u32, text: &str, span: Span) -> Expr {
        let name = self.structs[id.0].name.clone();
        match self.decimal_parts(text, digits, &name, span) {
            Some((w, f)) => Expr { kind: ExprKind::Struct { id, fields: vec![Expr { kind: ExprKind::Int(w), ty: Ty::Int, span }, Expr { kind: ExprKind::Int(f), ty: Ty::Int, span }] }, ty: Ty::Struct(id), span },
            None => Expr::error(span),
        }
    }

    /// `#[intrinsic("id", min_args)] fn f(..) -> T;` in the standard library: a function the
    /// compiler implements. The declaration is its signature.
    fn register_intrinsic(&mut self, mi: usize, f: &'a ast::FnDecl, attr: &'a ast::Attr, public: bool) {
        if !self.mods[mi].std {
            self.error(attr.span, "`#[intrinsic]` is only for the standard library");
            return;
        }
        if f.has_body {
            self.error(f.name.span, "an intrinsic has no body");
            return;
        }
        let id = match attr.args.first().map(|a| &a.kind) {
            Some(ast::ExprKind::Str(s)) => builtins::intrinsic_id(s),
            _ => None,
        };
        let Some(id) = id else {
            self.error_hint(attr.span, "`#[intrinsic]` needs the name of an intrinsic the compiler implements", "for example `#[intrinsic(\"tp\")]`");
            return;
        };
        let mut params = Vec::new();
        for p in &f.params {
            match BParam::from_type(&p.ty.name.name, p.ty.constant).filter(|_| p.ty.module.is_none() && p.ty.args.is_empty()) {
                Some(k) => params.push(k),
                None => {
                    self.error_hint(p.ty.span, format!("`{}` is not a type an intrinsic can take", p.ty.name.name), "use Target, Coord, Num, any, str, int, const str or const int");
                    return;
                }
            }
        }
        let ret = match f.ret.as_ref().map(|t| t.name.name.as_str()) {
            None => BRet::Unit,
            Some("bool") => BRet::Bool,
            Some("Entity") => BRet::Entity,
            Some("Result") => BRet::CommandResult,
            Some(other) => {
                self.error(f.ret.as_ref().map_or(f.name.span, |t| t.span), format!("an intrinsic cannot return `{other}`"));
                return;
            }
        };
        let min_args = match attr.args.get(1).map(|a| &a.kind) {
            Some(ast::ExprKind::Int(n)) if (0..=params.len() as i64).contains(n) => *n as usize,
            None => params.len(),
            _ => {
                self.error(attr.span, "the second argument of `#[intrinsic]` is the number of required parameters");
                return;
            }
        };
        let idx = self.intrinsics.len();
        self.intrinsics.push(Intrinsic { id, name: f.name.name.clone(), module: self.mods[mi].name.clone(), params, min_args, ret });
        self.declare_value(&f.name, ValueSym::Builtin(idx), public);
    }

    /// `#[lang("entity")] pub struct Entity {}` in the standard library: the name stands for a
    /// type the compiler provides.
    fn register_lang_type(&mut self, mi: usize, s: &'a ast::StructDecl, attr: &'a ast::Attr, public: bool) {
        if !self.mods[mi].std {
            self.error(attr.span, "`#[lang]` is only for the standard library");
            return;
        }
        let ty = match attr.args.first().map(|a| &a.kind) {
            Some(ast::ExprKind::Str(n)) if n == "entity" => Ty::Entity,
            _ => {
                self.error(attr.span, "`#[lang]` needs the name of a type the compiler provides (\"entity\")");
                return;
            }
        };
        if self.mods[mi].types.contains_key(&s.name.name) || self.mods[mi].lang_types.contains_key(&s.name.name) {
            self.error(s.name.span, format!("struct `{}` is defined twice", s.name.name));
            return;
        }
        self.mods[mi].lang_types.insert(s.name.name.clone(), (ty, public));
    }

    // ------------------------------------------------------------------ impl blocks

    /// Records an `impl` block and its methods; their types are resolved later, once every
    /// type name is known ([`Self::resolve_impls`]).
    fn register_impl(&mut self, mi: usize, d: &'a ast::ImplDecl, _span: Span) {
        let impl_idx = self.impls.len();
        self.impls.push(ImplInfo { module: mi, decl: d, self_ty: Ty::Error, methods: Vec::new() });
        for m in &d.methods {
            let f = &m.decl;
            let body = if let Some(attr) = m.attrs.iter().find(|a| a.name.name == "intrinsic") {
                if !self.mods[mi].std {
                    self.error(attr.span, "`#[intrinsic]` is only for the standard library");
                    continue;
                }
                if f.has_body {
                    self.error(f.name.span, "an intrinsic has no body");
                    continue;
                }
                let id = match attr.args.first().map(|a| &a.kind) {
                    Some(ast::ExprKind::Str(s)) => builtins::method_id(s),
                    _ => None,
                };
                let Some(id) = id else {
                    self.error(attr.span, "`#[intrinsic]` needs the name of a method the compiler implements");
                    continue;
                };
                MethodBody::Intrinsic(id)
            } else {
                if !f.has_body {
                    self.error_hint(f.name.span, format!("method `{}` has no body", f.name.name), "write `{ ... }` after the signature");
                    continue;
                }
                if !d.generics.is_empty() {
                    self.error_hint(f.name.span, format!("method `{}` is in a generic `impl`", f.name.name), "methods of generic types cannot be written in slab yet; only the standard library's built-in ones exist");
                    continue;
                }
                let fid = FuncId(self.fns.len());
                self.fns.push(FnSig { decl: f, attrs: &m.attrs, module: mi, params: Vec::new(), ret: Ty::Unit, entry: None, span: m.span, self_ty: Some(Ty::Error), qual: None });
                MethodBody::User(fid)
            };
            let idx = self.methods.len();
            self.methods.push(MethodInfo { name: f.name.name.clone(), module: mi, public: m.public, impl_idx, decl: f, takes_self: f.has_self, body, params: Vec::new(), ret: Ty::Unit });
            self.impls[impl_idx].methods.push(idx);
        }
    }

    /// The type each `impl` is for and the signatures of the methods; fills the method table.
    fn resolve_impls(&mut self) {
        for ii in 0..self.impls.len() {
            let (mi, d) = (self.impls[ii].module, self.impls[ii].decl);
            self.cur = mi;
            self.generic_scope = d.generics.iter().map(|g| g.name.clone()).collect();
            let mut seen = HashSet::new();
            for g in &d.generics {
                if !seen.insert(g.name.clone()) {
                    self.error(g.span, format!("type parameter `{}` is declared twice", g.name));
                }
            }
            let target = self.resolve_ty(&d.target);
            let (key, owner, type_name) = match &target {
                Ty::Struct(id) => (Some(TypeKey::Struct(*id)), Some(self.structs[id.0].module), self.struct_names[id.0].clone()),
                Ty::Enum(id, _) => (Some(TypeKey::Enum(*id)), Some(self.enums[id.0].module), self.enum_names[id.0].clone()),
                Ty::Entity => (Some(TypeKey::Entity), self.mods[mi].std.then_some(mi), "Entity".to_owned()),
                Ty::Error => (None, None, String::new()),
                other => {
                    let msg = format!("methods can only be added to structs, enums and `Entity`, not `{}`", self.tn(other));
                    self.error(d.target.span, msg);
                    (None, None, String::new())
                }
            };
            let key = match (key, owner) {
                (Some(k), Some(o)) if o == mi => Some(k),
                (Some(_), _) => {
                    self.error_hint(d.target.span, format!("an `impl` for `{type_name}` has to be in the module that declares it"), "move the `impl` next to the type");
                    None
                }
                (None, _) => None,
            };
            self.impls[ii].self_ty = target.clone();
            self.self_ty = Some(target.clone());
            for mx in self.impls[ii].methods.clone() {
                let name = self.methods[mx].name.clone();
                let f = self.methods[mx].decl;
                match self.methods[mx].body {
                    MethodBody::Intrinsic(_) => {
                        let mut params = Vec::new();
                        for p in f.params.iter().skip(f.has_self as usize) {
                            params.push(self.resolve_ty(&p.ty));
                        }
                        let ret = match &f.ret {
                            Some(t) => self.resolve_ty(t),
                            None => Ty::Unit,
                        };
                        self.methods[mx].params = params;
                        self.methods[mx].ret = ret;
                    }
                    MethodBody::User(fid) => {
                        self.fns[fid.0].self_ty = Some(target.clone());
                        self.fns[fid.0].qual = Some(format!("{type_name}::{name}"));
                    }
                }
                if let Some(key) = key {
                    if let TypeKey::Enum(e) = key
                        && self.enums[e.0].decl.variants.iter().any(|v| v.name.name == name)
                    {
                        self.error(f.name.span, format!("`{type_name}` already has a variant called `{name}`"));
                        continue;
                    }
                    if self.method_index.insert((key, name.clone()), mx).is_some() {
                        self.error(f.name.span, format!("`{type_name}` has two methods called `{name}`"));
                    }
                }
            }
            self.generic_scope.clear();
            self.self_ty = None;
        }
    }

    fn type_key(ty: &Ty) -> Option<TypeKey> {
        match ty {
            Ty::Struct(id) => Some(TypeKey::Struct(*id)),
            Ty::Enum(id, _) => Some(TypeKey::Enum(*id)),
            Ty::Entity => Some(TypeKey::Entity),
            _ => None,
        }
    }

    /// The method `Type::name` of a path such as `Point::new`, `Self::new` or `util::Point::new`
    /// (reports nothing when the prefix is not a type).
    fn assoc_method(&self, module: &ast::Ident, name: &ast::Ident) -> Option<usize> {
        let key = self.path_type_key(module)?;
        self.method_index.get(&(key, name.name.clone())).copied()
    }

    /// The type a path prefix such as `Point`, `Self` or `util::Point` names, if it names one.
    fn path_type_key(&self, module: &ast::Ident) -> Option<TypeKey> {
        let (prefix, type_name) = match module.name.rsplit_once("::") {
            Some((p, t)) => (Some(p), t),
            None => (None, module.name.as_str()),
        };
        match prefix {
            None if type_name == "Self" => Self::type_key(self.self_ty.as_ref()?),
            None => {
                let m = &self.mods[self.cur];
                let lang = m.lang_types.get(type_name).map(|(t, _)| t.clone()).or_else(|| m.imported_lang_types.get(type_name).cloned());
                match lang {
                    Some(t) => Self::type_key(&t),
                    None => match self.lookup_struct(type_name) {
                        Some(id) => Some(TypeKey::Struct(id)),
                        None => Self::type_key(self.prelude_types.get(type_name)?),
                    },
                }
            }
            Some(p) => {
                let mi = self.find_module(p)?;
                let m = &self.mods[mi];
                match m.lang_types.get(type_name) {
                    Some((t, _)) => Self::type_key(t),
                    None => m.types.get(type_name).map(|(id, _)| TypeKey::Struct(*id)),
                }
            }
        }
    }

    /// `Type::function(args)`, or `Type::method(receiver, args)`.
    fn call_assoc(&mut self, mx: usize, args: &'a [ast::Expr], callee: &'a ast::Expr, span: Span) -> Expr {
        if !self.methods[mx].public && self.methods[mx].module != self.cur {
            let name = ast::Ident { name: self.methods[mx].name.clone(), span: callee.span };
            let m = self.methods[mx].module;
            self.private_item(&name, m);
            self.args_for_errors(args);
            return Expr::error(span);
        }
        match self.methods[mx].body {
            MethodBody::User(fid) => self.user_call(fid, args, span),
            MethodBody::Intrinsic(id) => {
                if !self.methods[mx].takes_self || args.is_empty() {
                    self.error(callee.span, format!("`{}` is called on a value: `value.{}(..)`", self.methods[mx].name, self.methods[mx].name));
                    self.args_for_errors(args);
                    return Expr::error(span);
                }
                let recv = self.expr(&args[0], None);
                self.call_intrinsic_method(mx, id, recv, &args[1..], span)
            }
        }
    }

    /// `receiver.name(args)` for a method found in the table.
    fn call_method(&mut self, mx: usize, recv: Expr, args: &'a [ast::Expr], span: Span, name_span: Span) -> Expr {
        if !self.methods[mx].takes_self {
            let msg = format!("`{}` is an associated function, not a method", self.methods[mx].name);
            self.error_hint(name_span, msg, "call it on the type: `Type::name(..)`");
            self.args_for_errors(args);
            return Expr::error(span);
        }
        if !self.methods[mx].public && self.methods[mx].module != self.cur {
            let name = ast::Ident { name: self.methods[mx].name.clone(), span: name_span };
            let m = self.methods[mx].module;
            self.private_item(&name, m);
            self.args_for_errors(args);
            return Expr::error(span);
        }
        match self.methods[mx].body {
            MethodBody::User(fid) => {
                let params = self.fns[fid.0].params.clone();
                let ret = self.fns[fid.0].ret.clone();
                if args.len() + 1 != params.len() {
                    let takes = format!("{} argument{}", params.len() - 1, if params.len() == 2 { "" } else { "s" });
                    self.arity_error(&format!("method `{}`", self.methods[mx].name), takes, args.len(), span);
                    self.args_for_errors(args);
                    return Expr::error(span);
                }
                self.mismatch(recv.span, &params[0], &recv.ty);
                let mut hargs = vec![recv];
                for (a, ty) in args.iter().zip(&params[1..]) {
                    let h = self.expr(a, Some(ty));
                    self.mismatch(h.span, ty, &h.ty);
                    hargs.push(h);
                }
                Expr { kind: ExprKind::Call { func: fid, args: hargs }, ty: ret, span }
            }
            MethodBody::Intrinsic(id) => self.call_intrinsic_method(mx, id, recv, args, span),
        }
    }

    /// A method of the standard library the compiler implements (`opt.unwrap()`): the
    /// signature has the impl's type parameters, which the receiver's type binds.
    fn call_intrinsic_method(&mut self, mx: usize, id: &'static str, recv: Expr, args: &'a [ast::Expr], span: Span) -> Expr {
        let (pattern, n) = {
            let imp = &self.impls[self.methods[mx].impl_idx];
            (imp.self_ty.clone(), imp.decl.generics.len())
        };
        let params = self.methods[mx].params.clone();
        let ret = self.methods[mx].ret.clone();
        if recv.ty.is_error() {
            self.args_for_errors(args);
            return Expr::error(span);
        }
        let mut binds: Vec<Option<Ty>> = vec![None; n];
        if !Self::unify(&pattern, &recv.ty, &mut binds) {
            self.mismatch(recv.span, &pattern, &recv.ty);
            self.args_for_errors(args);
            return Expr::error(span);
        }
        if args.len() != params.len() {
            let takes = format!("{} argument{}", params.len(), if params.len() == 1 { "" } else { "s" });
            self.arity_error(&format!("method `{}`", self.methods[mx].name), takes, args.len(), span);
            self.args_for_errors(args);
            return Expr::error(span);
        }
        let mut hargs = Vec::new();
        for (a, pty) in args.iter().zip(&params) {
            let known: Option<Ty> = binds.iter().cloned().collect::<Option<Vec<Ty>>>().map(|all| pty.subst(&all));
            let h = self.expr(a, known.as_ref());
            if !h.ty.is_error() && !Self::unify(pty, &h.ty, &mut binds) {
                let partial: Vec<Ty> = binds.iter().map(|b| b.clone().unwrap_or(Ty::Error)).collect();
                self.mismatch(h.span, &pty.subst(&partial), &h.ty);
            }
            hargs.push(h);
        }
        let all: Vec<Ty> = binds.iter().map(|b| b.clone().unwrap_or(Ty::Error)).collect();
        Expr { kind: ExprKind::Method { kind: MethodKind::Intrinsic(id), recv: Box::new(recv), args: hargs }, ty: ret.subst(&all), span }
    }

    /// `Option`, `Result` and their variants are visible in every module (docs/lang/enums.md).
    fn build_prelude(&mut self) {
        let Some(&std) = self.by_name.get("std") else { return };
        for name in ["Option", "Result"] {
            if let Some((ty, true)) = self.mods[std].lang_types.get(name).cloned() {
                self.prelude_types.insert(name.to_owned(), ty);
            }
        }
        for name in ["Some", "None", "Ok", "Err"] {
            if let Some((sym, true)) = self.mods[std].values.get(name).copied() {
                self.prelude_values.insert(name.to_owned(), sym);
            }
        }
    }

    /// The payload types of every enum's variants, once all names are known.
    fn resolve_enums(&mut self) {
        for i in 0..self.enums.len() {
            self.cur = self.enums[i].module;
            let decl = self.enums[i].decl;
            self.generic_scope = self.enums[i].generics.clone();
            let mut seen = HashSet::new();
            for g in &decl.generics {
                if !seen.insert(g.name.clone()) {
                    self.error(g.span, format!("type parameter `{}` is declared twice", g.name));
                }
            }
            let mut seen = HashSet::new();
            let mut variants = Vec::new();
            for v in &decl.variants {
                if !seen.insert(v.name.name.clone()) {
                    self.error(v.name.span, format!("variant `{}` is declared twice in `{}`", v.name.name, decl.name.name));
                }
                let mut fields = Vec::new();
                for t in &v.fields {
                    let ty = self.resolve_ty(t);
                    if !(ty.is_storable() || ty == Ty::Unit) {
                        self.error(t.span, format!("a variant cannot hold `{}`", self.tn(&ty)));
                    }
                    fields.push(ty);
                }
                variants.push((v.name.name.clone(), fields));
            }
            self.generic_scope.clear();
            self.enums[i].variants = variants;
        }
    }

    /// A type found by name, with its type arguments applied: an enum takes exactly as many as it
    /// declares, anything else none.
    fn finish_type(&mut self, base: Ty, t: &ast::Type) -> Ty {
        match base {
            Ty::Enum(id, _) => {
                let n = self.enums[id.0].generics.len();
                if t.args.len() != n {
                    let plural = if n == 1 { "" } else { "s" };
                    let msg = format!("`{}` takes {n} type argument{plural} but {} {} supplied", self.enums[id.0].name, t.args.len(), if t.args.len() == 1 { "was" } else { "were" });
                    self.error(t.span, msg);
                    return Ty::Error;
                }
                let mut args = Vec::new();
                for a in &t.args {
                    let ty = self.resolve_ty(a);
                    if !(ty.is_storable() || ty == Ty::Unit || ty.is_error()) {
                        self.error(a.span, format!("`{}` cannot be a type argument", self.tn(&ty)));
                    }
                    args.push(ty);
                }
                Ty::Enum(id, args)
            }
            other => {
                if !t.args.is_empty() {
                    self.error(t.span, format!("type `{}` does not take type arguments", t.name.name));
                }
                other
            }
        }
    }

    /// A path like `Option::Some` or `util::Shape::Line`. `None`: the prefix does not name an
    /// enum, resolve it as a module path. `Some(None)`: it names one but the variant is missing
    /// or private (reported). `Some(Some(..))`: the enum and the variant's index.
    fn variant_path(&mut self, module: &ast::Ident, name: &ast::Ident) -> Option<Option<(EnumId, usize)>> {
        let (prefix, enum_name) = match module.name.rsplit_once("::") {
            Some((p, e)) => (Some(p), e),
            None => (None, module.name.as_str()),
        };
        let ty = match prefix {
            None if enum_name == "Self" => self.self_ty.clone()?,
            None => {
                let m = &self.mods[self.cur];
                m.lang_types.get(enum_name).map(|(t, _)| t.clone()).or_else(|| m.imported_lang_types.get(enum_name).cloned()).or_else(|| self.prelude_types.get(enum_name).cloned())?
            }
            Some(p) => {
                let mi = self.find_module(p)?;
                let (t, public) = self.mods[mi].lang_types.get(enum_name).cloned()?;
                if !public && mi != self.cur {
                    if matches!(t, Ty::Enum(..)) {
                        self.private_item(&ast::Ident { name: enum_name.to_owned(), span: module.span }, mi);
                        return Some(None);
                    }
                    return None;
                }
                t
            }
        };
        let Ty::Enum(id, _) = ty else { return None };
        match self.enums[id.0].decl.variants.iter().position(|v| v.name.name == name.name) {
            Some(vi) => Some(Some((id, vi))),
            None => {
                let msg = format!("enum `{}` has no variant `{}`", self.enums[id.0].name, name.name);
                self.error(name.span, msg);
                Some(None)
            }
        }
    }

    /// Binds the type parameters in `pattern` (a payload type of an enum) by matching it against
    /// the type of the value given for it. False when they cannot agree.
    fn unify(pattern: &Ty, actual: &Ty, binds: &mut [Option<Ty>]) -> bool {
        match (pattern, actual) {
            (_, Ty::Error) => true,
            (Ty::Param(i), a) => match &binds[*i] {
                Some(b) => b.compatible(a),
                None => {
                    binds[*i] = Some(a.clone());
                    true
                }
            },
            (Ty::List(p), Ty::List(a)) => Self::unify(p, a, binds),
            (Ty::Enum(pe, ps), Ty::Enum(ae, xs)) => pe == ae && ps.len() == xs.len() && ps.iter().zip(xs).all(|(p, a)| Self::unify(p, a, binds)),
            (p, a) => p.compatible(a),
        }
    }

    /// `Some(x)`, `None`, `Option::Some(x)`: builds the enum value. The type arguments come from
    /// the type the context expects (`let x: Option<int> = None;`) or from the payload values.
    fn construct(&mut self, eid: EnumId, vi: usize, args: &'a [ast::Expr], expected: Option<&Ty>, span: Span) -> Expr {
        let ename = self.enums[eid.0].name.clone();
        let (vname, fields) = self.enums[eid.0].variants.get(vi).cloned().unwrap_or_default();
        let n = self.enums[eid.0].generics.len();
        if args.len() != fields.len() {
            let msg = if fields.is_empty() {
                format!("variant `{ename}::{vname}` has no payload, so it is written without parentheses")
            } else {
                format!("variant `{ename}::{vname}` takes {} value{} but {} {} supplied", fields.len(), if fields.len() == 1 { "" } else { "s" }, args.len(), if args.len() == 1 { "was" } else { "were" })
            };
            self.error(span, msg);
            self.args_for_errors(args);
            return Expr::error(span);
        }
        let mut binds: Vec<Option<Ty>> = vec![None; n];
        if let Some(Ty::Enum(e2, targs)) = expected
            && *e2 == eid
            && targs.len() == n
        {
            binds = targs.iter().cloned().map(Some).collect();
        }
        let mut hargs = Vec::new();
        let mut poisoned = false;
        for (a, fty) in args.iter().zip(&fields) {
            // the payload type, when all its parameters are known, is the context for the value
            let known: Option<Ty> = {
                let all: Option<Vec<Ty>> = binds.iter().cloned().collect();
                all.map(|all| fty.subst(&all)).or_else(|| if matches!(fty, Ty::Param(_)) { None } else { Some(fty.clone()).filter(|t| !contains_param(t)) })
            };
            let h = self.expr(a, known.as_ref());
            if h.ty.is_error() {
                poisoned = true;
            } else if !(h.ty.is_storable() || h.ty == Ty::Unit) {
                self.error(h.span, format!("a value of type `{}` cannot be a payload", self.tn(&h.ty)));
                poisoned = true;
            } else if !Self::unify(fty, &h.ty, &mut binds) {
                let want = {
                    let partial: Vec<Ty> = binds.iter().map(|b| b.clone().unwrap_or(Ty::Error)).collect();
                    fty.subst(&partial)
                };
                self.mismatch(h.span, &want, &h.ty);
                poisoned = true;
            }
            hargs.push(h);
        }
        if poisoned {
            return Expr::error(span);
        }
        let Some(targs) = binds.iter().cloned().collect::<Option<Vec<Ty>>>() else {
            let missing = binds.iter().position(Option::is_none).map_or("?", |i| self.enums[eid.0].generics[i].as_str()).to_owned();
            let hint = match ename.as_str() {
                "Option" => "annotate it: `let x: Option<int> = None;`".to_owned(),
                _ => format!("annotate the type, as in `let x: {ename}<...> = ...;`"),
            };
            self.error_hint(span, format!("cannot infer the type parameter `{missing}` of `{ename}` for `{vname}`"), hint);
            return Expr::error(span);
        };
        Expr { kind: ExprKind::Variant { enum_id: eid, index: vi, args: hargs }, ty: Ty::Enum(eid, targs), span }
    }

    /// [`Self::construct`] for constants: `const X: Option<int> = Some(3);`.
    fn const_variant(&mut self, eid: EnumId, vi: usize, args: &'a [ast::Expr], expected: Option<&Ty>, span: Span) -> Option<(ConstVal, Ty)> {
        let ename = self.enums[eid.0].name.clone();
        let (vname, fields) = self.enums[eid.0].variants.get(vi).cloned().unwrap_or_default();
        let n = self.enums[eid.0].generics.len();
        if args.len() != fields.len() {
            self.error(span, format!("variant `{ename}::{vname}` takes {} value(s) but {} were supplied", fields.len(), args.len()));
            return None;
        }
        let mut binds: Vec<Option<Ty>> = vec![None; n];
        if let Some(Ty::Enum(e2, targs)) = expected
            && *e2 == eid
            && targs.len() == n
        {
            binds = targs.iter().cloned().map(Some).collect();
        }
        let mut vals = Vec::new();
        for (a, fty) in args.iter().zip(&fields) {
            let known: Option<Ty> = binds.iter().cloned().collect::<Option<Vec<Ty>>>().map(|all| fty.subst(&all));
            let (v, t) = self.const_expr(a, known.as_ref())?;
            if !Self::unify(fty, &t, &mut binds) {
                let partial: Vec<Ty> = binds.iter().map(|b| b.clone().unwrap_or(Ty::Error)).collect();
                self.mismatch(a.span, &fty.subst(&partial), &t);
                return None;
            }
            vals.push(v);
        }
        let Some(targs) = binds.into_iter().collect::<Option<Vec<Ty>>>() else {
            self.error_hint(span, format!("cannot infer the type arguments of `{ename}` for `{vname}`"), "give the constant a type: `const X: Option<int> = None;`");
            return None;
        };
        Some((ConstVal::Enum(eid, vi, vals), Ty::Enum(eid, targs)))
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
            return match self.qualified_type(m, &t.name) {
                Some(ty) => self.finish_type(ty, t),
                None => Ty::Error,
            };
        }
        if name == "()" {
            return Ty::Unit;
        }
        if name == "Self" && t.module.is_none() {
            return match self.self_ty.clone() {
                Some(ty) => no_args(self, ty),
                None => {
                    self.error(t.span, "`Self` is only available inside an `impl` block");
                    Ty::Error
                }
            };
        }
        if let Some(i) = self.generic_scope.iter().position(|g| g == name) {
            return no_args(self, Ty::Param(i));
        }
        if let Some(ty) = self.mods[self.cur].imported_lang_types.get(name).cloned().or_else(|| self.mods[self.cur].lang_types.get(name).map(|(t, _)| t.clone())) {
            return self.finish_type(ty, t);
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
                None => match self.prelude_types.get(name).cloned() {
                    Some(ty) => self.finish_type(ty, t),
                    None => {
                        self.error_hint(t.span, format!("unknown type `{name}`"), "types are int, bool, str, list<T>, Option<T>, Result<T, E>, game::Entity, and your structs and enums");
                        Ty::Error
                    }
                },
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
            self.self_ty = self.fns[fi].self_ty.clone();
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
            let entry = if self.fns[fi].self_ty.is_some() {
                if let Some(a) = attrs.first() {
                    self.error(a.span, "a method cannot be an entry point or take attributes");
                }
                None
            } else {
                self.entry_of(attrs, f, !params.is_empty(), &ret)
            };
            if f.is_task {
                if self.fns[fi].self_ty.is_some() {
                    self.error(f.name.span, "a method cannot be a task");
                }
                if ret != Ty::Unit {
                    self.error(f.name.span, format!("task `{}` returns nothing: it may run over many ticks, so there is nobody to wait for a value", f.name.name));
                }
                if entry.is_some() {
                    self.error_hint(f.name.span, format!("task `{}` cannot be an entry point", f.name.name), "start it from a `#[load]` or `#[tick]` function: `start name(..);`");
                }
            }
            self.self_ty = None;
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
            K::Float(text) => match self.decimal_type(expected) {
                Some((id, digits)) => {
                    let name = self.structs[id.0].name.clone();
                    let (w, f) = self.decimal_parts(text, digits, &name, e.span)?;
                    Some((ConstVal::Struct(id, vec![ConstVal::Int(w), ConstVal::Int(f)]), Ty::Struct(id)))
                }
                None => {
                    self.error_hint(e.span, "a number with a decimal point needs a type", "write it where a fixed-point type is expected: `global x: NearDouble = 1.5;`");
                    None
                }
            },
            K::Unit => Some((ConstVal::Unit, Ty::Unit)),
            K::Error => None,
            K::Ident(name) => {
                let sym = self.lookup_value(name);
                if let Some(ValueSym::Variant(eid, vi)) = sym {
                    return self.const_variant(eid, vi, &[], expected, e.span);
                }
                self.const_of_sym(sym, name, e.span)
            }
            K::Path { module, name } => {
                if let Some(found) = self.variant_path(module, name) {
                    let (eid, vi) = found?;
                    return self.const_variant(eid, vi, &[], expected, e.span);
                }
                let sym = self.qualified_value(module, name)?;
                self.const_of_sym(Some(sym), &name.name, e.span)
            }
            K::Call { callee, args } => {
                let variant = match &callee.kind {
                    K::Ident(name) => match self.lookup_value(name) {
                        Some(ValueSym::Variant(eid, vi)) => Some((eid, vi)),
                        _ => None,
                    },
                    K::Path { module, name } => self.variant_path(module, name).map(|v| v.unwrap_or((EnumId(usize::MAX), 0))),
                    _ => None,
                };
                match variant {
                    Some((EnumId(usize::MAX), _)) => None,
                    Some((eid, vi)) => self.const_variant(eid, vi, args, expected, e.span),
                    None => {
                        self.error(e.span, "a function call is not a constant expression");
                        None
                    }
                }
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
            (Str(a), Str(b)) if op == BinOp::Eq => Some((Bool(a == b), Ty::Bool)),
            (Str(a), Str(b)) if op == BinOp::Ne => Some((Bool(a != b), Ty::Bool)),
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
            ConstVal::Unit => ExprKind::Unit,
            ConstVal::Enum(id, vi, vs) => {
                let targs = match ty {
                    Ty::Enum(_, a) => a.clone(),
                    _ => Vec::new(),
                };
                let tys: Vec<Ty> = self.enums[id.0].variants.get(*vi).map(|(_, f)| f.iter().map(|t| t.subst(&targs)).collect()).unwrap_or_default();
                ExprKind::Variant { enum_id: *id, index: *vi, args: vs.iter().zip(&tys).map(|(v, t)| self.const_to_expr(v, t, span)).collect() }
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
        self.self_ty = self.fns[idx].self_ty.clone();
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
        self.self_ty = None;
        self.hir_fns.push(FnDef {
            name: self.fns[idx].qual.clone().unwrap_or_else(|| decl.name.name.clone()),
            module: self.fns[idx].module,
            params,
            ret,
            locals: ctx.locals,
            body,
            entry: self.fns[idx].entry,
            span: self.fns[idx].span,
            std: self.mods[self.fns[idx].module].std,
            task: decl.is_task,
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
                // `a += b` on a fixed-point number is `a = a.add(b)`
                if *op != AssignOp::Assign
                    && let Ty::Struct(_) = &t.ty
                    && let Some(name) = match op {
                        AssignOp::Add => Some("add"),
                        AssignOp::Sub => Some("sub"),
                        AssignOp::Mul => Some("mul"),
                        AssignOp::Div => Some("div"),
                        AssignOp::Rem => Some("rem"),
                        AssignOp::Assign => None,
                    }
                    && let Some(key) = Self::type_key(&t.ty)
                    && let Some(&mx) = self.method_index.get(&(key, name.to_owned()))
                {
                    let call = self.call_method(mx, t.clone(), std::slice::from_ref(value), s.span, s.span);
                    return Stmt::Assign { target: t, op: AssignOp::Assign, value: call };
                }
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
            K::Start(call) => {
                let ast::ExprKind::Call { callee, args } = &call.kind else { return Stmt::Expr(Expr::error(call.span)) };
                self.starting = true;
                let e = self.call(callee, args, None, call.span);
                self.starting = false;
                match e.kind {
                    ExprKind::Call { func, args } if self.fns[func.0].decl.is_task => Stmt::Start { func, args },
                    ExprKind::Call { .. } => {
                        self.error_hint(call.span, "`start` launches a task, and this is an ordinary function", "call it normally, or declare it `task fn`");
                        Stmt::Expr(Expr::error(call.span))
                    }
                    ExprKind::Error => Stmt::Expr(Expr::error(call.span)),
                    _ => {
                        self.error(call.span, "`start` needs a call of a task: `start name(args);`");
                        Stmt::Expr(Expr::error(call.span))
                    }
                }
            }
            K::IfLet { pat, value, then, els } => {
                let scrut = self.match_scrutinee(value);
                let arms = vec![
                    ArmIn { pat: Some(pat), guard: None, body: ArmSrc::Block(then), span: pat.span },
                    ArmIn { pat: None, guard: None, body: match els.as_deref() {
                        None => ArmSrc::Empty,
                        Some(ast::Else::Block(b)) => ArmSrc::Block(b),
                        Some(ast::Else::If(st)) => ArmSrc::ElseIf(st),
                    }, span: s.span },
                ];
                Stmt::Expr(self.match_arms(scrut, arms, None, true, s.span))
            }
            K::Expr(e) if matches!(e.kind, ast::ExprKind::Match { .. }) => {
                let ast::ExprKind::Match { scrut, arms } = &e.kind else { unreachable!() };
                let scrut = self.match_scrutinee(scrut);
                let arms: Vec<ArmIn<'a>> = arms
                    .iter()
                    .map(|a| ArmIn { pat: Some(&a.pat), guard: a.guard.as_ref(), body: match &a.body {
                        ast::ArmBody::Block(b) => ArmSrc::Block(b),
                        ast::ArmBody::Expr(x) => ArmSrc::Expr(x),
                    }, span: a.span })
                    .collect();
                Stmt::Expr(self.match_arms(scrut, arms, None, true, e.span))
            }
            K::Expr(e) => {
                let hir = self.expr(e, None);
                if !matches!(hir.kind, ExprKind::Call { .. } | ExprKind::Builtin { .. } | ExprKind::Method { .. } | ExprKind::Error) {
                    self.warn(e.span, "this expression has no effect");
                }
                Stmt::Expr(hir)
            }
        }
    }

    // ------------------------------------------------------------------ match

    /// The value a `match` or `if let` looks at: any storable value (a selector that matches one
    /// entity becomes the entity).
    fn match_scrutinee(&mut self, e: &'a ast::Expr) -> Expr {
        let mut v = self.expr(e, None);
        if v.ty == Ty::Selector {
            v = self.coerce_to_entity(v);
        }
        if !v.ty.is_error() && !v.ty.is_storable() {
            let msg = format!("cannot match on a value of type `{}`", self.tn(&v.ty));
            self.error(e.span, msg);
            return Expr::error(e.span);
        }
        v
    }

    /// Checks the arms of a `match`/`if let`. `as_stmt`: the arms' values are dropped and each
    /// arm may have its own type; otherwise all arms produce one type (arms that return, break
    /// or continue fit any).
    fn match_arms(&mut self, scrut: Expr, arms: Vec<ArmIn<'a>>, expected: Option<&Ty>, as_stmt: bool, span: Span) -> Expr {
        let vtys = |c: &Self| -> Vec<Vec<(String, Vec<Ty>)>> { c.enums.iter().map(|e| e.variants.clone()).collect() };
        let table = vtys(self);
        let variants = |id: EnumId| table.get(id.0).cloned().unwrap_or_default();
        let mut out: Vec<MatchArm> = Vec::new();
        let mut result: Option<Ty> = expected.filter(|t| !t.is_error() && **t != Ty::Unit).cloned();
        let mut rows: Vec<Vec<Pat>> = Vec::new();
        for arm in arms {
            self.fctx().scopes.push(HashMap::new());
            let mut names = Vec::new();
            let pat = match arm.pat {
                Some(p) => self.pattern(p, &scrut.ty, &mut names),
                None => Pat::Wild,
            };
            let guard = arm.guard.map(|g| self.expect_bool(g));
            // an arm that can never run: the rows before it already cover everything it matches
            if !scrut.ty.is_error() && arm.pat.is_some() && !matching::useful(&rows, std::slice::from_ref(&pat), std::slice::from_ref(&scrut.ty), &variants) {
                self.warn(arm.pat.map_or(arm.span, |p| p.span), "unreachable pattern: an earlier arm already matches everything this one does");
            }
            let (body, value) = match arm.body {
                ArmSrc::Empty => (Block::default(), None),
                ArmSrc::Block(b) => {
                    let body = self.block(b);
                    if !as_stmt && !body.diverges() {
                        self.error_hint(b.span, "this arm gives no value", "a match used as a value needs an expression after `=>`, or a block that ends in `return`, `break` or `continue`");
                    }
                    (body, None)
                }
                ArmSrc::ElseIf(st) => (Block { stmts: vec![self.stmt(st)] }, None),
                ArmSrc::Expr(x) if as_stmt => {
                    let hir = self.expr(x, None);
                    if !matches!(hir.kind, ExprKind::Call { .. } | ExprKind::Builtin { .. } | ExprKind::Method { .. } | ExprKind::Match { .. } | ExprKind::Error) {
                        self.warn(x.span, "this expression has no effect");
                    }
                    (Block { stmts: vec![Stmt::Expr(hir)] }, None)
                }
                ArmSrc::Expr(x) => {
                    let v = self.expr(x, result.as_ref());
                    match &result {
                        None => result = Some(v.ty.clone()).filter(|t| !t.is_error()).or(result),
                        Some(t) => self.mismatch(v.span, &t.clone(), &v.ty),
                    }
                    (Block::default(), Some(v))
                }
            };
            self.fctx().scopes.pop();
            if guard.is_none() && !scrut.ty.is_error() {
                rows.push(vec![pat.clone()]);
            }
            out.push(MatchArm { pat, guard, body, value });
        }
        if !scrut.ty.is_error()
            && let Some(w) = matching::witness(&rows, std::slice::from_ref(&scrut.ty), &variants)
        {
            let what = w.first().cloned().unwrap_or_else(|| "_".to_owned());
            let hint = if what == "_" { "add an arm with `_ =>` to cover the rest".to_owned() } else { format!("add an arm for `{what}`, or `_ =>` to cover the rest") };
            self.error_hint(scrut.span, format!("this `match` does not cover `{what}`"), hint);
        }
        let ty = if as_stmt { Ty::Unit } else { result.unwrap_or(Ty::Unit) };
        Expr { kind: ExprKind::Match { scrut: Box::new(scrut), arms: out }, ty, span }
    }

    /// Checks a pattern against the type it must match, declaring its bindings in the current
    /// scope. `names` collects the binding names (a name may appear once per pattern).
    fn pattern(&mut self, p: &'a ast::Pattern, ty: &Ty, names: &mut Vec<String>) -> Pat {
        use ast::PatKind as P;
        if ty.is_error() {
            // still declare the names, so the arm body does not complain about unknown variables
            self.declare_pattern_names(p, names);
            return Pat::Wild;
        }
        match &p.kind {
            P::Wild => Pat::Wild,
            P::Int(n) => {
                if *ty != Ty::Int {
                    self.mismatch(p.span, ty, &Ty::Int);
                    return Pat::Wild;
                }
                if !fits_i32(*n) {
                    self.error(p.span, "integer literal is out of range for `int`");
                    return Pat::Wild;
                }
                Pat::Int(*n as i32)
            }
            P::Bool(b) => {
                if *ty != Ty::Bool {
                    self.mismatch(p.span, ty, &Ty::Bool);
                    return Pat::Wild;
                }
                Pat::Bool(*b)
            }
            P::Str(s) => {
                if *ty != Ty::Str {
                    self.mismatch(p.span, ty, &Ty::Str);
                    return Pat::Wild;
                }
                Pat::Str(s.clone())
            }
            P::Ident(id) => match self.lookup_value(&id.name) {
                Some(ValueSym::Variant(eid, vi)) => self.variant_pattern(eid, vi, &[], false, p.span, ty, names),
                _ => self.binding(id, ty, names),
            },
            P::Variant { module, name, fields, parens } => {
                let found = match module {
                    Some(m) => match self.variant_path(m, name) {
                        Some(found) => found,
                        None => {
                            self.error(p.span, format!("`{}::{}` is not an enum variant", m.name, name.name));
                            None
                        }
                    },
                    None => match self.lookup_value(&name.name) {
                        Some(ValueSym::Variant(eid, vi)) => Some((eid, vi)),
                        _ => {
                            self.error(name.span, format!("cannot find the variant `{}`", name.name));
                            None
                        }
                    },
                };
                match found {
                    Some((eid, vi)) => self.variant_pattern(eid, vi, fields, *parens, p.span, ty, names),
                    None => {
                        self.declare_pattern_names(p, names);
                        Pat::Wild
                    }
                }
            }
            P::Or(alts) => {
                let mut out = Vec::new();
                for a in alts {
                    let before = names.len();
                    out.push(self.pattern(a, ty, names));
                    if names.len() > before {
                        self.error(a.span, "a pattern with `|` cannot bind variables yet");
                    }
                }
                Pat::Or(out)
            }
        }
    }

    fn binding(&mut self, id: &'a ast::Ident, ty: &Ty, names: &mut Vec<String>) -> Pat {
        if names.contains(&id.name) {
            self.error(id.span, format!("`{}` is bound twice in this pattern", id.name));
        }
        names.push(id.name.clone());
        Pat::Bind(self.declare_local(id, ty.clone(), false, false))
    }

    /// Declares the names of a pattern that could not be checked.
    fn declare_pattern_names(&mut self, p: &'a ast::Pattern, names: &mut Vec<String>) {
        match &p.kind {
            ast::PatKind::Ident(id) if !matches!(self.lookup_value(&id.name), Some(ValueSym::Variant(..))) => {
                self.binding(id, &Ty::Error, names);
            }
            ast::PatKind::Variant { fields, .. } => fields.iter().for_each(|f| self.declare_pattern_names(f, names)),
            ast::PatKind::Or(alts) => alts.iter().for_each(|f| self.declare_pattern_names(f, names)),
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn variant_pattern(&mut self, eid: EnumId, vi: usize, fields: &'a [ast::Pattern], parens: bool, span: Span, ty: &Ty, names: &mut Vec<String>) -> Pat {
        let ename = self.enums[eid.0].name.clone();
        let (vname, payload) = self.enums[eid.0].variants.get(vi).cloned().unwrap_or_default();
        let targs = match ty {
            Ty::Enum(e2, targs) if *e2 == eid => targs.clone(),
            _ => {
                let msg = format!("this pattern is a variant of `{ename}` but the value is `{}`", self.tn(ty));
                self.error(span, msg);
                fields.iter().for_each(|f| self.declare_pattern_names(f, names));
                return Pat::Wild;
            }
        };
        if payload.len() != fields.len() {
            let msg = if payload.is_empty() {
                format!("variant `{ename}::{vname}` has no payload, so it is written without parentheses")
            } else if !parens {
                format!("variant `{ename}::{vname}` has a payload: write `{vname}({})`", vec!["_"; payload.len()].join(", "))
            } else {
                format!("variant `{ename}::{vname}` has {} value{} but the pattern has {}", payload.len(), if payload.len() == 1 { "" } else { "s" }, fields.len())
            };
            self.error(span, msg);
            fields.iter().for_each(|f| self.declare_pattern_names(f, names));
            return Pat::Wild;
        }
        let subs = fields.iter().zip(&payload).map(|(f, t)| self.pattern(f, &t.subst(&targs), names)).collect();
        Pat::Variant { index: vi, fields: subs }
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
            if let ast::CtxKind::Be(e) = &c.kind {
                // `be e` is `as e at @s`: after `as`, `@s` is the entity, and `e` is evaluated once
                let who = self.selector_operand(e);
                let here = Expr { kind: ExprKind::Selector(crate::datapack::Selector::new(crate::datapack::SelectorVar::S), Vec::new()), ty: Ty::Selector, span: e.span };
                out.push(CtxClause::As(who));
                out.push(CtxClause::At(here));
            } else if let Some(h) = self.ctx_clause(c) {
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
            K::Be(_) => unreachable!("`be` is expanded by context_stmt"),
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
            K::RotatedAs(e) => CtxClause::RotatedAs(self.selector_operand(e)),
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
            ExprKind::Selector(sel, _) if sel.is_single() => {
                let span = h.span;
                Expr { kind: ExprKind::ToEntity(Box::new(h)), ty: Ty::Entity, span }
            }
            ExprKind::Selector(sel, _) => {
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
        if let K::Int(n) = &e.kind
            && let Some((id, digits)) = self.decimal_type(expected)
        {
            // `x + 1` where `x` is a fixed-point number
            return self.decimal_literal(id, digits, &n.to_string(), span);
        }
        match &e.kind {
            K::Match { scrut, arms } => {
                let scrut = self.match_scrutinee(scrut);
                let arms: Vec<ArmIn<'a>> = arms
                    .iter()
                    .map(|a| ArmIn { pat: Some(&a.pat), guard: a.guard.as_ref(), body: match &a.body {
                        ast::ArmBody::Block(b) => ArmSrc::Block(b),
                        ast::ArmBody::Expr(x) => ArmSrc::Expr(x),
                    }, span: a.span })
                    .collect();
                self.match_arms(scrut, arms, expected, false, span)
            }
            K::Int(n) => {
                if !fits_i32(*n) {
                    self.error(span, "integer literal is out of range for `int` (-2147483648 to 2147483647)");
                    return Expr::error(span);
                }
                mk(ExprKind::Int(*n as i32), Ty::Int)
            }
            K::Float(text) => match self.decimal_type(expected) {
                Some((id, digits)) => self.decimal_literal(id, digits, text, span),
                None => {
                    self.error_hint(span, "a number with a decimal point needs a type", "write it where a fixed-point type is expected: `let x: NearDouble = 1.5;` (`NearDouble` and `FarDouble` are in std)");
                    Expr::error(span)
                }
            },
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
            K::Unit => mk(ExprKind::Unit, Ty::Unit),
            // already reported by the parser
            K::Error => Expr::error(span),
            K::Ident(name) => self.ident(name, span, expected),
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
            K::Path { module, name } => match self.variant_path(module, name) {
                Some(Some((eid, vi))) => self.variant_value(eid, vi, expected, span),
                Some(None) => Expr::error(span),
                None => match self.qualified_value(module, name) {
                    Some(ValueSym::Variant(eid, vi)) => self.variant_value(eid, vi, expected, span),
                    Some(sym) => self.sym_expr(Some(sym), &name.name, span),
                    None => Expr::error(span),
                },
            },
            K::Selector(lit) => match selector::convert(lit) {
                Ok(c) => {
                    let mut holes = Vec::new();
                    for slot in &c.holes {
                        let e = self.expr(&lit.args[slot.arg].holes[slot.index], Some(&slot.ty));
                        self.mismatch(e.span, &slot.ty, &e.ty);
                        holes.push(e);
                    }
                    mk(ExprKind::Selector(c.selector, holes), Ty::Selector)
                }
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
            K::Unary { op, expr } => self.unary(*op, expr, expected, span),
            K::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, span),
            K::Range { .. } => {
                self.error_hint(span, "a range is only allowed as the subject of a `for` loop", "write `for i in 0..n { ... }`");
                Expr::error(span)
            }
            K::Call { callee, args } => self.call(callee, args, expected, span),
            K::Field { base, name } => self.field(base, name, span),
            K::Index { base, index } => self.index(base, index, span),
        }
    }

    fn ident(&mut self, name: &str, span: Span, expected: Option<&Ty>) -> Expr {
        if let Some(id) = self.lookup_local(name) {
            let ty = self.fctx().locals[id.0].ty.clone();
            return Expr { kind: ExprKind::Local(id), ty, span };
        }
        let sym = self.lookup_value(name);
        if let Some(ValueSym::Variant(eid, vi)) = sym {
            return self.variant_value(eid, vi, expected, span);
        }
        self.sym_expr(sym, name, span)
    }

    /// A variant written without arguments: `None`. One with a payload needs them.
    fn variant_value(&mut self, eid: EnumId, vi: usize, expected: Option<&Ty>, span: Span) -> Expr {
        let (vname, fields) = self.enums[eid.0].variants.get(vi).cloned().unwrap_or_default();
        if !fields.is_empty() {
            self.error_hint(span, format!("variant `{vname}` carries a value and is used without one"), format!("write `{vname}(...)`"));
            return Expr::error(span);
        }
        self.construct(eid, vi, &[], expected, span)
    }

    /// The entity the body of a `for e in @a { }` runs as, as a handle: an `entity`-typed
    /// read of the id score of the current `@s` (`docs/lang/entities.md`).
    fn current_entity(&mut self, span: Span) -> Expr {
        let sel = Expr { kind: ExprKind::Selector(crate::datapack::Selector::new(crate::datapack::SelectorVar::S), Vec::new()), ty: Ty::Selector, span };
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
            Some(ValueSym::Variant(..)) => unreachable!("variants are handled before sym_expr"),
            Some(ValueSym::Objective(_)) => {
                self.error_hint(span, format!("objective `{name}` needs an index"), format!("read or write a score with `{name}[@s]`"));
                Expr::error(span)
            }
            None => {
                if self.find_module(name).is_some() {
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

    fn unary(&mut self, op: UnOp, inner: &'a ast::Expr, expected: Option<&Ty>, span: Span) -> Expr {
        match op {
            UnOp::Neg => {
                // a fixed-point number is negated by its `neg` method
                if self.decimal_type(expected).is_some() {
                    let e = self.expr(inner, expected);
                    if let Some(key) = Self::type_key(&e.ty)
                        && let Some(&mx) = self.method_index.get(&(key, "neg".to_owned()))
                    {
                        return self.call_method(mx, e, &[], span, span);
                    }
                    self.operand(&e, &Ty::Int, "-");
                    return Expr::error(span);
                }
                // `-2147483648`
                if let ast::ExprKind::Int(n) = &inner.kind
                    && fits_i32(-*n) {
                        return Expr { kind: ExprKind::Int((-*n) as i32), ty: Ty::Int, span };
                    }
                let e = self.expr(inner, Some(&Ty::Int));
                if let Some(key) = Self::type_key(&e.ty)
                    && matches!(e.ty, Ty::Struct(_))
                    && let Some(&mx) = self.method_index.get(&(key, "neg".to_owned()))
                {
                    return self.call_method(mx, e, &[], span, span);
                }
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
        let mut l = self.expr(lhs, None);
        if let Ty::Struct(_) = &l.ty
            && let Some(name) = operator_method(op)
            && let Some(key) = Self::type_key(&l.ty)
            && let Some(&mx) = self.method_index.get(&(key, name.to_owned()))
        {
            // `a + b` on a fixed-point number is `a.add(b)`
            return self.call_method(mx, l, std::slice::from_ref(rhs), span, span);
        }
        let comparing = matches!(op, BinOp::Eq | BinOp::Ne);
        // the left side gives the right one its expected type (`xs == []`, `e == @s`)
        let mut r = if comparing && !matches!(l.ty, Ty::Selector | Ty::Error) { self.expr(rhs, Some(&l.ty.clone())) } else { self.expr(rhs, None) };
        if comparing && matches!((&l.ty, &r.ty), (Ty::Selector, Ty::Entity | Ty::Selector)) {
            // a selector that matches one entity stands for that entity
            l = self.coerce_to_entity(l);
            r = self.coerce_to_entity(r);
        }
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
                    } else if !l.ty.is_storable() {
                        // selectors and coordinates are queries, not values; `()` has nothing to compare
                        let msg = format!("`{}` cannot compare values of type `{}`", op_str(op), self.tn(&l.ty));
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
        if name.name.is_empty() {
            // `value.` with the name still to be typed (the parser reported it): keep the base
            // and its type, so an editor can offer completions
            return Expr { kind: ExprKind::Field { base: Box::new(b), index: usize::MAX, name: String::new() }, ty: Ty::Error, span };
        }
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
                // keep the base and its type (as for a struct), so an editor can offer its methods
                Expr { kind: ExprKind::Field { base: Box::new(b), index: usize::MAX, name: name.name.clone() }, ty: Ty::Error, span }
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

    fn call(&mut self, callee: &'a ast::Expr, args: &'a [ast::Expr], expected: Option<&Ty>, span: Span) -> Expr {
        match &callee.kind {
            ast::ExprKind::Ident(name) => {
                if self.lookup_value_or_local(name).is_some() && !matches!(self.lookup_value(name), Some(ValueSym::Fn(_) | ValueSym::Builtin(_) | ValueSym::Variant(..))) {
                    self.error(callee.span, format!("`{name}` is not a function"));
                    self.args_for_errors(args);
                    return Expr::error(span);
                }
                match self.lookup_value(name) {
                    Some(ValueSym::Fn(id)) => self.user_call(id, args, span),
                    Some(ValueSym::Variant(eid, vi)) => self.construct(eid, vi, args, expected, span),
                    Some(ValueSym::Builtin(i)) => {
                        let b = self.intrinsics[i].clone();
                        self.builtin_call(&b, args, span)
                    }
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
            ast::ExprKind::Path { module, name } if self.assoc_method(module, name).is_some() => {
                let mx = self.assoc_method(module, name).expect("checked");
                self.call_assoc(mx, args, callee, span)
            }
            ast::ExprKind::Path { module, name } => {
                // `Option::Some(x)` builds an enum value; anything else names a module's function
                let sym = match self.variant_path(module, name) {
                    Some(Some((eid, vi))) => Some(ValueSym::Variant(eid, vi)),
                    Some(None) => None,
                    None if self.path_type_key(module).is_some() => {
                        self.error(name.span, format!("`{}` has no function or method `{}`", module.name, name.name));
                        None
                    }
                    None => self.qualified_value(module, name),
                };
                match sym {
                    Some(ValueSym::Fn(id)) => self.user_call(id, args, span),
                    Some(ValueSym::Variant(eid, vi)) => self.construct(eid, vi, args, expected, span),
                    Some(ValueSym::Builtin(i)) => {
                        let b = self.intrinsics[i].clone();
                        self.builtin_call(&b, args, span)
                    }
                    Some(_) => {
                        self.error(callee.span, format!("`{}::{}` is not a function", module.name, name.name));
                        self.args_for_errors(args);
                        Expr::error(span)
                    }
                    None => {
                        self.args_for_errors(args);
                        Expr::error(span)
                    }
                }
            }
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
        if self.fns[id.0].decl.is_task && !self.starting {
            self.error_hint(span, format!("`{name}` is a task: it runs over several ticks, so it is not called"), format!("start it with `start {name}(..);`"));
            self.args_for_errors(args);
            return Expr::error(span);
        }
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

    /// The type an intrinsic returns; `Result<int, ()>` needs the standard library's `Result`.
    fn intrinsic_ret(&self, b: &Intrinsic) -> Ty {
        match (b.ret, self.prelude_types.get("Result")) {
            (BRet::CommandResult, Some(Ty::Enum(id, _))) => Ty::Enum(*id, vec![Ty::Int, Ty::Unit]),
            (BRet::CommandResult, _) => Ty::Error,
            (other, _) => other.ty(),
        }
    }

    fn builtin_call(&mut self, b: &Intrinsic, args: &'a [ast::Expr], span: Span) -> Expr {
        let ret = self.intrinsic_ret(b);
        // `particle(kind, { options }, x, y, z, ..)`: a compound right after the name is the data
        // of the particle, not one of the declared parameters
        let options_at = (b.id == "particle" && args.len() >= 2 && matches!(args[1].kind, ast::ExprKind::Compound(_))).then_some(1);
        let options = options_at.map(|i| self.nbt_const(&args[i]));
        let argc = args.len() - usize::from(options_at.is_some());
        if argc < b.min_args || argc > b.params.len() {
            let takes = if b.min_args == b.params.len() {
                format!("{} argument{}", b.params.len(), if b.params.len() == 1 { "" } else { "s" })
            } else {
                format!("{} to {} arguments", b.min_args, b.params.len())
            };
            self.arity_error(&format!("`{}`", b.path()), takes, argc, span);
            self.args_for_errors(args);
            return Expr { kind: ExprKind::Error, ty: ret, span };
        }
        // `particle` takes its arguments in groups: the name, the position, the spread (delta,
        // speed, count), then the mode and the viewers; the options go after the name
        if b.id == "particle" && !matches!(argc, 1 | 4 | 9 | 10 | 11) {
            self.error_hint(span, format!("`particle` takes 1, 4, 9, 10 or 11 arguments (not counting the options), found {argc}"), "(kind), (kind, x, y, z), (kind, x, y, z, dx, dy, dz, speed, count), then a mode `\"force\"`/`\"normal\"`, then the viewers; the options `{ .. }` come right after the kind");
            self.args_for_errors(args);
            return Expr { kind: ExprKind::Error, ty: ret, span };
        }
        let mut hargs = Vec::new();
        let mut ok = true;
        for (a, p) in args.iter().enumerate().filter(|(i, _)| Some(*i) != options_at).map(|(_, a)| a).zip(&b.params) {
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
                BParam::ConstNum => match self.number_text(a) {
                    // carried as its text: it goes into the command as written
                    Some(text) => Expr { kind: ExprKind::Str(text), ty: Ty::Str, span: a.span },
                    None => {
                        self.error_hint(a.span, "expected a number written out", "like `0.2`, `-1.5` or `3` (a constant, not a variable: the game reads it as a plain number)");
                        Expr::error(a.span)
                    }
                },
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
        if let (Some(at), Some(compound)) = (options_at, options) {
            match compound {
                Some(c) => hargs.insert(at, Expr { kind: ExprKind::Nbt(c), ty: Ty::Str, span: args[at].span }),
                None => ok = false,
            }
        }
        if ok {
            self.builtin_semantics(b.id, &hargs);
        }
        Expr { kind: ExprKind::Builtin { name: b.id, args: hargs }, ty: ret, span }
    }

    /// A `{ key: value, .. }` written in the source as NBT: numbers, strings, booleans, lists and
    /// compounds of constants. A decimal number is a `float`, which is what the particle options
    /// that take one (`scale`, `color`) ask for, and the game also accepts it where an int is expected.
    fn nbt_const(&mut self, e: &'a ast::Expr) -> Option<Compound> {
        let ast::ExprKind::Compound(fields) = &e.kind else {
            self.error(e.span, "expected `{ key: value, .. }`");
            return None;
        };
        let mut out = Compound::new();
        for f in fields {
            let value = self.nbt_value(&f.value)?;
            out.insert(f.name.clone(), value);
        }
        Some(out)
    }

    fn nbt_value(&mut self, e: &'a ast::Expr) -> Option<Nbt> {
        use ast::ExprKind as K;
        Some(match &e.kind {
            K::Str(s) => Nbt::String(s.clone()),
            K::Bool(b) => Nbt::bool(*b),
            K::Compound(_) => Nbt::Compound(self.nbt_const(e)?),
            K::List(items) => Nbt::List(items.iter().map(|i| self.nbt_value(i)).collect::<Option<Vec<_>>>()?),
            K::Float(t) => match t.replace('_', "").parse::<f32>() {
                Ok(v) if v.is_finite() => Nbt::Float(v),
                _ => {
                    self.error(e.span, format!("`{t}` is not a number"));
                    return None;
                }
            },
            K::Unary { op: ast::UnOp::Neg, expr } => match self.nbt_value(expr)? {
                Nbt::Float(v) => Nbt::Float(-v),
                Nbt::Int(v) => Nbt::Int(v.checked_neg()?),
                _ => {
                    self.error(e.span, "`-` needs a number");
                    return None;
                }
            },
            _ => match self.const_expr(e, None) {
                Some((ConstVal::Int(n), _)) => Nbt::Int(n),
                Some((ConstVal::Bool(b), _)) => Nbt::bool(b),
                Some((ConstVal::Str(s), _)) => Nbt::String(s),
                Some(_) => {
                    self.error(e.span, "this kind of value is not allowed in NBT here");
                    return None;
                }
                None => return None,
            },
        })
    }

    /// The text of a number written in the source: an integer (or constant), or a decimal literal,
    /// possibly negated.
    fn number_text(&mut self, e: &'a ast::Expr) -> Option<String> {
        use ast::ExprKind as K;
        match &e.kind {
            K::Float(t) => {
                let clean: String = t.chars().filter(|c| *c != '_').collect();
                (clean.chars().all(|c| c.is_ascii_digit() || c == '.') && clean.matches('.').count() == 1).then_some(clean)
            }
            K::Unary { op: ast::UnOp::Neg, expr } => self.number_text(expr).map(|t| format!("-{t}")),
            _ => match self.const_expr(e, Some(&Ty::Int)) {
                Some((ConstVal::Int(n), _)) => Some(n.to_string()),
                _ => None,
            },
        }
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
            "effect.give" | "summon" | "setblock" | "particle" => {
                let idx = if name == "effect.give" { 1 } else if name == "summon" || name == "particle" { 0 } else { 3 };
                if let Some((id, sp)) = s(idx)
                    && let Err(e) = ResourceLocation::parse(&id) {
                        if name == "particle" && id.contains('{') {
                            self.error_hint(sp, "the options of a particle are their own argument", "write `particle(\"item\", { item: { id: \"minecraft:diamond\", count: 1 } }, ~, ~1, ~)`");
                        } else {
                            self.error(sp, format!("invalid id: {}", e.message()));
                        }
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
        if name.name.is_empty() {
            self.args_for_errors(args);
            return Expr::error(span);
        }
        if recv.ty.is_error() {
            self.args_for_errors(args);
            return Expr::error(span);
        }
        if let Some(key) = Self::type_key(&recv.ty)
            && let Some(&mx) = self.method_index.get(&(key, name.name.clone()))
        {
            return self.call_method(mx, recv, args, span, name.span);
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

/// The method a struct provides for an operator (`a + b` is `a.add(b)`).
fn operator_method(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::Rem => "rem",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        BinOp::Eq | BinOp::Ne | BinOp::And | BinOp::Or => return None,
    })
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

/// Whether a type mentions an enum's type parameter.
fn contains_param(t: &Ty) -> bool {
    match t {
        Ty::Param(_) => true,
        Ty::List(i) => contains_param(i),
        Ty::Enum(_, args) => args.iter().any(contains_param),
        _ => false,
    }
}
