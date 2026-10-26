//! Naming conventions, checked as warnings (Rust's own: `snake_case` for functions, variables,
//! fields and parameters, `PascalCase` for types, enum variants and type parameters,
//! `SCREAMING_SNAKE_CASE` for `const`s). `Slab.toml` can turn it off: `[lint] naming = "off"`.
//!
//! The library (`std`, `game`) is not checked, and a leading `_` (an unused name) is ignored.

use crate::diag::Diagnostic;
use crate::syntax::ast::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Snake,
    Pascal,
    Screaming,
}

impl Style {
    fn label(self) -> &'static str {
        match self {
            Style::Snake => "snake_case",
            Style::Pascal => "PascalCase",
            Style::Screaming => "SCREAMING_SNAKE_CASE",
        }
    }

    fn accepts(self, name: &str) -> bool {
        let core = name.trim_start_matches('_');
        if core.is_empty() {
            return true;
        }
        let first_ok = !core.starts_with(|c: char| c.is_ascii_digit());
        match self {
            Style::Snake => first_ok && core.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            Style::Screaming => first_ok && core.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            Style::Pascal => core.starts_with(|c: char| c.is_ascii_uppercase()) && core.chars().all(|c| c.is_ascii_alphanumeric()),
        }
    }

    /// The name in this style: `createRing` -> `create_ring`, `my_type` -> `MyType`.
    fn convert(self, name: &str) -> String {
        let lead: String = name.chars().take_while(|c| *c == '_').collect();
        let core = &name[lead.len()..];
        // words: split at `_`, at a lower-to-upper step (`createRing`), and before the last capital
        // of an acronym followed by a word (`HTTPServer` -> `HTTP`, `Server`)
        let chars: Vec<char> = core.chars().collect();
        let mut words: Vec<String> = Vec::new();
        let mut word = String::new();
        for (i, &c) in chars.iter().enumerate() {
            if c == '_' {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                continue;
            }
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1).copied();
            let boundary = c.is_ascii_uppercase() && prev.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit() || (p.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase())));
            if boundary && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            word.push(c);
        }
        if !word.is_empty() {
            words.push(word);
        }
        let body = match self {
            Style::Snake => words.iter().map(|w| w.to_ascii_lowercase()).collect::<Vec<_>>().join("_"),
            Style::Screaming => words.iter().map(|w| w.to_ascii_uppercase()).collect::<Vec<_>>().join("_"),
            Style::Pascal => words
                .iter()
                .map(|w| {
                    let mut cs = w.chars();
                    cs.next().map(|f| f.to_ascii_uppercase().to_string() + &cs.as_str().to_ascii_lowercase()).unwrap_or_default()
                })
                .collect::<String>(),
        };
        format!("{lead}{body}")
    }
}

struct Lint {
    file: usize,
    out: Vec<Diagnostic>,
}

impl Lint {
    fn name(&mut self, what: &str, id: &Ident, style: Style) {
        if style.accepts(&id.name) {
            return;
        }
        let better = style.convert(&id.name);
        if better == id.name || better.is_empty() {
            return;
        }
        self.out.push(Diagnostic::warning(format!("{what} `{}` should be {}", id.name, style.label()), id.span).hint(format!("rename it to `{better}`")).in_file(self.file));
    }

    fn program(&mut self, p: &Program) {
        for item in &p.items {
            self.item(item);
        }
    }

    fn item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Fn(f) => self.func(f),
            ItemKind::Global(v) => {
                self.name("global", &v.name, Style::Snake);
                self.expr(&v.init);
            }
            ItemKind::Const(v) => {
                self.name("constant", &v.name, Style::Screaming);
                self.expr(&v.init);
            }
            ItemKind::Struct(s) => {
                self.name("struct", &s.name, Style::Pascal);
                for f in &s.fields {
                    self.name("field", &f.name, Style::Snake);
                }
            }
            ItemKind::Enum(e) => {
                self.name("enum", &e.name, Style::Pascal);
                for g in &e.generics {
                    self.name("type parameter", g, Style::Pascal);
                }
                for v in &e.variants {
                    self.name("variant", &v.name, Style::Pascal);
                }
            }
            ItemKind::Impl(i) => {
                for g in &i.generics {
                    self.name("type parameter", g, Style::Pascal);
                }
                for m in &i.methods {
                    self.func(&m.decl);
                }
            }
            ItemKind::Objective { name, .. } => self.name("objective", name, Style::Snake),
            ItemKind::Import(_) => {}
        }
    }

    fn func(&mut self, f: &FnDecl) {
        self.name("function", &f.name, Style::Snake);
        for p in &f.params {
            if p.name.name != "self" {
                self.name("parameter", &p.name, Style::Snake);
            }
        }
        self.block(&f.body);
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let { name, init, .. } => {
                self.name("variable", name, Style::Snake);
                self.expr(init);
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            StmtKind::If { cond, then, els } => {
                self.expr(cond);
                self.block(then);
                self.else_part(els);
            }
            StmtKind::IfLet { pat, value, then, els } => {
                self.pattern(pat);
                self.expr(value);
                self.block(then);
                self.else_part(els);
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            StmtKind::Loop { body } => self.block(body),
            StmtKind::For { var, iter, body } => {
                self.name("variable", var, Style::Snake);
                self.expr(iter);
                self.block(body);
            }
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    self.expr(e);
                }
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Context { body, .. } => self.block(body),
            StmtKind::Block(b) => self.block(b),
            StmtKind::Start(e) | StmtKind::Expr(e) => self.expr(e),
        }
    }

    fn else_part(&mut self, els: &Option<Box<Else>>) {
        match els.as_deref() {
            Some(Else::Block(b)) => self.block(b),
            Some(Else::If(s)) => self.stmt(s),
            None => {}
        }
    }

    /// Only the names that a pattern introduces: a lower-case bare name is a new variable.
    fn pattern(&mut self, p: &Pattern) {
        match &p.kind {
            PatKind::Ident(id) if id.name.starts_with(|c: char| c.is_lowercase() || c == '_') => self.name("variable", id, Style::Snake),
            PatKind::Variant { fields, .. } => fields.iter().for_each(|f| self.pattern(f)),
            PatKind::Or(alts) => alts.iter().for_each(|a| self.pattern(a)),
            _ => {}
        }
    }

    /// Expressions hold statements only inside `match` arms.
    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Match { scrut, arms } => {
                self.expr(scrut);
                for a in arms {
                    self.pattern(&a.pat);
                    if let Some(g) = &a.guard {
                        self.expr(g);
                    }
                    match &a.body {
                        ArmBody::Expr(x) => self.expr(x),
                        ArmBody::Block(b) => self.block(b),
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee);
                args.iter().for_each(|a| self.expr(a));
            }
            ExprKind::Unary { expr, .. } => self.expr(expr),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Field { base, .. } => self.expr(base),
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            ExprKind::List(items) => items.iter().for_each(|i| self.expr(i)),
            ExprKind::Compound(fields) | ExprKind::StructLit { fields, .. } => fields.iter().for_each(|f| self.expr(&f.value)),
            ExprKind::Format(parts) => {
                for p in parts {
                    if let FormatPart::Expr(e) = p {
                        self.expr(e);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The naming warnings of one module of the project (not of the library).
pub fn naming(program: &Program, file: usize) -> Vec<Diagnostic> {
    let mut lint = Lint { file, out: Vec::new() };
    lint.program(program);
    lint.out.sort_by_key(|d| d.span.start);
    lint.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn warnings(src: &str) -> Vec<String> {
        naming(&parse(src).unwrap(), 0).into_iter().map(|d| format!("{} -> {}", d.message, d.hint.unwrap_or_default())).collect()
    }

    #[test]
    fn conversions() {
        assert_eq!(Style::Snake.convert("createRing"), "create_ring");
        assert_eq!(Style::Snake.convert("HTTPServer"), "http_server");
        assert_eq!(Style::Snake.convert("_unusedValue"), "_unused_value");
        assert_eq!(Style::Pascal.convert("my_type"), "MyType");
        assert_eq!(Style::Pascal.convert("phase"), "Phase");
        assert_eq!(Style::Screaming.convert("maxHealth"), "MAX_HEALTH");
        assert_eq!(Style::Screaming.convert("max_health"), "MAX_HEALTH");
        assert!(Style::Snake.accepts("_x") && Style::Snake.accepts("x1_y") && !Style::Snake.accepts("xY"));
        assert!(Style::Pascal.accepts("Point") && Style::Pascal.accepts("HTTPServer") && !Style::Pascal.accepts("point") && !Style::Pascal.accepts("My_Type"));
        assert!(Style::Screaming.accepts("MAX_1") && !Style::Screaming.accepts("Max"));
    }

    #[test]
    fn conventions_pass_quietly() {
        let ok = "const MAX_HEALTH: int = 20; global kills: int = 0; objective score_board = \"dummy\";\nstruct Point { x: int, y_pos: int }\nenum Phase<T> { Waiting, Running(T) }\nimpl Point { fn sum(self) -> int { return self.x; } }\nfn make_ring(center_x: int) { let mut count = 0; for i in 0..3 { count += i; } match count { n if n > 1 => {} _ => {} } let _unused = 1; }";
        assert_eq!(warnings(ok), Vec::<String>::new());
    }

    #[test]
    fn mistakes_name_the_better_spelling() {
        let w = warnings("const maxHealth: int = 1; global Kills: int = 0; struct point { xPos: int } enum phase { running_now } fn makeRing(centerX: int) { let myVar = 1; for Item in [1] {} if let Some(innerValue) = o() {} }");
        for expected in [
            "constant `maxHealth` should be SCREAMING_SNAKE_CASE -> rename it to `MAX_HEALTH`",
            "global `Kills` should be snake_case -> rename it to `kills`",
            "struct `point` should be PascalCase -> rename it to `Point`",
            "field `xPos` should be snake_case -> rename it to `x_pos`",
            "enum `phase` should be PascalCase -> rename it to `Phase`",
            "variant `running_now` should be PascalCase -> rename it to `RunningNow`",
            "function `makeRing` should be snake_case -> rename it to `make_ring`",
            "parameter `centerX` should be snake_case -> rename it to `center_x`",
            "variable `myVar` should be snake_case -> rename it to `my_var`",
            "variable `Item` should be snake_case -> rename it to `item`",
            "variable `innerValue` should be snake_case -> rename it to `inner_value`",
        ] {
            assert!(w.contains(&expected.to_owned()), "{expected}\nin {w:#?}");
        }
        let d = naming(&parse("fn badName() {}").unwrap(), 3);
        assert_eq!((d.len(), d[0].file), (1, 3));
    }
}
