//! `slab fmt`: the canonical layout of slab source.
//!
//! The formatter works on the token stream and the text between tokens, not on the syntax tree, so
//! comments and the line breaks you chose survive. It fixes the things there is one right answer
//! for: spaces around tokens, 4-space indentation (one level per line that opens a bracket, one
//! more for a line that continues a statement), `{` on the line of its header, `} else`, blank
//! lines (at most one, none after `{` or before `}`), trailing whitespace and the final newline.
//! It does not wrap long lines or join short ones.
//!
//! Two checks run on every result, so a bug in here can never change a program: only whitespace may
//! differ from the input, and the output must lex to the same tokens.

use crate::diag::{Diagnostic, Span};
use crate::syntax::lexer::lex_tolerant;
use crate::syntax::token::{FPart, Kw, Tok, Token};

const INDENT: &str = "    ";

/// The formatted source, or the first syntax error (code that does not parse is not touched).
pub fn format(src: &str) -> Result<String, Diagnostic> {
    let src = src.replace("\r\n", "\n");
    crate::syntax::parse(&src)?;
    let (tokens, _) = lex_tolerant(&src);
    let out = Formatter::new(&src, &tokens).run();
    check(&src, &out)?;
    Ok(out)
}

/// Only whitespace may have changed, and the tokens must be the same.
fn check(src: &str, out: &str) -> Result<(), Diagnostic> {
    let bare = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let toks = |s: &str| {
        lex_tolerant(s)
            .0
            .into_iter()
            .map(|t| position_free(t.tok))
            .collect::<Vec<_>>()
    };
    if bare(src) != bare(out) || toks(src) != toks(out) || crate::syntax::parse(out).is_err() {
        return Err(Diagnostic::new(
            "the formatter would have changed the program, so it left it alone (this is a bug in `slab fmt`)",
            Span::new(0, 0),
        ));
    }
    Ok(())
}

/// The token without the source offsets some tokens carry (a selector's argument text, an
/// f-string's `{expression}`), which move when whitespace does.
fn position_free(t: Tok) -> Tok {
    match t {
        Tok::Selector { var, args } => Tok::Selector {
            var,
            args: args.map(|(text, _)| (text, Span::new(0, 0))),
        },
        Tok::FStr(parts) => Tok::FStr(
            parts
                .into_iter()
                .map(|p| match p {
                    FPart::Expr { src, .. } => FPart::Expr { src, start: 0 },
                    lit => lit,
                })
                .collect(),
        ),
        t => t,
    }
}

struct Comment<'a> {
    /// Line breaks between the previous thing and this comment.
    nl_before: usize,
    /// The spaces between the previous thing and this comment, when on the same line.
    ws_before: String,
    text: &'a str,
    line: bool,
}

/// The comments in the text between two tokens, and the line breaks after the last of them.
fn scan(gap: &str) -> (Vec<Comment<'_>>, usize) {
    let b = gap.as_bytes();
    let (mut comments, mut nl, mut ws, mut i) = (Vec::new(), 0, String::new(), 0);
    while i < b.len() {
        match b[i] {
            b'\n' => {
                nl += 1;
                ws.clear();
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = gap[i..].find('\n').map_or(gap.len(), |n| i + n);
                comments.push(Comment {
                    nl_before: nl,
                    ws_before: std::mem::take(&mut ws),
                    text: gap[i..end].trim_end(),
                    line: true,
                });
                nl = 0;
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let (mut depth, mut j) = (1, i + 2);
                while depth > 0 && j < b.len() {
                    if gap[j..].starts_with("/*") {
                        depth += 1;
                        j += 2;
                    } else if gap[j..].starts_with("*/") {
                        depth -= 1;
                        j += 2;
                    } else {
                        j += 1;
                    }
                }
                comments.push(Comment {
                    nl_before: nl,
                    ws_before: std::mem::take(&mut ws),
                    text: &gap[i..j],
                    line: false,
                });
                nl = 0;
                i = j;
            }
            b'\t' => {
                ws.push_str(INDENT);
                i += 1;
            }
            c => {
                ws.push(c as char);
                i += 1;
            }
        }
    }
    (comments, nl)
}

struct Entry {
    /// Indent of the line the bracket was opened on.
    line_indent: usize,
    /// `#[` .. `]`: the line after it starts a new statement.
    attr: bool,
}

struct Formatter<'a> {
    src: &'a str,
    toks: &'a [Token],
    /// `<` and `>` that open and close a type's arguments (`list<int>`), not comparisons.
    generic: Vec<bool>,
    /// Tokens of an `import ...;` statement, whose braces hug their contents: `{Pair, square}`.
    import: Vec<bool>,
    out: String,
    stack: Vec<Entry>,
    /// Indent of the line being written.
    line_indent: usize,
    /// The previous token ends a statement or opens a bracket, so the next line starts afresh
    /// instead of continuing it.
    fresh: bool,
}

impl<'a> Formatter<'a> {
    fn new(src: &'a str, toks: &'a [Token]) -> Self {
        Formatter {
            src,
            toks,
            generic: generics(src, toks),
            import: imports(toks),
            out: String::new(),
            stack: Vec::new(),
            line_indent: 0,
            fresh: true,
        }
    }

    fn run(mut self) -> String {
        for c in 0..self.toks.len() {
            let p = c.checked_sub(1);
            self.gap(p, c);
            if self.toks[c].tok != Tok::Eof {
                self.token(c);
            }
        }
        let trimmed = self.out.trim_end();
        if trimmed.is_empty() {
            String::new()
        } else {
            format!("{trimmed}\n")
        }
    }

    fn tok(&self, i: usize) -> &Tok {
        &self.toks[i].tok
    }

    fn text(&self, i: usize) -> &'a str {
        let s = self.toks[i].span;
        &self.src[s.start as usize..s.end as usize]
    }

    /// `-` with nothing to subtract from before it.
    fn unary(&self, i: usize) -> bool {
        match i.checked_sub(1) {
            None => true,
            Some(p) => {
                !(matches!(
                    self.tok(p),
                    Tok::Ident(_)
                        | Tok::Int(_)
                        | Tok::Float(_)
                        | Tok::Str(_)
                        | Tok::FStr(_)
                        | Tok::Selector { .. }
                        | Tok::Rel { .. }
                        | Tok::RParen
                        | Tok::RBracket
                        | Tok::Kw(Kw::True | Kw::False)
                ) || (matches!(self.tok(p), Tok::Gt) && self.generic[p]))
            }
        }
    }

    /// Whether a space goes between two neighbouring tokens on one line.
    fn space(&self, p: usize, c: usize) -> bool {
        let (a, b) = (self.tok(p), self.tok(c));
        if matches!(
            b,
            Tok::RParen
                | Tok::RBracket
                | Tok::Comma
                | Tok::Semi
                | Tok::Dot
                | Tok::DotDot
                | Tok::DotDotEq
                | Tok::ColonColon
                | Tok::Colon
        ) {
            return false;
        }
        if matches!(b, Tok::Lt | Tok::Gt) && self.generic[c] {
            return false;
        }
        if matches!(
            a,
            Tok::LParen
                | Tok::LBracket
                | Tok::Dot
                | Tok::DotDot
                | Tok::DotDotEq
                | Tok::ColonColon
                | Tok::Bang
                | Tok::Hash
        ) {
            return false;
        }
        if (matches!(a, Tok::Lt) && self.generic[p]) || (matches!(a, Tok::Minus) && self.unary(p)) {
            return false;
        }
        if self.import[p] && self.import[c] && (matches!(a, Tok::LBrace) || matches!(b, Tok::RBrace)) {
            return false;
        }
        match (a, b) {
            (_, Tok::LBrace) => return true,
            (Tok::LBrace, _) => return !matches!(b, Tok::RBrace),
            (_, Tok::RBrace) => return true,
            _ => {}
        }
        if matches!(b, Tok::LParen | Tok::LBracket) {
            // a call or an index follows its subject closely; after anything else it opens a group
            return !(matches!(a, Tok::Ident(_) | Tok::RParen | Tok::RBracket | Tok::Str(_))
                || (matches!(a, Tok::Gt) && self.generic[p]));
        }
        true
    }

    fn indent_for(&self, c: usize, as_content: bool) -> usize {
        let b = self.tok(c);
        let closer = matches!(b, Tok::RParen | Tok::RBracket | Tok::RBrace);
        if closer && !as_content {
            return self.stack.last().map_or(0, |e| e.line_indent);
        }
        let base = self.stack.last().map_or(0, |e| e.line_indent + 1);
        let continues = !closer && !matches!(b, Tok::Eof) && (!self.fresh || matches!(b, Tok::Dot));
        base + usize::from(continues)
    }

    fn newline(&mut self, blank: bool, indent: usize) {
        let trimmed = self.out.trim_end_matches(' ').len();
        self.out.truncate(trimmed);
        self.out.push('\n');
        if blank {
            self.out.push('\n');
        }
        self.out.push_str(&INDENT.repeat(indent));
        self.line_indent = indent;
    }

    /// Writes what separates token `c` from the one before it: spaces, line breaks, comments.
    fn gap(&mut self, p: Option<usize>, c: usize) {
        let start = p.map_or(0, |p| self.toks[p].span.end as usize);
        let (comments, trailing_nl) = scan(&self.src[start..self.toks[c].span.start as usize]);
        let b = self.tok(c).clone();
        let closer = matches!(b, Tok::RParen | Tok::RBracket | Tok::RBrace);
        let eof = b == Tok::Eof;
        let after_opener =
            p.is_some_and(|p| matches!(self.tok(p), Tok::LParen | Tok::LBracket | Tok::LBrace));
        let mut broke = false;
        for (k, cm) in comments.iter().enumerate() {
            if cm.nl_before == 0 && p.is_some() && !broke {
                // on the line of the token before it
                let mut ws = cm.ws_before.clone();
                if ws.is_empty() && !(after_opener && !cm.line) {
                    ws.push(' ');
                }
                self.out.push_str(&ws);
            } else if p.is_some() || k > 0 {
                let blank = cm.nl_before >= 2 && !(k == 0 && after_opener);
                let ind = self.indent_for(c, true);
                self.newline(blank, ind);
            }
            self.out.push_str(cm.text);
            broke = cm.line;
        }
        if eof {
            return;
        }
        let join = trailing_nl >= 1
            && comments.is_empty()
            && p.is_some_and(|p| {
                (matches!(b, Tok::LBrace) && !self.fresh)
                    || (matches!(b, Tok::Kw(Kw::Else)) && matches!(self.tok(p), Tok::RBrace))
            });
        if (trailing_nl == 0 || join) && !broke {
            match p {
                Some(p) if comments.is_empty() => {
                    if self.space(p, c) {
                        self.out.push(' ');
                    }
                }
                Some(_) if !matches!(b, Tok::RParen | Tok::RBracket | Tok::Comma | Tok::Semi) => self.out.push(' '),
                _ => {}
            }
        } else if p.is_some() || !comments.is_empty() {
            let blank = trailing_nl >= 2 && !closer && !(comments.is_empty() && after_opener);
            let ind = self.indent_for(c, false);
            self.newline(blank, ind);
        }
    }

    fn token(&mut self, c: usize) {
        let tok = self.tok(c).clone();
        if matches!(tok, Tok::RParen | Tok::RBracket | Tok::RBrace) {
            let at_line_start = self.out.ends_with('\n')
                || self.out[self.out.rfind('\n').map_or(0, |n| n + 1)..]
                    .trim()
                    .is_empty();
            let entry = self.stack.pop();
            if at_line_start {
                self.line_indent = entry.as_ref().map_or(0, |e| e.line_indent);
            }
            self.fresh = matches!(tok, Tok::RBrace) || entry.is_some_and(|e| e.attr);
        } else {
            self.fresh = matches!(
                tok,
                Tok::LParen | Tok::LBracket | Tok::LBrace | Tok::Semi | Tok::Comma
            );
        }
        self.out.push_str(self.text(c));
        if matches!(tok, Tok::LParen | Tok::LBracket | Tok::LBrace) {
            let attr =
                matches!(tok, Tok::LBracket) && c > 0 && matches!(self.tok(c - 1), Tok::Hash);
            self.stack.push(Entry {
                line_indent: self.line_indent,
                attr,
            });
        }
    }
}

/// Marks the tokens from `import` to its `;`.
fn imports(toks: &[Token]) -> Vec<bool> {
    let mut inside = false;
    toks.iter()
        .map(|t| {
            inside |= t.tok == Tok::Kw(Kw::Import);
            let now = inside;
            inside &= t.tok != Tok::Semi;
            now
        })
        .collect()
}

/// Marks the `<` and `>` of type arguments: `list<int>`, `Option<Option<int>>`, `impl<T>`,
/// `enum Phase<T>`. A name that is capitalised (or `list`) followed straight away by `<` and a
/// matching `>` around nothing but type names is a type; `a < b` stays a comparison.
fn generics(src: &str, toks: &[Token]) -> Vec<bool> {
    let mut g = vec![false; toks.len()];
    for i in 1..toks.len() {
        if toks[i].tok != Tok::Lt {
            continue;
        }
        let adjacent = toks[i - 1].span.end == toks[i].span.start;
        let named = match &toks[i - 1].tok {
            Tok::Ident(n) => {
                let _ = src;
                adjacent
                    && (n == "list"
                        || (n.starts_with(|c: char| c.is_ascii_uppercase())
                            && !(n.len() > 1
                                && n.chars().all(|c| {
                                    c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'
                                }))))
            }
            Tok::Kw(Kw::Impl) => true,
            Tok::ColonColon => adjacent,
            _ => false,
        };
        if !named {
            continue;
        }
        let mut depth = 0usize;
        for (j, t) in toks.iter().enumerate().skip(i) {
            match &t.tok {
                Tok::Lt => depth += 1,
                Tok::Gt => {
                    depth -= 1;
                    if depth == 0 {
                        g[i] = true;
                        g[j] = true;
                        break;
                    }
                }
                Tok::Ident(_)
                | Tok::Comma
                | Tok::ColonColon
                | Tok::LParen
                | Tok::RParen
                | Tok::Kw(Kw::Const) => {}
                _ => break,
            }
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str) -> String {
        format(src).unwrap_or_else(|e| panic!("{}: {src}", e.message))
    }

    #[test]
    fn spaces_and_indentation() {
        assert_eq!(
            fmt("fn  f( a:int,b : int )->int{return a+b*-2;}"),
            "fn f(a: int, b: int) -> int { return a + b * -2; }\n"
        );
        assert_eq!(
            fmt("fn f(){\nlet x=1;\n   if x==1{\nsay(\"a\");\n}else{\nsay(\"b\");\n}\n}"),
            "fn f() {\n    let x = 1;\n    if x == 1 {\n        say(\"a\");\n    } else {\n        say(\"b\");\n    }\n}\n"
        );
    }

    #[test]
    fn braces_join_their_header_and_else_joins_the_brace() {
        assert_eq!(
            fmt("fn f()\n{\n  if true\n  {\n  }\n  else\n  {\n  }\n}"),
            "fn f() {\n    if true {\n    } else {\n    }\n}\n"
        );
    }

    #[test]
    fn blank_lines_are_kept_once_and_dropped_inside_braces() {
        assert_eq!(
            fmt("fn a() {}\n\n\n\nfn b() {\n\n  say(\"x\");\n\n\n  say(\"y\");\n\n}\n"),
            "fn a() {}\n\nfn b() {\n    say(\"x\");\n\n    say(\"y\");\n}\n"
        );
    }

    #[test]
    fn comments_stay_where_they_are() {
        let src = "// header\nfn f() { // after the brace\n  // own line\n  say(\"a\"); // trailing\n\n  /* block */ say(\"b\");\n  // last\n}\n// end\n";
        assert_eq!(
            fmt(src),
            "// header\nfn f() { // after the brace\n    // own line\n    say(\"a\"); // trailing\n\n    /* block */ say(\"b\");\n    // last\n}\n// end\n"
        );
    }

    #[test]
    fn types_and_comparisons() {
        assert_eq!(
            fmt("fn f(a:list<int >,b:Option<list<int>>)->Result<int,str>{if a<b&&c>d{}}"),
            "fn f(a: list<int>, b: Option<list<int>>) -> Result<int, str> { if a < b && c > d {} }\n"
        );
        assert_eq!(
            fmt("enum Phase<T>{Waiting,Running(T)}impl<T> Phase<T>{}"),
            "enum Phase<T> { Waiting, Running(T) } impl<T> Phase<T> {}\n"
        );
    }

    #[test]
    fn calls_indexes_lists_and_paths() {
        assert_eq!(
            fmt("fn f(){let a=[1,2,3];let b=a[0]+ f (a[1]);let c=std::len(a);return -c;}"),
            "fn f() { let a = [1, 2, 3]; let b = a[0] + f(a[1]); let c = std::len(a); return -c; }\n"
        );
        assert_eq!(
            fmt("fn f(){for i in 0 ..10{}for j in 0..=n{}x.y().z=!a;}"),
            "fn f() { for i in 0..10 {} for j in 0..=n {} x.y().z = !a; }\n"
        );
    }

    #[test]
    fn selectors_coordinates_and_time_literals() {
        assert_eq!(
            fmt("#[every(1s)]\nfn f(){\nas @a[tag=x] at @s{tp(@s,~,~1,^-2);}\n}"),
            "#[every(1s)]\nfn f() {\n    as @a[tag=x] at @s { tp(@s, ~, ~1, ^-2); }\n}\n"
        );
    }

    #[test]
    fn continuation_lines_are_indented_once() {
        assert_eq!(
            fmt(
                "fn f() {\nlet x = a\n+ b\n+ c;\nfoo(1,\n2,\n3);\nlet y = bar(\n1,\n2\n);\nz\n.m()\n.n();\n}"
            ),
            "fn f() {\n    let x = a\n        + b\n        + c;\n    foo(1,\n        2,\n        3);\n    let y = bar(\n        1,\n        2\n    );\n    z\n        .m()\n        .n();\n}\n"
        );
    }

    #[test]
    fn attributes_do_not_continue_into_the_function() {
        assert_eq!(
            fmt("#[load]\n   #[tick]\n   fn f() {}"),
            "#[load]\n#[tick]\nfn f() {}\n"
        );
    }

    #[test]
    fn match_arms() {
        assert_eq!(
            fmt("fn f(){\nmatch x{\nSome(v) if v>1=>{ say(\"a\"); }\n_ =>say(\"b\"),\n}\n}"),
            "fn f() {\n    match x {\n        Some(v) if v > 1 => { say(\"a\"); }\n        _ => say(\"b\"),\n    }\n}\n"
        );
    }

    #[test]
    fn strings_are_untouched() {
        let src = "fn f() {\n    say(\"a   b  {  }\");\n    print(f\"x = {  x+1 }\");\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn empty_and_comment_only_files() {
        assert_eq!(fmt(""), "");
        assert_eq!(fmt("\n\n  \n"), "");
        assert_eq!(fmt("// only\n\n\n// comments"), "// only\n\n// comments\n");
    }

    #[test]
    fn crlf_becomes_lf_and_trailing_space_goes() {
        assert_eq!(
            fmt("fn f() {   \r\n    say(\"a\");   \r\n}\r\n"),
            "fn f() {\n    say(\"a\");\n}\n"
        );
    }

    #[test]
    fn code_with_a_syntax_error_is_refused() {
        assert!(format("fn f( {").is_err());
    }

    #[test]
    fn formatting_twice_changes_nothing() {
        for src in [
            "fn f(){let x=a\n+b;foo(1,\n2);z\n.m();}",
            "// c\nfn  f( )  {\n\n\n  match x{A=>{}\n B=>1,}\n}",
            "enum E<T>{A(T),B}\nimpl<T> E<T>{fn m(self)->T{match self{E::A(v)=>v,E::B=>panic()}}}",
        ] {
            let once = fmt(src);
            assert_eq!(fmt(&once), once, "{src}");
        }
    }
}
