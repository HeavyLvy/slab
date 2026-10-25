//! Source text -> tokens. See docs/lang/syntax.md, "Lexical structure".

use super::token::{FPart, Kw, RelKind, Tok, Token};
use crate::diag::{Diagnostic, Span};

type R<T> = Result<T, Diagnostic>;

/// The selector variables that may follow `@` (`Tok::Selector { var }`), in the order the
/// language documents them.
pub const SELECTOR_VARS: &[char] = &['a', 'e', 'p', 'r', 's', 'n'];
/// Suffixes on integer literals that make them time literals: ticks, seconds, days.
pub const TIME_SUFFIXES: &[char] = &['t', 's', 'd'];
/// How many ticks each time suffix stands for, in the order of [`TIME_SUFFIXES`].
pub const TIME_MULTIPLIERS: &[i64] = &[1, 20, 24000];

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

/// Tokens of a file. Stops at the first error.
pub fn lex(src: &str) -> R<Vec<Token>> {
    let (tokens, mut errors) = lex_tolerant(src);
    if errors.is_empty() { Ok(tokens) } else { Err(errors.remove(0)) }
}

/// Tokens of a file and every lexical error in it. Text that is not a token becomes a
/// [`Tok::Error`] token so the rest of the file can still be read.
pub fn lex_tolerant(src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut l = Lexer { src, pos: 0 };
    let mut out = Vec::new();
    let mut errors = Vec::new();
    loop {
        if let Err(d) = l.skip_trivia() {
            // an unterminated block comment swallows the rest of the file
            errors.push(d);
            out.push(Token { tok: Tok::Eof, span: Span::new(src.len(), src.len()) });
            return (out, errors);
        }
        let start = l.pos;
        let Some(c) = l.peek() else {
            out.push(Token { tok: Tok::Eof, span: Span::new(start, start) });
            return (out, errors);
        };
        match l.token(c) {
            Ok(tok) => out.push(Token { tok, span: Span::new(start, l.pos) }),
            Err(d) => {
                errors.push(d);
                if l.pos <= start {
                    l.bump();
                }
                out.push(Token { tok: Tok::Error, span: Span::new(start, l.pos) });
            }
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_n(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn err<T>(&self, msg: impl Into<String>, start: usize) -> R<T> {
        Err(Diagnostic::new(msg, Span::new(start, self.pos.max(start + 1).min(self.src.len().max(start + 1)))))
    }

    fn skip_trivia(&mut self) -> R<()> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_n(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some('/') if self.peek_n(1) == Some('*') => {
                    let start = self.pos;
                    self.pos += 2;
                    let mut depth = 1;
                    while depth > 0 {
                        match (self.peek(), self.peek_n(1)) {
                            (Some('/'), Some('*')) => {
                                self.pos += 2;
                                depth += 1;
                            }
                            (Some('*'), Some('/')) => {
                                self.pos += 2;
                                depth -= 1;
                            }
                            (Some(_), _) => {
                                self.bump();
                            }
                            (None, _) => return Err(Diagnostic::new("unterminated block comment", Span::new(start, start + 2))),
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn token(&mut self, c: char) -> R<Tok> {
        let start = self.pos;
        if c == 'f' && self.src[self.pos..].starts_with("f\"") {
            self.pos += 2;
            return self.fstring(start);
        }
        if is_ident_start(c) {
            while self.peek().is_some_and(is_ident_char) {
                self.bump();
            }
            let text = &self.src[start..self.pos];
            return Ok(match Kw::from_ident(text) {
                Some(k) => Tok::Kw(k),
                None => Tok::Ident(text.to_owned()),
            });
        }
        if c.is_ascii_digit() {
            return self.number();
        }
        self.bump();
        let two = |this: &mut Self, next: char, yes: Tok, no: Tok| if this.eat(next) { yes } else { no };
        Ok(match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            ',' => Tok::Comma,
            ';' => Tok::Semi,
            ':' => two(self, ':', Tok::ColonColon, Tok::Colon),
            '#' => Tok::Hash,
            '.' => {
                if self.eat('.') {
                    two(self, '=', Tok::DotDotEq, Tok::DotDot)
                } else {
                    Tok::Dot
                }
            }
            '-' => {
                if self.eat('>') {
                    Tok::Arrow
                } else {
                    two(self, '=', Tok::MinusEq, Tok::Minus)
                }
            }
            '=' => {
                if self.eat('>') {
                    Tok::FatArrow
                } else {
                    two(self, '=', Tok::EqEq, Tok::Eq)
                }
            }
            '!' => two(self, '=', Tok::NotEq, Tok::Bang),
            '<' => two(self, '=', Tok::Le, Tok::Lt),
            '>' => two(self, '=', Tok::Ge, Tok::Gt),
            '+' => two(self, '=', Tok::PlusEq, Tok::Plus),
            '*' => two(self, '=', Tok::StarEq, Tok::Star),
            '/' => two(self, '=', Tok::SlashEq, Tok::Slash),
            '%' => two(self, '=', Tok::PercentEq, Tok::Percent),
            '&' => {
                if self.eat('&') {
                    Tok::AndAnd
                } else {
                    return self.err("expected `&&` (there is no bitwise `&`)", start);
                }
            }
            '|' => {
                if self.eat('|') { Tok::OrOr } else { Tok::Pipe }
            }
            '"' => return self.string(start),
            '@' => return self.selector(start),
            '~' => return Ok(self.rel(RelKind::Tilde)),
            '^' => return Ok(self.rel(RelKind::Caret)),
            other => return self.err(format!("unexpected character `{other}`"), start),
        })
    }

    fn digits(&mut self, ok: impl Fn(char) -> bool) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if ok(c) {
                s.push(c);
                self.bump();
            } else if c == '_' && self.peek_n(1).is_some_and(&ok) {
                self.bump();
            } else {
                break;
            }
        }
        s
    }

    fn number(&mut self) -> R<Tok> {
        let start = self.pos;
        if self.src[self.pos..].starts_with("0x") || self.src[self.pos..].starts_with("0X") {
            self.pos += 2;
            let digits = self.digits(|c| c.is_ascii_hexdigit());
            if digits.is_empty() {
                return self.err("expected hexadecimal digits after `0x`", start);
            }
            return match i64::from_str_radix(&digits, 16) {
                Ok(n) => Ok(Tok::Int(n)),
                Err(_) => self.err("integer literal is too large", start),
            };
        }
        let int_part = self.digits(|c| c.is_ascii_digit());
        // `1.5` is a float, but `1..5` and `x.1` are not
        if self.peek() == Some('.') && self.peek_n(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            let frac = self.digits(|c| c.is_ascii_digit());
            return Ok(Tok::Float(format!("{int_part}.{frac}")));
        }
        let mut value: i64 = match int_part.parse() {
            Ok(n) => n,
            Err(_) => return self.err("integer literal is too large", start),
        };
        // time suffix: 5t, 10s, 1d
        if let Some(unit @ ('t' | 's' | 'd')) = self.peek()
            && !self.peek_n(1).is_some_and(is_ident_char) {
                self.bump();
                let mult = TIME_SUFFIXES.iter().position(|&s| s == unit).map_or(1, |i| TIME_MULTIPLIERS[i]);
                value = match value.checked_mul(mult) {
                    Some(v) => v,
                    None => return self.err("time literal is too large", start),
                };
                return Ok(Tok::Int(value));
            }
        if self.peek().is_some_and(is_ident_start) {
            let bad = self.pos;
            while self.peek().is_some_and(is_ident_char) {
                self.bump();
            }
            return Err(Diagnostic::new(format!("invalid suffix `{}` on number", &self.src[bad..self.pos]), Span::new(start, self.pos))
                .hint("time literals use `t` (ticks), `s` (seconds) or `d` (days)"));
        }
        Ok(Tok::Int(value))
    }

    fn string(&mut self, start: usize) -> R<Tok> {
        let mut s = String::new();
        loop {
            let at = self.pos;
            match self.bump() {
                None | Some('\n') => return Err(Diagnostic::new("unterminated string", Span::new(start, self.pos))),
                Some('"') => return Ok(Tok::Str(s)),
                Some('\\') => self.escape(at, &mut s)?,
                Some(c) => s.push(c),
            }
        }
    }

    /// The part after a `\` in a string literal.
    fn escape(&mut self, at: usize, s: &mut String) -> R<()> {
        match self.bump() {
            Some('n') => s.push('\n'),
            Some('t') => s.push('\t'),
            Some('r') => s.push('\r'),
            Some('0') => s.push('\0'),
            Some('\\') => s.push('\\'),
            Some('"') => s.push('"'),
            Some('\'') => s.push('\''),
            Some('u') => {
                if !self.eat('{') {
                    return self.err("expected `{` after `\\u`", at);
                }
                let hex = self.digits(|c| c.is_ascii_hexdigit());
                if !self.eat('}') {
                    return self.err("expected `}` to end the unicode escape", at);
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(c) => s.push(c),
                    None => return self.err("invalid unicode escape", at),
                }
            }
            _ => return self.err("unknown escape sequence", at),
        }
        Ok(())
    }

    /// `f"..."` after the opening quote: literal text with `{expression}` holes (`{{` and `}}`
    /// write a brace). Strings inside a hole are skipped over, so `f"{name == \"x\"}"` works.
    fn fstring(&mut self, start: usize) -> R<Tok> {
        let unterminated = |this: &Self| Diagnostic::new("unterminated f-string", Span::new(start, this.pos));
        let mut parts = Vec::new();
        let mut lit = String::new();
        loop {
            let at = self.pos;
            match self.bump() {
                None | Some('\n') => return Err(unterminated(self)),
                Some('"') => {
                    if !lit.is_empty() {
                        parts.push(FPart::Lit(lit));
                    }
                    return Ok(Tok::FStr(parts));
                }
                Some('\\') => self.escape(at, &mut lit)?,
                Some('{') if self.eat('{') => lit.push('{'),
                Some('{') => {
                    if !lit.is_empty() {
                        parts.push(FPart::Lit(std::mem::take(&mut lit)));
                    }
                    let inner = self.pos;
                    let mut depth = 1;
                    while depth > 0 {
                        match self.bump() {
                            None | Some('\n') => return Err(unterminated(self)),
                            Some('{') => depth += 1,
                            Some('}') => depth -= 1,
                            Some('"') => loop {
                                match self.bump() {
                                    None | Some('\n') => return Err(unterminated(self)),
                                    Some('\\') => {
                                        self.bump();
                                    }
                                    Some('"') => break,
                                    Some(_) => {}
                                }
                            },
                            Some(_) => {}
                        }
                    }
                    parts.push(FPart::Expr { src: self.src[inner..self.pos - 1].to_owned(), start: inner });
                }
                Some('}') if self.eat('}') => lit.push('}'),
                Some('}') => return self.err("a single `}` in an f-string must be written `}}`", at),
                Some(c) => lit.push(c),
            }
        }
    }

    fn selector(&mut self, start: usize) -> R<Tok> {
        let var = match self.peek() {
            Some(c) if SELECTOR_VARS.contains(&c) => c,
            _ => {
                let list = SELECTOR_VARS.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(", ");
                return self.err(format!("expected a selector variable after `@` (one of {list})"), start);
            }
        };
        self.bump();
        if self.peek().is_some_and(is_ident_char) {
            return self.err(format!("unknown selector `{}`", &self.src[start..self.pos + 1]), start);
        }
        if self.peek() != Some('[') {
            return Ok(Tok::Selector { var, args: None });
        }
        self.bump();
        let args_start = self.pos;
        let mut depth = 1usize;
        while depth > 0 {
            match self.bump() {
                None => return Err(Diagnostic::new("unterminated selector arguments", Span::new(start, self.pos))),
                Some('[' | '{' | '(') => depth += 1,
                Some(']' | '}' | ')') => depth -= 1,
                Some(q @ ('"' | '\'')) => loop {
                    match self.bump() {
                        None => return Err(Diagnostic::new("unterminated string in selector", Span::new(start, self.pos))),
                        Some('\\') => {
                            self.bump();
                        }
                        Some(c) if c == q => break,
                        Some(_) => {}
                    }
                },
                Some(_) => {}
            }
        }
        let args_end = self.pos - 1;
        Ok(Tok::Selector { var, args: Some((self.src[args_start..args_end].to_owned(), Span::new(args_start, args_end))) })
    }

    /// After `~` or `^`: an offset directly attached to it (`~1`, `~-2.5`).
    fn rel(&mut self, kind: RelKind) -> Tok {
        let rest = &self.src[self.pos..];
        let neg = rest.starts_with('-');
        let after = if neg { &rest[1..] } else { rest };
        let starts_number = after.starts_with(|c: char| c.is_ascii_digit()) || (after.starts_with('.') && after[1..].starts_with(|c: char| c.is_ascii_digit()));
        if !starts_number {
            return Tok::Rel { kind, offset: None };
        }
        let start = self.pos;
        if neg {
            self.bump();
        }
        self.digits(|c| c.is_ascii_digit());
        if self.peek() == Some('.') && self.peek_n(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            self.digits(|c| c.is_ascii_digit());
        }
        Tok::Rel { kind, offset: Some(self.src[start..self.pos].to_owned()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn err(src: &str) -> String {
        lex(src).unwrap_err().message
    }

    #[test]
    fn identifiers_and_keywords() {
        assert_eq!(toks("fn foo_1 let"), [Tok::Kw(Kw::Fn), Tok::Ident("foo_1".into()), Tok::Kw(Kw::Let), Tok::Eof]);
    }

    #[test]
    fn numbers_and_time_literals() {
        assert_eq!(toks("1_000 0xFF 5t 10s 1d 0"), [Tok::Int(1000), Tok::Int(255), Tok::Int(5), Tok::Int(200), Tok::Int(24000), Tok::Int(0), Tok::Eof]);
        assert_eq!(toks("1.5"), [Tok::Float("1.5".into()), Tok::Eof]);
        // a range is not a float
        assert_eq!(toks("0..10"), [Tok::Int(0), Tok::DotDot, Tok::Int(10), Tok::Eof]);
        assert_eq!(toks("0..=10"), [Tok::Int(0), Tok::DotDotEq, Tok::Int(10), Tok::Eof]);
        assert!(err("12abc").contains("invalid suffix"));
        assert!(err("99999999999999999999").contains("too large"));
        // `5s` is a time literal but `5sec` is not
        assert!(err("5sec").contains("invalid suffix"));
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(toks(r#""a\n\"b\u{41}""#), [Tok::Str("a\n\"bA".into()), Tok::Eof]);
        assert!(err("\"abc").contains("unterminated"));
        assert!(err("\"a\nb\"").contains("unterminated"));
        assert!(err(r#""\q""#).contains("unknown escape"));
    }

    #[test]
    fn f_strings() {
        let parts = |src: &str| match lex(src).unwrap().remove(0).tok {
            Tok::FStr(p) => p,
            other => panic!("{other:?}"),
        };
        assert_eq!(parts(r#"f"a {x} b""#), [FPart::Lit("a ".into()), FPart::Expr { src: "x".into(), start: 5 }, FPart::Lit(" b".into())]);
        assert_eq!(parts(r#"f"{{literal}} \n""#), [FPart::Lit("{literal} \n".into())]);
        assert_eq!(parts(r#"f"{a == "x"}{b}""#).len(), 2);
        assert_eq!(parts(r#"f"""#), []);
        // a plain identifier named f is still an identifier
        assert_eq!(toks("f x"), [Tok::Ident("f".into()), Tok::Ident("x".into()), Tok::Eof]);
        assert!(err("f\"abc").contains("unterminated f-string"));
        assert!(err("f\"{x\"").contains("unterminated f-string"));
        assert!(err("f\"a } b\"").contains("`}}`"));
    }

    #[test]
    fn comments() {
        assert_eq!(toks("a // x\n b /* c /* nested */ d */ e"), [Tok::Ident("a".into()), Tok::Ident("b".into()), Tok::Ident("e".into()), Tok::Eof]);
        assert!(err("/* open").contains("unterminated block comment"));
    }

    #[test]
    fn paths_and_module_keywords() {
        assert_eq!(toks("a::b"), [Tok::Ident("a".into()), Tok::ColonColon, Tok::Ident("b".into()), Tok::Eof]);
        assert_eq!(toks("x: y"), [Tok::Ident("x".into()), Tok::Colon, Tok::Ident("y".into()), Tok::Eof]);
        assert_eq!(toks("pub import"), [Tok::Kw(Kw::Pub), Tok::Kw(Kw::Import), Tok::Eof]);
    }

    #[test]
    fn operators() {
        assert_eq!(
            toks("== != <= >= += -= *= /= %= && || -> .. ..= !"),
            [
                Tok::EqEq,
                Tok::NotEq,
                Tok::Le,
                Tok::Ge,
                Tok::PlusEq,
                Tok::MinusEq,
                Tok::StarEq,
                Tok::SlashEq,
                Tok::PercentEq,
                Tok::AndAnd,
                Tok::OrOr,
                Tok::Arrow,
                Tok::DotDot,
                Tok::DotDotEq,
                Tok::Bang,
                Tok::Eof
            ]
        );
        assert!(err("a & b").contains("`&&`"));
    }

    #[test]
    fn selectors() {
        assert_eq!(toks("@s"), [Tok::Selector { var: 's', args: None }, Tok::Eof]);
        let t = lex("@e[type=zombie, nbt={a:[1,2]}, name=\"a]b\"]").unwrap();
        match &t[0].tok {
            Tok::Selector { var: 'e', args: Some((raw, _)) } => assert_eq!(raw, "type=zombie, nbt={a:[1,2]}, name=\"a]b\""),
            other => panic!("{other:?}"),
        }
        // an index after a selector is not part of it when separated by a space
        assert_eq!(toks("@s [0]").len(), 5);
        assert!(err("@x").contains("expected a selector variable"));
        assert!(err("@all").contains("unknown selector"));
        assert!(err("@e[type=a").contains("unterminated selector"));
    }

    #[test]
    fn relative_coordinates() {
        assert_eq!(
            toks("~ ~1 ~-2.5 ^3 ^"),
            [
                Tok::Rel { kind: RelKind::Tilde, offset: None },
                Tok::Rel { kind: RelKind::Tilde, offset: Some("1".into()) },
                Tok::Rel { kind: RelKind::Tilde, offset: Some("-2.5".into()) },
                Tok::Rel { kind: RelKind::Caret, offset: Some("3".into()) },
                Tok::Rel { kind: RelKind::Caret, offset: None },
                Tok::Eof
            ]
        );
        // a space detaches the number
        assert_eq!(toks("~ 1"), [Tok::Rel { kind: RelKind::Tilde, offset: None }, Tok::Int(1), Tok::Eof]);
    }

    #[test]
    fn spans() {
        let t = lex("ab  cd").unwrap();
        assert_eq!(t[1].span, Span::new(4, 6));
    }
}
