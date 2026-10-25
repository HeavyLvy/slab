//! Semantic tokens: the one place that classifies slab text for highlighting. The runtime
//! token collection and the generated TextMate grammar both derive from the legend here, so
//! they cannot disagree (a drift test regenerates the grammar and compares).

use std::collections::HashSet;

use slab::diag::Span;
use slab::syntax::token::Tok;

use crate::analysis::{self, Hit, Role};
use crate::defs::DefKind;

/// Token types, in legend order (all standard, so every theme colors them).
pub const TOKEN_TYPES: &[&str] = &[
    "keyword",
    "string",
    "number",
    "operator",
    "function",
    "method",
    "variable",
    "parameter",
    "property",
    "type",
    "namespace",
    "decorator",
];
/// Token modifiers, in legend order (all standard).
pub const TOKEN_MODIFIERS: &[&str] = &["declaration", "definition", "static", "readonly", "defaultLibrary"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ST {
    Keyword,
    Str,
    Number,
    Operator,
    Function,
    Method,
    Var,
    Param,
    Prop,
    Type,
    Namespace,
    Decorator,
}

impl ST {
    /// Index into [`TOKEN_TYPES`].
    pub fn index(self) -> u32 {
        match self {
            ST::Keyword => 0,
            ST::Str => 1,
            ST::Number => 2,
            ST::Operator => 3,
            ST::Function => 4,
            ST::Method => 5,
            ST::Var => 6,
            ST::Param => 7,
            ST::Prop => 8,
            ST::Type => 9,
            ST::Namespace => 10,
            ST::Decorator => 11,
        }
    }

    /// The TextMate scopes that give close colors before the server is running.
    pub fn scopes(self) -> &'static [&'static str] {
        match self {
            ST::Keyword => &["keyword.slab"],
            ST::Str => &["string.quoted.double.slab"],
            ST::Number => &["constant.numeric.slab"],
            ST::Operator => &["keyword.operator.slab"],
            ST::Function => &["entity.name.function.slab", "support.function.slab"],
            ST::Method => &["entity.name.function.slab", "support.function.slab"],
            ST::Var => &["variable.other.slab"],
            ST::Param => &["variable.parameter.slab"],
            ST::Prop => &["variable.other.property.slab"],
            ST::Type => &["entity.name.type.slab"],
            ST::Namespace => &["entity.name.namespace.slab"],
            ST::Decorator => &["entity.name.decorator.slab"],
        }
    }
}

pub const M_DECLARATION: u32 = 1;
pub const M_DEFINITION: u32 = 2;
pub const M_STATIC: u32 = 4;
pub const M_READONLY: u32 = 8;
pub const M_DEFAULT_LIBRARY: u32 = 16;

/// A semantic token, positioned by byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemTok {
    pub start: usize,
    pub len: usize,
    pub ty: ST,
    pub mods: u32,
}

/// All semantic tokens of one parsed module, in source order.
pub fn document_tokens(text: &str, program: &slab::syntax::ast::Program) -> Vec<SemTok> {
    let mut out: Vec<SemTok> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    let holes = format_holes(program);
    let mut idents = Vec::new();
    analysis::each_ident(program, &mut idents);
    let whole = Span::new(0, text.len());
    collect_region(text, whole, &holes, &idents, &mut out, &mut seen);
    out.sort_by_key(|t| (t.start, t.len));
    // Never hand VS Code an overlapping token: one bad token makes a client discard the
    // whole array, so the later of two overlapping tokens is dropped instead.
    let mut compacted: Vec<SemTok> = Vec::with_capacity(out.len());
    let mut end = 0usize;
    for t in out {
        if t.start < end {
            continue;
        }
        end = t.start + t.len;
        compacted.push(t);
    }
    compacted
}

// ------------------------------------------------------------------ f-string holes

/// One f-string hole: the span of the whole `f"..."` literal and of a `{expr}` inside it.
#[derive(Clone, Copy, Debug)]
pub struct Hole {
    pub outer: Span,
    pub inner: Span,
}

/// Every f-string hole in a program, recursively (holes can contain f-strings).
pub fn format_holes(program: &slab::syntax::ast::Program) -> Vec<Hole> {
    let mut holes = Vec::new();
    let mut sink = |e: &slab::syntax::ast::Expr| {
        if let slab::syntax::ast::ExprKind::Format(parts) = &e.kind {
            for p in parts {
                if let slab::syntax::ast::FormatPart::Expr(inner) = p {
                    holes.push(Hole { outer: e.span, inner: inner.span });
                }
            }
        }
    };
    analysis::each_expr(program, &mut sink);
    holes
}

// ------------------------------------------------------------------ collection

/// Lexed tokens of one region, with the identifier roles found inside it.
fn collect_region(
    src: &str,
    region: Span,
    holes: &[Hole],
    idents: &[Hit],
    out: &mut Vec<SemTok>,
    seen: &mut HashSet<usize>,
) {
    let base = region.start as usize;
    let end = (region.end as usize).min(src.len());
    let region_src = &src[base.min(end)..end];
    for tok in lex_lenient(region_src) {
        let span = Span::new(base + tok.span.start as usize, base + tok.span.end as usize);
        match &tok.tok {
            Tok::Kw(_) => emit(out, seen, span, ST::Keyword, 0),
            Tok::Str(_) => emit(out, seen, span, ST::Str, 0),
            Tok::Int(_) | Tok::Float(_) => emit(out, seen, span, ST::Number, 0),
            // Selectors and relative coordinates stay with the TextMate grammar.
            Tok::Selector { .. } | Tok::Rel { .. } => {}
            Tok::FStr(_) => fstring(src, span, holes, idents, out, seen),
            Tok::Ident(_) => {
                let (ty, mods) = ident_token(role_of(idents, span), &src[span.start as usize..span.end as usize]);
                emit(out, seen, span, ty, mods);
            }
            Tok::Eof => {}
            other if is_operator(other) => emit(out, seen, span, ST::Operator, 0),
            _ => {}
        }
    }
}

/// An f-string: literal parts as `string`, one brace pair and hole content per hole.
fn fstring(src: &str, outer: Span, holes: &[Hole], idents: &[Hit], out: &mut Vec<SemTok>, seen: &mut HashSet<usize>) {
    let start = outer.start as usize;
    let close = (outer.end as usize).saturating_sub(1); // the closing quote
    let region_holes: Vec<&Hole> = holes.iter().filter(|h| h.outer == outer).collect();
    let mut region_holes = region_holes;
    region_holes.sort_by_key(|h| h.inner.start);
    // Literal text up to each hole's `{` (after `f"`), then the hole, then on.
    let mut cursor = start + 2; // past `f"`
    for h in &region_holes {
        let hole_start = h.inner.start as usize;
        if hole_start > cursor {
            emit(out, seen, Span::new(cursor, hole_start), ST::Str, 0);
        }
        collect_region(src, h.inner, holes, idents, out, seen);
        cursor = (h.inner.end as usize).min(close);
    }
    if close > cursor {
        emit(out, seen, Span::new(cursor, close), ST::Str, 0);
    }
}

/// The role of an occurrence, by the AST's own span for it (smallest containing span).
fn role_of(idents: &[Hit], span: Span) -> Option<&Hit> {
    idents
        .iter()
        .filter(|h| analysis::contains(h.span, span.start as usize))
        .min_by_key(|h| h.span.end - h.span.start)
}

/// Whether the standard library has a function of this name (marked "default library").
fn is_std_function(name: &str) -> bool {
    static NAMES: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| slab::stdlib::function_names().into_iter().map(|(_, n)| n).collect()).contains(name)
}

/// (token type, modifiers) for an identifier occurrence; broken files degrade to variables.
pub fn ident_token(hit: Option<&Hit>, name: &str) -> (ST, u32) {
    let Some(hit) = hit else { return (ST::Var, 0) };
    match hit.role {
        Role::Decl(kind) => decl_token_for(kind),
        Role::StructFieldName | Role::FieldInitName => (ST::Prop, if hit.role == Role::FieldInitName { 0 } else { M_DECLARATION }),
        Role::Param => (ST::Param, M_DECLARATION),
        Role::LetName => (ST::Var, M_DECLARATION),
        Role::Callee => (ST::Function, if is_std_function(name) { M_DEFAULT_LIBRARY } else { 0 }),
        Role::MethodName => (ST::Method, 0),
        Role::FieldName => (ST::Prop, 0),
        Role::TypeName => (ST::Type, 0),
        Role::ModuleName | Role::ImportModule => (ST::Namespace, 0),
        Role::PathItem => (ST::Function, 0),
        Role::ImportItem => (ST::Prop, M_DECLARATION),
        Role::AttrName => (ST::Decorator, M_DECLARATION),
        Role::Plain => (ST::Var, 0),
    }
}

/// (token type, modifiers) for a top-level declaration's own name.
pub fn decl_token_for(kind: DefKind) -> (ST, u32) {
    match kind {
        DefKind::Fn => (ST::Function, M_DECLARATION | M_DEFINITION),
        DefKind::Global => (ST::Var, M_STATIC | M_DECLARATION),
        DefKind::Const => (ST::Var, M_STATIC | M_READONLY | M_DECLARATION),
        DefKind::Struct | DefKind::Enum => (ST::Type, M_DECLARATION | M_DEFINITION),
        DefKind::Objective => (ST::Prop, M_DECLARATION),
        DefKind::Import => (ST::Namespace, M_DECLARATION),
    }
}

/// Whether a token is punctuation that highlights as an operator (the structural ones are
/// left to the theme).
fn is_operator(tok: &Tok) -> bool {
    use Tok::*;
    matches!(tok, Eq | EqEq | NotEq | Lt | Le | Gt | Ge | Plus | Minus | Star | Slash | Percent | PlusEq | MinusEq | StarEq | SlashEq | PercentEq | AndAnd | OrOr | Arrow | DotDot | DotDotEq | Bang | ColonColon)
}

fn emit(out: &mut Vec<SemTok>, seen: &mut HashSet<usize>, span: Span, ty: ST, mods: u32) {
    let start = span.start as usize;
    if start >= span.end as usize || !seen.insert(start) {
        return;
    }
    out.push(SemTok { start, len: (span.end - span.start) as usize, ty, mods });
}

/// A lex with error recovery: `lex` stops at the first bad character, so anything after it
/// would lose highlighting. Retry from the end of the offending line.
pub fn lex_lenient(src: &str) -> Vec<slab::syntax::token::Token> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos <= src.len() {
        match slab::syntax::lexer::lex(&src[pos..]) {
            Ok(mut tokens) => {
                tokens.pop(); // Eof
                for t in &mut tokens {
                    t.span.start += pos as u32;
                    t.span.end += pos as u32;
                }
                out.extend(tokens);
                break;
            }
            Err(d) => {
                let bad = d.span.start as usize + pos;
                if let Ok(mut ok) = slab::syntax::lexer::lex(&src[pos..bad.min(src.len())]) {
                    ok.pop();
                    for t in &mut ok {
                        t.span.start += pos as u32;
                        t.span.end += pos as u32;
                    }
                    out.extend(ok);
                }
                let next = src[bad.min(src.len())..].find('\n').map_or(src.len(), |i| bad.min(src.len()) + i + 1);
                pos = next.max(bad + 1);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use slab::syntax::parse;

    fn valid(text: &str) {
        let file = slab::diag::SourceFile::new("t.slab", text);
        let program = parse(&file.text).unwrap();
        let toks = document_tokens(text, &program);
        let mut prev_end = 0usize;
        for t in &toks {
            assert!(t.start >= prev_end, "overlap at {}: {:?}", t.start, &text[t.start..t.start + t.len]);
            assert!(t.len > 0);
            assert!(t.ty.index() < (TOKEN_TYPES.len() as u32), "type index out of legend: {:?}", t.ty);
            // any used modifier bit must exist in the legend
            for bit in [M_DECLARATION, M_DEFINITION, M_STATIC, M_READONLY, M_DEFAULT_LIBRARY] {
                if t.mods & bit != 0 {
                    assert!(TOKEN_MODIFIERS.contains(&["declaration", "definition", "static", "readonly", "defaultLibrary"][bit.trailing_zeros() as usize]));
                }
            }
            prev_end = t.start + t.len;
        }
    }

    #[test]
    fn tokens_never_overlap() {
        valid("import util;\n\n#[load]\nfn boot() {\n    let s = \"a\\\"b\";\n    print(f\"x {s} y\");\n    as @a at @s {\n        effect.give(@s, \"minecraft:speed\", 10s);\n    }\n}");
        // the e2e project for good measure
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/projects/e2e/main.slab");
        let text = std::fs::read_to_string(path).unwrap();
        valid(&text);
    }

    #[test]
    fn calls_are_function_tokens_even_without_a_server() {
        let src = "fn f() {}\nfn g() {\n    f();\n}\n";
        let program = parse(src).unwrap();
        let toks = document_tokens(src, &program);
        let call = toks.iter().find(|t| t.start == src.find("f();").unwrap()).unwrap();
        assert_eq!(call.ty, ST::Function);
        // a keyword token is what the theme's default rules expect too
        let kw = toks.iter().find(|t| t.start == src.find("fn g").unwrap()).unwrap();
        assert_eq!(kw.ty, ST::Keyword);
    }
}