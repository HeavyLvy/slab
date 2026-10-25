//! Tokens -> AST. Grammar: docs/lang/syntax.md.

use super::ast::*;
use super::lexer::lex;
use super::token::{FPart, Kw, Tok, Token};
use crate::diag::{Diagnostic, Span};

type R<T> = Result<T, Diagnostic>;

/// Parses a whole file. Stops at the first error.
pub fn parse(src: &str) -> R<Program> {
    let tokens = lex(src)?;
    Parser { tokens, pos: 0 }.program()
}

/// Expression-context restrictions.
#[derive(Clone, Copy, Default)]
struct Flags {
    /// `Name {` is not a struct literal (the `{` starts a statement body).
    no_struct: bool,
    /// No call or index suffix: context operands such as `positioned x (y) z` are adjacent.
    no_call: bool,
}

const HEAD: Flags = Flags { no_struct: true, no_call: false };
const OPERAND: Flags = Flags { no_struct: true, no_call: true };
const FREE: Flags = Flags { no_struct: false, no_call: false };

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

fn is_ctx_kw(t: &Tok) -> bool {
    matches!(t, Tok::Kw(Kw::As | Kw::At | Kw::In | Kw::Positioned | Kw::Rotated | Kw::Facing | Kw::Align | Kw::Anchored))
}

fn bin_op(t: &Tok) -> Option<(BinOp, u8)> {
    Some(match t {
        Tok::OrOr => (BinOp::Or, 1),
        Tok::AndAnd => (BinOp::And, 2),
        Tok::EqEq => (BinOp::Eq, 3),
        Tok::NotEq => (BinOp::Ne, 3),
        Tok::Lt => (BinOp::Lt, 3),
        Tok::Le => (BinOp::Le, 3),
        Tok::Gt => (BinOp::Gt, 3),
        Tok::Ge => (BinOp::Ge, 3),
        Tok::Plus => (BinOp::Add, 4),
        Tok::Minus => (BinOp::Sub, 4),
        Tok::Star => (BinOp::Mul, 5),
        Tok::Slash => (BinOp::Div, 5),
        Tok::Percent => (BinOp::Rem, 5),
        _ => return None,
    })
}

fn assign_op(t: &Tok) -> Option<AssignOp> {
    Some(match t {
        Tok::Eq => AssignOp::Assign,
        Tok::PlusEq => AssignOp::Add,
        Tok::MinusEq => AssignOp::Sub,
        Tok::StarEq => AssignOp::Mul,
        Tok::SlashEq => AssignOp::Div,
        Tok::PercentEq => AssignOp::Rem,
        _ => return None,
    })
}

impl Parser {
    // ---- token access

    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn prev_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }

    fn next(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }

    fn at_kw(&self, k: Kw) -> bool {
        self.peek() == &Tok::Kw(k)
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.next();
            true
        } else {
            false
        }
    }

    fn unexpected<T>(&self, expected: &str) -> R<T> {
        Err(Diagnostic::new(format!("expected {expected}, found {}", self.peek().describe()), self.span()))
    }

    fn expect(&mut self, t: &Tok) -> R<Span> {
        if self.at(t) {
            Ok(self.next().span)
        } else {
            self.unexpected(&t.describe())
        }
    }

    /// `;` errors point just after the previous token, where the `;` belongs.
    fn expect_semi(&mut self) -> R<Span> {
        if self.at(&Tok::Semi) {
            return Ok(self.next().span);
        }
        let prev = self.prev_span();
        Err(Diagnostic::new(format!("expected `;`, found {}", self.peek().describe()), Span { start: prev.end, end: prev.end + 1 }))
    }

    fn ident(&mut self, what: &str) -> R<Ident> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let span = self.next().span;
                Ok(Ident { name, span })
            }
            Tok::Kw(k) => Err(Diagnostic::new(format!("expected {what}, found keyword `{}`", k.as_str()), self.span())
                .hint("keywords cannot be used as names")),
            _ => self.unexpected(what),
        }
    }

    // ---- items

    fn program(mut self) -> R<Program> {
        let mut items = Vec::new();
        while !self.at(&Tok::Eof) {
            items.push(self.item()?);
        }
        Ok(Program { items })
    }

    fn attr(&mut self) -> R<Attr> {
        let start = self.expect(&Tok::Hash)?;
        self.expect(&Tok::LBracket)?;
        let name = self.ident("an attribute name")?;
        let mut args = Vec::new();
        if self.eat(&Tok::LParen) {
            while !self.at(&Tok::RParen) {
                args.push(self.expr(FREE)?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen)?;
        }
        let end = self.expect(&Tok::RBracket)?;
        Ok(Attr { name, args, span: start.to(end) })
    }

    fn item(&mut self) -> R<Item> {
        let mut attrs = Vec::new();
        while self.at(&Tok::Hash) {
            attrs.push(self.attr()?);
        }
        let start = attrs.first().map(|a| a.span).unwrap_or_else(|| self.span());
        let pub_span = self.span();
        let public = self.eat(&Tok::Kw(Kw::Pub));
        let kind = match self.peek() {
            Tok::Kw(Kw::Import) => {
                if public {
                    return Err(Diagnostic::new("an import cannot be `pub`", pub_span).hint("re-exporting is not supported; import the module where you use it"));
                }
                ItemKind::Import(self.import_decl()?)
            }
            Tok::Kw(Kw::Fn) => ItemKind::Fn(self.fn_decl()?),
            Tok::Kw(Kw::Global) => {
                self.next();
                ItemKind::Global(self.var_decl()?)
            }
            Tok::Kw(Kw::Const) => {
                self.next();
                ItemKind::Const(self.var_decl()?)
            }
            Tok::Kw(Kw::Struct) => ItemKind::Struct(self.struct_decl()?),
            Tok::Kw(Kw::Objective) => {
                self.next();
                let name = self.ident("an objective name")?;
                self.expect(&Tok::Eq)?;
                let criteria = self.expr(FREE)?;
                self.expect_semi()?;
                ItemKind::Objective { name, criteria }
            }
            _ => return self.unexpected("an item (`fn`, `global`, `const`, `struct`, `objective` or `import`)"),
        };
        Ok(Item { attrs, public, kind, span: start.to(self.prev_span()) })
    }

    /// `import a::b;`  `import a::b as c;`  `import a::b::{x, y};`
    fn import_decl(&mut self) -> R<ImportDecl> {
        self.expect(&Tok::Kw(Kw::Import))?;
        let mut path = vec![self.ident("a module name")?];
        let mut items = None;
        let mut glob = false;
        while self.eat(&Tok::ColonColon) {
            if self.eat(&Tok::Star) {
                glob = true;
                break;
            }
            if self.eat(&Tok::LBrace) {
                let mut names = Vec::new();
                while !self.at(&Tok::RBrace) {
                    names.push(self.ident("an item name")?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RBrace)?;
                if names.is_empty() {
                    return Err(Diagnostic::new("an import list needs at least one item", self.prev_span()));
                }
                items = Some(names);
                break;
            }
            path.push(self.ident("a module name")?);
        }
        let alias = if self.eat(&Tok::Kw(Kw::As)) {
            if items.is_some() || glob {
                return Err(Diagnostic::new("`as` cannot be combined with an item list or `*`", self.prev_span()));
            }
            Some(self.ident("an alias")?)
        } else {
            None
        };
        self.expect_semi()?;
        Ok(ImportDecl { path, alias, items, glob })
    }

    fn fn_decl(&mut self) -> R<FnDecl> {
        self.expect(&Tok::Kw(Kw::Fn))?;
        let name = self.ident("a function name")?;
        self.expect(&Tok::LParen)?;
        let params = self.params(&Tok::RParen)?;
        self.expect(&Tok::RParen)?;
        let ret = if self.eat(&Tok::Arrow) { Some(self.ty()?) } else { None };
        let body = self.block()?;
        Ok(FnDecl { name, params, ret, body })
    }

    /// `name: type, ...` up to (not including) `end`.
    fn params(&mut self, end: &Tok) -> R<Vec<Param>> {
        let mut out = Vec::new();
        while !self.at(end) {
            let name = self.ident("a name")?;
            self.expect(&Tok::Colon)?;
            let ty = self.ty()?;
            out.push(Param { name, ty });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        Ok(out)
    }

    fn var_decl(&mut self) -> R<VarDecl> {
        let name = self.ident("a name")?;
        self.expect(&Tok::Colon)?;
        let ty = self.ty()?;
        self.expect(&Tok::Eq)?;
        let init = self.expr(FREE)?;
        self.expect_semi()?;
        Ok(VarDecl { name, ty, init })
    }

    fn struct_decl(&mut self) -> R<StructDecl> {
        self.expect(&Tok::Kw(Kw::Struct))?;
        let name = self.ident("a struct name")?;
        self.expect(&Tok::LBrace)?;
        let fields = self.params(&Tok::RBrace)?;
        self.expect(&Tok::RBrace)?;
        Ok(StructDecl { name, fields })
    }

    /// The rest of `a::b::c` after its first segment: the module (`a::b`, one identifier joined
    /// with `::`) and the final name; `(None, a)` when there is no `::`.
    fn path_tail(&mut self, first: Ident, what: &str) -> R<(Option<Ident>, Ident)> {
        if !self.at(&Tok::ColonColon) {
            return Ok((None, first));
        }
        let mut module = first;
        loop {
            self.expect(&Tok::ColonColon)?;
            let next = self.ident(what)?;
            if self.at(&Tok::ColonColon) {
                module = Ident { name: format!("{}::{}", module.name, next.name), span: module.span.to(next.span) };
            } else {
                return Ok((Some(module), next));
            }
        }
    }

    fn ty(&mut self) -> R<Type> {
        let first = self.ident("a type")?;
        let (module, name) = self.path_tail(first, "a type name")?;
        let mut args = Vec::new();
        let mut end = name.span;
        if self.eat(&Tok::Lt) {
            loop {
                args.push(self.ty()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            end = self.expect(&Tok::Gt)?;
        }
        let start = module.as_ref().map_or(name.span, |m| m.span);
        Ok(Type { span: start.to(end), module, name, args })
    }

    // ---- statements

    fn block(&mut self) -> R<Block> {
        let start = self.expect(&Tok::LBrace)?;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) {
            if self.at(&Tok::Eof) {
                return Err(Diagnostic::new("unclosed block, expected `}`", start).hint("this `{` is never closed"));
            }
            stmts.push(self.stmt()?);
        }
        let end = self.expect(&Tok::RBrace)?;
        Ok(Block { stmts, span: start.to(end) })
    }

    fn stmt(&mut self) -> R<Stmt> {
        let start = self.span();
        let kind = match self.peek() {
            Tok::Kw(Kw::Let) => {
                self.next();
                let mutable = self.eat(&Tok::Kw(Kw::Mut));
                let name = self.ident("a variable name")?;
                let ty = if self.eat(&Tok::Colon) { Some(self.ty()?) } else { None };
                self.expect(&Tok::Eq)?;
                let init = self.expr(FREE)?;
                self.expect_semi()?;
                StmtKind::Let { name, mutable, ty, init }
            }
            Tok::Kw(Kw::If) => return self.if_stmt(),
            Tok::Kw(Kw::While) => {
                self.next();
                let cond = self.expr(HEAD)?;
                StmtKind::While { cond, body: self.block()? }
            }
            Tok::Kw(Kw::Loop) => {
                self.next();
                StmtKind::Loop { body: self.block()? }
            }
            Tok::Kw(Kw::For) => {
                self.next();
                let var = self.ident("a loop variable")?;
                self.expect(&Tok::Kw(Kw::In))?;
                let iter = self.expr(HEAD)?;
                StmtKind::For { var, iter, body: self.block()? }
            }
            Tok::Kw(Kw::Return) => {
                self.next();
                let value = if self.at(&Tok::Semi) { None } else { Some(self.expr(FREE)?) };
                self.expect_semi()?;
                StmtKind::Return(value)
            }
            Tok::Kw(Kw::Break) => {
                self.next();
                self.expect_semi()?;
                StmtKind::Break
            }
            Tok::Kw(Kw::Continue) => {
                self.next();
                self.expect_semi()?;
                StmtKind::Continue
            }
            Tok::LBrace => StmtKind::Block(self.block()?),
            t if is_ctx_kw(t) => self.context()?,
            _ => self.expr_stmt()?,
        };
        Ok(Stmt { kind, span: start.to(self.prev_span()) })
    }

    fn if_stmt(&mut self) -> R<Stmt> {
        let start = self.expect(&Tok::Kw(Kw::If))?;
        let cond = self.expr(HEAD)?;
        let then = self.block()?;
        let els = if self.eat(&Tok::Kw(Kw::Else)) {
            if self.at_kw(Kw::If) {
                Some(Box::new(Else::If(Box::new(self.if_stmt()?))))
            } else {
                Some(Box::new(Else::Block(self.block()?)))
            }
        } else {
            None
        };
        Ok(Stmt { kind: StmtKind::If { cond, then, els }, span: start.to(self.prev_span()) })
    }

    fn context(&mut self) -> R<StmtKind> {
        let mut clauses = Vec::new();
        while is_ctx_kw(self.peek()) {
            let start = self.span();
            let kw = match self.next().tok {
                Tok::Kw(k) => k,
                _ => unreachable!(),
            };
            let kind = match kw {
                Kw::As => CtxKind::As(self.expr_operand()?),
                Kw::At => CtxKind::At(self.expr_operand()?),
                Kw::In => CtxKind::In(self.expr_operand()?),
                Kw::Positioned => CtxKind::Positioned([self.expr_operand()?, self.expr_operand()?, self.expr_operand()?]),
                Kw::Facing => CtxKind::Facing([self.expr_operand()?, self.expr_operand()?, self.expr_operand()?]),
                Kw::Rotated => CtxKind::Rotated([self.expr_operand()?, self.expr_operand()?]),
                Kw::Align => CtxKind::Align(self.ident("axes such as `xz`")?),
                Kw::Anchored => CtxKind::Anchored(self.ident("`eyes` or `feet`")?),
                _ => unreachable!(),
            };
            clauses.push(CtxClause { kind, span: start.to(self.prev_span()) });
        }
        if !self.at(&Tok::LBrace) {
            return self.unexpected("`{` or another context clause");
        }
        Ok(StmtKind::Context { clauses, body: self.block()? })
    }

    fn expr_stmt(&mut self) -> R<StmtKind> {
        let e = self.expr(FREE)?;
        if let Some(op) = assign_op(self.peek()) {
            if !matches!(e.kind, ExprKind::Ident(_) | ExprKind::Path { .. } | ExprKind::Field { .. } | ExprKind::Index { .. }) {
                return Err(Diagnostic::new("cannot assign to this expression", e.span).hint("assign to a variable, a field (`a.b`) or an index (`a[i]`)"));
            }
            self.next();
            let value = self.expr(FREE)?;
            self.expect_semi()?;
            return Ok(StmtKind::Assign { target: e, op, value });
        }
        self.expect_semi()?;
        Ok(StmtKind::Expr(e))
    }

    // ---- expressions

    fn can_start_expr(&self, flags: Flags) -> bool {
        match self.peek() {
            Tok::Int(_) | Tok::Float(_) | Tok::Str(_) | Tok::Ident(_) | Tok::Selector { .. } | Tok::Rel { .. } => true,
            Tok::Kw(Kw::True | Kw::False) => true,
            Tok::LParen | Tok::LBracket | Tok::Minus | Tok::Bang => true,
            Tok::LBrace => !flags.no_struct,
            _ => false,
        }
    }

    fn expr(&mut self, flags: Flags) -> R<Expr> {
        let range_op = |t: &Tok| match t {
            Tok::DotDot => Some(false),
            Tok::DotDotEq => Some(true),
            _ => None,
        };
        let start = self.span();
        if let Some(inclusive) = range_op(self.peek()) {
            self.next();
            let end = self.binary(1, flags)?;
            return Ok(Expr { span: start.to(end.span), kind: ExprKind::Range { start: None, end: Some(Box::new(end)), inclusive } });
        }
        let lhs = self.binary(1, flags)?;
        if let Some(inclusive) = range_op(self.peek()) {
            self.next();
            let end = if self.can_start_expr(flags) { Some(Box::new(self.binary(1, flags)?)) } else { None };
            if inclusive && end.is_none() {
                return Err(Diagnostic::new("an inclusive range `..=` needs an end", self.prev_span()));
            }
            let span = start.to(self.prev_span());
            return Ok(Expr { kind: ExprKind::Range { start: Some(Box::new(lhs)), end, inclusive }, span });
        }
        Ok(lhs)
    }

    /// One context-clause operand: a unary-level expression with no call/index suffix.
    fn expr_operand(&mut self) -> R<Expr> {
        self.unary(OPERAND)
    }

    fn binary(&mut self, min_prec: u8, flags: Flags) -> R<Expr> {
        let mut lhs = self.unary(flags)?;
        while let Some((op, prec)) = bin_op(self.peek()) {
            if prec < min_prec {
                break;
            }
            self.next();
            let rhs = self.binary(prec + 1, flags)?;
            let span = lhs.span.to(rhs.span);
            lhs = Expr { kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, span };
            if prec == 3 && matches!(bin_op(self.peek()), Some((_, 3))) {
                return Err(Diagnostic::new("comparison operators cannot be chained", self.span()).hint("use `&&` to combine comparisons: `a < b && b < c`"));
            }
        }
        Ok(lhs)
    }

    fn unary(&mut self, flags: Flags) -> R<Expr> {
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Bang => UnOp::Not,
            _ => return self.postfix(flags),
        };
        let start = self.next().span;
        let inner = self.unary(flags)?;
        Ok(Expr { span: start.to(inner.span), kind: ExprKind::Unary { op, expr: Box::new(inner) } })
    }

    fn postfix(&mut self, flags: Flags) -> R<Expr> {
        let mut e = self.primary(flags)?;
        loop {
            match self.peek() {
                Tok::LParen if !flags.no_call => {
                    self.next();
                    let mut args = Vec::new();
                    while !self.at(&Tok::RParen) {
                        args.push(self.expr(FREE)?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    let end = self.expect(&Tok::RParen)?;
                    e = Expr { span: e.span.to(end), kind: ExprKind::Call { callee: Box::new(e), args } };
                }
                Tok::Dot => {
                    self.next();
                    let name = self.ident("a field or method name")?;
                    e = Expr { span: e.span.to(name.span), kind: ExprKind::Field { base: Box::new(e), name } };
                }
                Tok::LBracket if !flags.no_call => {
                    self.next();
                    let index = self.expr(FREE)?;
                    let end = self.expect(&Tok::RBracket)?;
                    e = Expr { span: e.span.to(end), kind: ExprKind::Index { base: Box::new(e), index: Box::new(index) } };
                }
                _ => return Ok(e),
            }
        }
    }

    /// At a `{`: is it followed by `}` or `key:`, i.e. the body of a struct literal?
    fn looks_like_struct_body(&self) -> bool {
        matches!(self.peek(), Tok::LBrace)
            && match self.peek_at(1) {
                Tok::RBrace => true,
                Tok::Ident(_) | Tok::Str(_) => matches!(self.peek_at(2), Tok::Colon),
                _ => false,
            }
    }

    fn field_inits(&mut self) -> R<Vec<FieldInit>> {
        let mut out = Vec::new();
        while !self.at(&Tok::RBrace) {
            let (name, name_span) = match self.peek().clone() {
                Tok::Ident(n) | Tok::Str(n) => (n, self.next().span),
                _ => return self.unexpected("a field name"),
            };
            self.expect(&Tok::Colon)?;
            let value = self.expr(FREE)?;
            out.push(FieldInit { name, name_span, value });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        Ok(out)
    }

    fn primary(&mut self, flags: Flags) -> R<Expr> {
        let span = self.span();
        let kind = match self.peek().clone() {
            Tok::Int(n) => {
                self.next();
                ExprKind::Int(n)
            }
            Tok::Float(s) => {
                self.next();
                ExprKind::Float(s)
            }
            Tok::Str(s) => {
                self.next();
                ExprKind::Str(s)
            }
            Tok::FStr(parts) => {
                self.next();
                let mut out = Vec::new();
                for p in parts {
                    out.push(match p {
                        FPart::Lit(s) => FormatPart::Lit(s),
                        FPart::Expr { src, start } => FormatPart::Expr(parse_embedded(&src, start as u32)?),
                    });
                }
                ExprKind::Format(out)
            }
            Tok::Kw(Kw::True) => {
                self.next();
                ExprKind::Bool(true)
            }
            Tok::Kw(Kw::False) => {
                self.next();
                ExprKind::Bool(false)
            }
            Tok::Ident(_) => {
                let first = self.ident("a name")?;
                let (module, name) = self.path_tail(first, "a name after `::`")?;
                if !flags.no_struct && self.looks_like_struct_body() {
                    self.expect(&Tok::LBrace)?;
                    let fields = self.field_inits()?;
                    let end = self.expect(&Tok::RBrace)?;
                    return Ok(Expr { span: span.to(end), kind: ExprKind::StructLit { module, name, fields } });
                }
                match module {
                    Some(module) => ExprKind::Path { module, name },
                    None => ExprKind::Ident(name.name),
                }
            }
            Tok::Selector { var, args } => {
                self.next();
                let args = match args {
                    Some((raw, raw_span)) => split_selector_args(&raw, raw_span)?,
                    None => Vec::new(),
                };
                ExprKind::Selector(SelectorLit { var, args })
            }
            Tok::Rel { kind, offset } => {
                self.next();
                ExprKind::Rel { kind, offset }
            }
            Tok::LParen => {
                self.next();
                let inner = self.expr(FREE)?;
                let end = self.expect(&Tok::RParen)?;
                return Ok(Expr { span: span.to(end), kind: inner.kind });
            }
            Tok::LBracket => {
                self.next();
                let mut items = Vec::new();
                while !self.at(&Tok::RBracket) {
                    items.push(self.expr(FREE)?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let end = self.expect(&Tok::RBracket)?;
                return Ok(Expr { span: span.to(end), kind: ExprKind::List(items) });
            }
            Tok::LBrace if !flags.no_struct => {
                self.next();
                let fields = self.field_inits()?;
                let end = self.expect(&Tok::RBrace)?;
                return Ok(Expr { span: span.to(end), kind: ExprKind::Compound(fields) });
            }
            _ => return self.unexpected("an expression"),
        };
        Ok(Expr { kind, span: span.to(self.prev_span()) })
    }
}

/// Parses the expression inside an f-string's `{...}`. `base` is where the text starts in the
/// file, so every token (and therefore every error) points at the real position.
fn parse_embedded(src: &str, base: u32) -> R<Expr> {
    let whole = Span { start: base.saturating_sub(1), end: base + src.len() as u32 + 1 };
    if src.trim().is_empty() {
        return Err(Diagnostic::new("empty `{}` in an f-string", whole).hint("put an expression between the braces, or write `{{}}` for literal braces"));
    }
    let shift = |s: Span| Span { start: s.start + base, end: s.end + base };
    let mut tokens = lex(src).map_err(|mut d| {
        d.span = shift(d.span);
        d
    })?;
    for t in &mut tokens {
        t.span = shift(t.span);
        if let Tok::Selector { args: Some((_, sp)), .. } = &mut t.tok {
            *sp = shift(*sp);
        }
    }
    let mut p = Parser { tokens, pos: 0 };
    let e = p.expr(FREE).map_err(|mut d| {
        // the embedded text ends at the closing brace, not at the end of the file
        d.message = d.message.replace("end of file", "the closing `}`");
        d
    })?;
    if !p.at(&Tok::Eof) {
        return p.unexpected("the end of the expression");
    }
    Ok(e)
}

/// Replaces the run-time parts of a selector value (`$name`, `${expression}`) with markers and
/// parses their expressions. `start` is the value's offset in the file.
fn extract_holes(value: &str, start: u32) -> R<(String, Vec<Expr>)> {
    if !value.contains('$') {
        return Ok((value.to_owned(), Vec::new()));
    }
    let mut out = String::new();
    let mut holes = Vec::new();
    let mut chars = value.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let at = start + i as u32;
        let bad = |d: &str| Diagnostic::new(d.to_owned(), Span { start: at, end: at + 1 }).hint("write `$name`, `${expression}`, or `$$` for a literal dollar sign");
        match chars.peek().copied() {
            Some((_, '$')) => {
                chars.next();
                out.push('$');
            }
            Some((_, '{')) => {
                chars.next();
                let body_start = i + 2;
                let (mut depth, mut quote, mut end) = (1usize, None::<char>, None);
                while let Some((j, c)) = chars.next() {
                    match (quote, c) {
                        (Some(_), '\\') => {
                            chars.next();
                        }
                        (Some(q), c) if c == q => quote = None,
                        (Some(_), _) => {}
                        (None, '"' | '\'') => quote = Some(c),
                        (None, '{') => depth += 1,
                        (None, '}') => {
                            depth -= 1;
                            if depth == 0 {
                                end = Some(j);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let Some(end) = end else {
                    return Err(Diagnostic::new("`${` in a selector is never closed", Span { start: at, end: at + 2 }).hint("end the expression with `}`"));
                };
                let text = &value[body_start..end];
                if text.trim().is_empty() {
                    return Err(Diagnostic::new("empty `${}` in a selector", Span { start: at, end: start + end as u32 + 1 }).hint("put an expression between the braces"));
                }
                holes.push(parse_embedded(text, start + body_start as u32)?);
                out.push_str(&format!("$(h{})", holes.len() - 1));
            }
            Some((_, n)) if n.is_alphabetic() || n == '_' => {
                let begin = i + 1;
                let mut end = begin;
                while let Some(&(j, c)) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        end = j + c.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                holes.push(parse_embedded(&value[begin..end], start + begin as u32)?);
                out.push_str(&format!("$(h{})", holes.len() - 1));
            }
            _ => return Err(bad("`$` in a selector must be followed by a name or `{expression}`")),
        }
    }
    Ok((out, holes))
}

/// Splits the raw text between a selector's brackets into `key=value` pairs. Commas and `=`
/// inside `{}`, `[]`, `()` or quotes do not split.
fn split_selector_args(raw: &str, raw_span: Span) -> R<Vec<SelectorArg>> {
    let base = raw_span.start as usize;
    let mut pieces: Vec<(usize, usize)> = Vec::new();
    let (mut depth, mut start, mut quote) = (0usize, 0usize, None::<char>);
    let mut chars = raw.char_indices();
    while let Some((i, c)) = chars.next() {
        match (quote, c) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '{' | '[' | '(') => depth += 1,
            (None, '}' | ']' | ')') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                pieces.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    pieces.push((start, raw.len()));

    let mut out = Vec::new();
    for (s, e) in pieces {
        let piece = &raw[s..e];
        let trimmed = piece.trim();
        if trimmed.is_empty() {
            continue; // empty selector `@e[]` or a trailing comma
        }
        let lead = piece.len() - piece.trim_start().len();
        let span = Span::new(base + s + lead, base + s + lead + trimmed.len());
        let (mut depth, mut quote, mut eq) = (0usize, None::<char>, None);
        for (i, c) in trimmed.char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '{' | '[' | '(') => depth += 1,
                (None, '}' | ']' | ')') => depth = depth.saturating_sub(1),
                (None, '=') if depth == 0 => {
                    eq = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(eq) = eq else {
            return Err(Diagnostic::new(format!("selector argument `{trimmed}` must be written `key=value`"), span));
        };
        let key = trimmed[..eq].trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Diagnostic::new(format!("invalid selector argument name `{key}`"), span));
        }
        let raw_value = &trimmed[eq + 1..];
        let value = raw_value.trim();
        let value_start = (base + s + lead + eq + 1 + (raw_value.len() - raw_value.trim_start().len())) as u32;
        let (value, holes) = extract_holes(value, value_start)?;
        out.push(SelectorArg { key: key.to_owned(), value, span, holes });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Program {
        parse(src).unwrap_or_else(|e| panic!("parse failed: {} at {:?}\n{src}", e.message, e.span))
    }

    fn err(src: &str) -> Diagnostic {
        match parse(src) {
            Ok(p) => panic!("expected an error, parsed {p:#?}"),
            Err(e) => e,
        }
    }

    fn func(src: &str) -> FnDecl {
        match ok(src).items.remove(0).kind {
            ItemKind::Fn(f) => f,
            other => panic!("{other:?}"),
        }
    }

    /// S-expression form of an expression, to assert precedence.
    fn sx(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => n.to_string(),
            ExprKind::Float(s) => s.clone(),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Str(s) => format!("{s:?}"),
            ExprKind::Ident(n) => n.clone(),
            ExprKind::List(v) => format!("[{}]", v.iter().map(sx).collect::<Vec<_>>().join(" ")),
            ExprKind::Compound(f) => format!("{{{}}}", f.iter().map(|f| format!("{}:{}", f.name, sx(&f.value))).collect::<Vec<_>>().join(" ")),
            ExprKind::Format(parts) => format!(
                "(fmt {})",
                parts
                    .iter()
                    .map(|p| match p {
                        FormatPart::Lit(s) => format!("{s:?}"),
                        FormatPart::Expr(e) => format!("{{{}}}", sx(e)),
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            ExprKind::Path { module, name } => format!("{}::{}", module.name, name.name),
            ExprKind::StructLit { module, name, fields } => format!("{}{}{{{}}}", module.as_ref().map_or(String::new(), |m| format!("{}::", m.name)), name.name, fields.iter().map(|f| format!("{}:{}", f.name, sx(&f.value))).collect::<Vec<_>>().join(" ")),
            ExprKind::Selector(s) => format!("@{}{:?}", s.var, s.args.iter().map(|a| format!("{}={}", a.key, a.value)).collect::<Vec<_>>()),
            ExprKind::Rel { kind, offset } => format!("{}{}", if *kind == super::super::token::RelKind::Tilde { "~" } else { "^" }, offset.clone().unwrap_or_default()),
            ExprKind::Unary { op, expr } => format!("({} {})", if *op == UnOp::Neg { "neg" } else { "not" }, sx(expr)),
            ExprKind::Binary { op, lhs, rhs } => format!("({op:?} {} {})", sx(lhs), sx(rhs)),
            ExprKind::Range { start, end, inclusive } => {
                format!("(range{} {} {})", if *inclusive { "=" } else { "" }, start.as_ref().map_or("_".into(), |e| sx(e)), end.as_ref().map_or("_".into(), |e| sx(e)))
            }
            ExprKind::Call { callee, args } => format!("(call {} {})", sx(callee), args.iter().map(sx).collect::<Vec<_>>().join(" ")),
            ExprKind::Field { base, name } => format!("(. {} {})", sx(base), name.name),
            ExprKind::Index { base, index } => format!("(idx {} {})", sx(base), sx(index)),
        }
    }

    fn expr_of(src: &str) -> String {
        let f = func(&format!("fn t() {{ {src}; }}"));
        match &f.body.stmts[0].kind {
            StmtKind::Expr(e) => sx(e),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fib() {
        let f = func("fn fib(n: int) -> int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }");
        assert_eq!(f.name.name, "fib");
        assert_eq!(f.params.len(), 1);
        assert_eq!(f.ret.as_ref().unwrap().name.name, "int");
        assert_eq!(f.body.stmts.len(), 2);
        match &f.body.stmts[1].kind {
            StmtKind::Return(Some(e)) => assert_eq!(sx(e), "(Add (call fib (Sub n 1)) (call fib (Sub n 2)))"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(expr_of("1 + 2 * 3"), "(Add 1 (Mul 2 3))");
        assert_eq!(expr_of("1 - 2 - 3"), "(Sub (Sub 1 2) 3)");
        assert_eq!(expr_of("(1 + 2) * 3"), "(Mul (Add 1 2) 3)");
        assert_eq!(expr_of("a || b && c"), "(Or a (And b c))");
        assert_eq!(expr_of("a < b && c == d"), "(And (Lt a b) (Eq c d))");
        assert_eq!(expr_of("-a * b"), "(Mul (neg a) b)");
        assert_eq!(expr_of("!a && b"), "(And (not a) b)");
        assert_eq!(expr_of("a % 2 == 0"), "(Eq (Rem a 2) 0)");
        assert_eq!(expr_of("- -3"), "(neg (neg 3))");
    }

    #[test]
    fn postfix_chains() {
        assert_eq!(expr_of("a.b.c"), "(. (. a b) c)");
        assert_eq!(expr_of("xs.push(1, 2)"), "(call (. xs push) 1 2)");
        assert_eq!(expr_of("kills[@s]"), "(idx kills @s[])");
        assert_eq!(expr_of("f(1)(2)"), "(call (call f 1) 2)");
        assert_eq!(expr_of("a[1][2].x"), "(. (idx (idx a 1) 2) x)");
        assert_eq!(expr_of("effect.give(@s, \"speed\", 10s)"), "(call (. effect give) @s[] \"speed\" 200)");
    }

    #[test]
    fn literals() {
        assert_eq!(expr_of("[1, 2, 3,]"), "[1 2 3]");
        // at the start of a statement `{` is a block, so a compound literal needs an expression context
        assert_eq!(expr_of("({ a: 1, \"b c\": [] })"), "{a:1 b c:[]}");
        let f = func("fn t() { let v = { a: 1 }; { let w = 2; } }");
        assert!(matches!(&f.body.stmts[0].kind, StmtKind::Let { init: Expr { kind: ExprKind::Compound(_), .. }, .. }));
        assert!(matches!(&f.body.stmts[1].kind, StmtKind::Block(_)));
        assert_eq!(expr_of("Point { x: 1, y: 2 }"), "Point{x:1 y:2}");
        assert_eq!(expr_of("Empty {}"), "Empty{}");
        assert_eq!(expr_of("1.5"), "1.5");
        assert_eq!(expr_of("true"), "true");
    }

    #[test]
    fn ranges() {
        assert_eq!(expr_of("0..10"), "(range 0 10)");
        assert_eq!(expr_of("0..=n + 1"), "(range= 0 (Add n 1))");
        assert_eq!(expr_of("..5"), "(range _ 5)");
        assert_eq!(expr_of("5.."), "(range 5 _)");
        assert!(err("fn t() { x = 1..=; }").message.contains("needs an end"));
    }

    #[test]
    fn comparisons_do_not_chain() {
        let e = err("fn t() { a < b < c; }");
        assert!(e.message.contains("cannot be chained"));
        assert!(e.hint.unwrap().contains("&&"));
    }

    #[test]
    fn selectors_and_coordinates() {
        assert_eq!(
            expr_of("@e[type=zombie, distance=..10, tag=!boss, nbt={a:[1,2]}, scores={kills=1..}]"),
            r#"@e["type=zombie", "distance=..10", "tag=!boss", "nbt={a:[1,2]}", "scores={kills=1..}"]"#
        );
        assert_eq!(expr_of("@e[name=\"a,b\"]"), r#"@e["name=\"a,b\""]"#);
        assert_eq!(expr_of("@a[]"), "@a[]");
        assert_eq!(expr_of("~-1"), "~-1");
        assert!(err("fn t() { @e[type]; }").message.contains("key=value"));
        assert!(err("fn t() { @e[=x]; }").message.contains("invalid selector argument name"));
    }

    #[test]
    fn selector_arg_spans_point_into_the_source() {
        let src = "fn t() { @e[ tag = x ]; }";
        let f = func(src);
        let StmtKind::Expr(Expr { kind: ExprKind::Selector(s), .. }) = &f.body.stmts[0].kind else { panic!() };
        let a = &s.args[0];
        assert_eq!(&src[a.span.start as usize..a.span.end as usize], "tag = x");
        assert_eq!((a.key.as_str(), a.value.as_str()), ("tag", "x"));
    }

    #[test]
    fn statements() {
        let f = func(
            "fn t() {
                let a = 1;
                let mut b: list<int> = [];
                b = [1];
                a += 2;
                p.x -= 1;
                xs[0] *= 3;
                while a < 3 { a += 1; }
                loop { break; }
                for i in 0..3 { continue; }
                { let c = 1; }
                if a == 1 { } else if a == 2 { } else { }
                return;
            }",
        );
        let kinds: Vec<&str> = f
            .body
            .stmts
            .iter()
            .map(|s| match &s.kind {
                StmtKind::Let { .. } => "let",
                StmtKind::Assign { .. } => "assign",
                StmtKind::While { .. } => "while",
                StmtKind::Loop { .. } => "loop",
                StmtKind::For { .. } => "for",
                StmtKind::Block(_) => "block",
                StmtKind::If { .. } => "if",
                StmtKind::Return(_) => "return",
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(kinds, ["let", "let", "assign", "assign", "assign", "assign", "while", "loop", "for", "block", "if", "return"]);
        match &f.body.stmts[1].kind {
            StmtKind::Let { mutable: true, ty: Some(ty), .. } => {
                assert_eq!(ty.name.name, "list");
                assert_eq!(ty.args[0].name.name, "int");
            }
            other => panic!("{other:?}"),
        }
        match &f.body.stmts[10].kind {
            StmtKind::If { els: Some(e), .. } => assert!(matches!(**e, Else::If(_))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn struct_literal_is_not_confused_with_a_block() {
        // `Point {` after `if` starts the body
        let f = func("fn t() { if x == y { z; } }");
        assert!(matches!(f.body.stmts[0].kind, StmtKind::If { .. }));
        let f = func("fn t() { while ready { z; } }");
        assert!(matches!(f.body.stmts[0].kind, StmtKind::While { .. }));
        // parentheses force a struct literal in a head
        assert_eq!(expr_of("(Point { x: 1 }).x"), "(. Point{x:1} x)");
        // `for i in 0.. {` : the open range stops before the body
        let f = func("fn t() { for i in 0.. { break; } }");
        match &f.body.stmts[0].kind {
            StmtKind::For { iter, .. } => assert_eq!(sx(iter), "(range 0 _)"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn context_blocks() {
        let f = func("fn t() { as @a[tag=ready] at @s positioned ~ ~1 ~ rotated 0 90 { say(\"x\"); } in \"minecraft:the_nether\" align xz anchored eyes { } facing 0 64 0 { } }");
        let ctx = |i: usize| match &f.body.stmts[i].kind {
            StmtKind::Context { clauses, body } => (clauses.clone(), body.clone()),
            other => panic!("{other:?}"),
        };
        let (c, body) = ctx(0);
        assert_eq!(c.len(), 4);
        assert!(matches!(&c[0].kind, CtxKind::As(e) if sx(e) == "@a[\"tag=ready\"]"));
        assert!(matches!(&c[1].kind, CtxKind::At(e) if sx(e) == "@s[]"));
        assert!(matches!(&c[2].kind, CtxKind::Positioned([x, y, z]) if sx(x) == "~" && sx(y) == "~1" && sx(z) == "~"));
        assert!(matches!(&c[3].kind, CtxKind::Rotated([a, b]) if sx(a) == "0" && sx(b) == "90"));
        assert_eq!(body.stmts.len(), 1);
        let (c, _) = ctx(1);
        assert!(matches!(&c[0].kind, CtxKind::In(_)));
        assert!(matches!(&c[1].kind, CtxKind::Align(a) if a.name == "xz"));
        assert!(matches!(&c[2].kind, CtxKind::Anchored(a) if a.name == "eyes"));
        let (c, _) = ctx(2);
        assert!(matches!(&c[0].kind, CtxKind::Facing(_)));
    }

    #[test]
    fn context_operands_stay_adjacent() {
        // without the call restriction `x (y)` would parse as a call
        let f = func("fn t() { positioned x (y + 1) z { } }");
        match &f.body.stmts[0].kind {
            StmtKind::Context { clauses, .. } => match &clauses[0].kind {
                CtxKind::Positioned([x, y, z]) => assert_eq!((sx(x), sx(y), sx(z)), ("x".into(), "(Add y 1)".into(), "z".into())),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        // negative coordinates are unary, not a subtraction
        let f = func("fn t() { positioned 0 64 -3 { } }");
        match &f.body.stmts[0].kind {
            StmtKind::Context { clauses, .. } => assert!(matches!(&clauses[0].kind, CtxKind::Positioned([_, _, z]) if sx(z) == "(neg 3)")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn items_and_attributes() {
        let p = ok("#[load] #[every(1s)] fn f() { }
                    global score: int = 0;
                    const MAX: int = 10;
                    struct Player { name: str, hp: int, items: list<str>, }
                    objective kills = \"playerKillCount\";");
        assert_eq!(p.items.len(), 5);
        let a = &p.items[0].attrs;
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].name.name, "load");
        assert_eq!(a[1].name.name, "every");
        assert_eq!(sx(&a[1].args[0]), "20");
        assert!(matches!(p.items[1].kind, ItemKind::Global(_)));
        assert!(matches!(p.items[2].kind, ItemKind::Const(_)));
        match &p.items[3].kind {
            ItemKind::Struct(s) => assert_eq!(s.fields.len(), 3),
            other => panic!("{other:?}"),
        }
        assert!(matches!(p.items[4].kind, ItemKind::Objective { .. }));
    }

    #[test]
    fn selector_options_can_use_variables() {
        let p = ok(r#"fn f() { kill(@e[tag=$t, distance=..${r * 2}, scores={kills=$n..}, name="a$$b", type=zombie]); }"#);
        let ItemKind::Fn(f) = &p.items[0].kind else { panic!() };
        let StmtKind::Expr(call) = &f.body.stmts[0].kind else { panic!() };
        let ExprKind::Call { args, .. } = &call.kind else { panic!() };
        let ExprKind::Selector(sel) = &args[0].kind else { panic!("{:?}", args[0]) };
        let values: Vec<(&str, &str, usize)> = sel.args.iter().map(|a| (a.key.as_str(), a.value.as_str(), a.holes.len())).collect();
        assert_eq!(
            values,
            [("tag", "$(h0)", 1), ("distance", "..$(h0)", 1), ("scores", "{kills=$(h0)..}", 1), ("name", "\"a$b\"", 0), ("type", "zombie", 0)]
        );
        assert_eq!(sx(&sel.args[1].holes[0]), "(Mul r 2)");
        // the hole expression keeps its position in the file
        let src = "fn f() { kill(@e[tag=$t]); }";
        let ExprKind::Selector(sel) = &(match &ok(src).items[0].kind { ItemKind::Fn(f) => match &f.body.stmts[0].kind { StmtKind::Expr(c) => match &c.kind { ExprKind::Call { args, .. } => args[0].clone(), _ => panic!() }, _ => panic!() }, _ => panic!() }).kind else { panic!() };
        let hole = &sel.args[0].holes[0];
        assert_eq!(&src[hole.span.start as usize..hole.span.end as usize], "t");
        assert!(err("fn f() { kill(@e[tag=$]); }").message.contains("followed by a name"));
        assert!(err("fn f() { kill(@e[tag=${}]); }").message.contains("empty `${}`"));
        assert!(err("fn f() { kill(@e[tag=${x]); }").message.contains("never closed"));
    }

    #[test]
    fn imports_and_visibility() {
        let p = ok("import a; import a::b::c; import a::b as x; import util::{clamp, Point,}; pub fn f() {} pub struct S {} pub const K: int = 1;");
        let imp = |i: usize| match &p.items[i].kind {
            ItemKind::Import(d) => d.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(imp(0).path.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["a"]);
        assert_eq!(imp(1).path.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(imp(2).alias.unwrap().name, "x");
        assert_eq!(imp(3).items.unwrap().iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["clamp", "Point"]);
        assert!(!p.items[0].public && p.items[4].public && p.items[5].public && p.items[6].public);
        assert!(err("pub import a;").message.contains("cannot be `pub`"));
        assert!(err("import a::{};").message.contains("at least one item"));
        assert!(err("import a::{b} as c;").message.contains("cannot be combined"));
        assert!(err("import ;").message.contains("a module name"));
        assert!(err("import a").message.contains("expected `;`"));
    }

    #[test]
    fn qualified_names() {
        assert_eq!(expr_of("util::clamp(1, 2)"), "(call util::clamp 1 2)");
        assert_eq!(expr_of("util::Point { x: 1 }"), "util::Point{x:1}");
        assert_eq!(expr_of("util::LIMIT + 1"), "(Add util::LIMIT 1)");
        let f = func("fn f(p: util::Point, q: list<util::Point>) -> util::Point { return p; }");
        assert_eq!(f.params[0].ty.module.as_ref().unwrap().name, "util");
        assert_eq!(f.params[1].ty.args[0].module.as_ref().unwrap().name, "util");
        assert_eq!(f.ret.unwrap().name.name, "Point");
        // `module::Name {` is not a struct literal in a statement head
        let f = func("fn f() { if a::b { } }");
        assert!(matches!(f.body.stmts[0].kind, StmtKind::If { .. }));
        assert!(err("fn f() { a::; }").message.contains("after `::`"));
        // qualified globals and objectives are assignable
        let f = func("fn f() { util::counter += 1; util::points[@s] = 2; util::cfg.x = 3; }");
        assert_eq!(f.body.stmts.len(), 3);
        assert!(err("fn f() { util::g() = 1; }").message.contains("cannot assign"));
    }

    #[test]
    fn f_strings() {
        assert_eq!(expr_of(r#"f"x = {x}, y = {a + 1}!""#), r#"(fmt "x = " {x} ", y = " {(Add a 1)} "!")"#);
        assert_eq!(expr_of(r#"f"{p.name} has {xs.len()} items {{ok}}""#), r#"(fmt {(. p name)} " has " {(call (. xs len) )} " items {ok}")"#);
        assert_eq!(expr_of(r#"f"{a == "s"}""#), r#"(fmt {(Eq a "s")})"#);
        assert!(expr_of(r#"f"{@a[tag=x].len}""#).contains("@a"));
        // errors inside a hole point at the real position in the file
        let src = r#"fn f() { print(f"v = {1 + }"); }"#;
        let e = err(src);
        assert!(e.message.contains("expected an expression"), "{}", e.message);
        // the caret lands on the closing brace of the hole (a zero-width end-of-input span)
        assert_eq!(e.span.start as usize, src.find('}').unwrap());
        assert!(err(r#"fn f() { print(f"{}"); }"#).message.contains("empty `{}`"));
        assert!(err(r#"fn f() { print(f"{a b}"); }"#).message.contains("end of the expression"));
        assert!(err(r#"fn f() { print(f"{1 +}"); }"#).message.contains("expression"));
    }

    #[test]
    fn error_messages() {
        let e = err("fn f() { return 1 }");
        assert_eq!(e.message, "expected `;`, found `}`");
        // points right after `1`, not at the `}`
        assert_eq!((e.span.start, e.span.end), (17, 18));

        assert!(err("fn f( { }").message.contains("expected a name"));
        assert!(err("fn f() { let = 1; }").message.contains("a variable name"));
        assert!(err("fn f() { let fn = 1; }").message.contains("keyword `fn`"));
        assert!(err("fn f() { 1 + ; }").message.contains("expected an expression, found `;`"));
        assert!(err("fn f() { 1 = 2; }").message.contains("cannot assign"));
        assert!(err("fn f() { f() = 2; }").message.contains("cannot assign"));
        assert!(err("fn f() {").message.contains("unclosed block"));
        assert!(err("x").message.contains("expected an item"));
        assert!(err("pub x").message.contains("expected an item"));
        assert!(err("fn f() { as @a ; }").message.contains("expected `{`"));
        assert!(err("#[ ] fn f() {}").message.contains("attribute name"));
        assert!(err("#[load]").message.contains("expected an item"));
        assert!(err("fn f() { if { } }").message.contains("expected an expression"));
    }

    #[test]
    fn spans_cover_the_construct() {
        let src = "fn f() { let x = 1 + 2; }";
        let f = func(src);
        let s = &f.body.stmts[0];
        assert_eq!(&src[s.span.start as usize..s.span.end as usize], "let x = 1 + 2;");
    }
}
