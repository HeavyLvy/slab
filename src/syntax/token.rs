use crate::diag::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kw {
    Fn,
    Let,
    Mut,
    If,
    Else,
    While,
    Loop,
    For,
    In,
    Return,
    Break,
    Continue,
    Struct,
    Enum,
    Impl,
    Match,
    Global,
    Const,
    Objective,
    True,
    False,
    As,
    At,
    Positioned,
    Rotated,
    Facing,
    Align,
    Anchored,
    Pub,
    Import,
}

impl Kw {
    pub fn from_ident(s: &str) -> Option<Kw> {
        Some(match s {
            "fn" => Kw::Fn,
            "let" => Kw::Let,
            "mut" => Kw::Mut,
            "if" => Kw::If,
            "else" => Kw::Else,
            "while" => Kw::While,
            "loop" => Kw::Loop,
            "for" => Kw::For,
            "in" => Kw::In,
            "return" => Kw::Return,
            "break" => Kw::Break,
            "continue" => Kw::Continue,
            "struct" => Kw::Struct,
            "enum" => Kw::Enum,
            "impl" => Kw::Impl,
            "match" => Kw::Match,
            "global" => Kw::Global,
            "const" => Kw::Const,
            "objective" => Kw::Objective,
            "true" => Kw::True,
            "false" => Kw::False,
            "as" => Kw::As,
            "at" => Kw::At,
            "positioned" => Kw::Positioned,
            "rotated" => Kw::Rotated,
            "facing" => Kw::Facing,
            "align" => Kw::Align,
            "anchored" => Kw::Anchored,
            "pub" => Kw::Pub,
            "import" => Kw::Import,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kw::Fn => "fn",
            Kw::Let => "let",
            Kw::Mut => "mut",
            Kw::If => "if",
            Kw::Else => "else",
            Kw::While => "while",
            Kw::Loop => "loop",
            Kw::For => "for",
            Kw::In => "in",
            Kw::Return => "return",
            Kw::Break => "break",
            Kw::Continue => "continue",
            Kw::Struct => "struct",
            Kw::Enum => "enum",
            Kw::Impl => "impl",
            Kw::Match => "match",
            Kw::Global => "global",
            Kw::Const => "const",
            Kw::Objective => "objective",
            Kw::True => "true",
            Kw::False => "false",
            Kw::As => "as",
            Kw::At => "at",
            Kw::Positioned => "positioned",
            Kw::Rotated => "rotated",
            Kw::Facing => "facing",
            Kw::Align => "align",
            Kw::Anchored => "anchored",
            Kw::Pub => "pub",
            Kw::Import => "import",
        }
    }

    /// Every keyword, in declaration order. Tools that spell out the language's words
    /// (highlighting, completion) take it from here so they cannot go stale.
    pub const ALL: &'static [Kw] = &[
        Kw::Fn,
        Kw::Let,
        Kw::Mut,
        Kw::If,
        Kw::Else,
        Kw::While,
        Kw::Loop,
        Kw::For,
        Kw::In,
        Kw::Return,
        Kw::Break,
        Kw::Continue,
        Kw::Struct,
        Kw::Enum,
        Kw::Impl,
        Kw::Match,
        Kw::Global,
        Kw::Const,
        Kw::Objective,
        Kw::True,
        Kw::False,
        Kw::As,
        Kw::At,
        Kw::Positioned,
        Kw::Rotated,
        Kw::Facing,
        Kw::Align,
        Kw::Anchored,
        Kw::Pub,
        Kw::Import,
    ];

    /// The broad role of a keyword: how highlighting scopes and completion group it.
    pub fn class(self) -> KwClass {
        match self {
            Kw::Fn | Kw::Let | Kw::Struct | Kw::Enum | Kw::Impl | Kw::Global | Kw::Const | Kw::Objective | Kw::Import | Kw::Pub => KwClass::Declaration,
            Kw::If | Kw::Match | Kw::Else | Kw::While | Kw::Loop | Kw::For | Kw::In | Kw::Return | Kw::Break | Kw::Continue => KwClass::Control,
            Kw::Mut => KwClass::Modifier,
            Kw::As | Kw::At | Kw::Positioned | Kw::Rotated | Kw::Facing | Kw::Align | Kw::Anchored => KwClass::Context,
            Kw::True | Kw::False => KwClass::Literal,
        }
    }
}

/// The role of a keyword; see [`Kw::class`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KwClass {
    /// Starts an item: `fn let struct global const objective import pub`.
    Declaration,
    /// Control flow: `if else while loop for in return break continue`.
    Control,
    /// Changes how a declaration works: `mut`.
    Modifier,
    /// Execute-style context: `as at positioned rotated facing align anchored`.
    Context,
    /// Values of type `bool`: `true false`.
    Literal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelKind {
    /// `~`
    Tilde,
    /// `^`
    Caret,
}

/// A piece of an f-string: literal text, or the source of a `{expression}` and where it starts.
#[derive(Clone, Debug, PartialEq)]
pub enum FPart {
    Lit(String),
    Expr { src: String, start: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Kw(Kw),
    /// Integer (time literals are already converted to ticks).
    Int(i64),
    /// Float literal text, kept verbatim.
    Float(String),
    Str(String),
    /// `f"text {expr} text"`
    FStr(Vec<FPart>),
    /// `@a`, `@e[...]`: the variable letter and the raw text between the brackets.
    Selector { var: char, args: Option<(String, Span)> },
    /// `~`, `~1`, `^-2.5`
    Rel { kind: RelKind, offset: Option<String> },

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    ColonColon,
    Dot,
    DotDot,
    DotDotEq,
    Arrow,
    /// `=>`
    FatArrow,
    /// `|` (alternatives in a pattern)
    Pipe,
    Hash,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AndAnd,
    OrOr,
    Bang,
    /// Text the lexer could not make a token of (its error is reported by the lexer).
    Error,
    Eof,
}

impl Tok {
    /// How the token is quoted in error messages.
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("identifier `{s}`"),
            Tok::Kw(k) => format!("`{}`", k.as_str()),
            Tok::Int(n) => format!("`{n}`"),
            Tok::Float(s) => format!("`{s}`"),
            Tok::Str(_) => "a string".to_owned(),
            Tok::FStr(_) => "an f-string".to_owned(),
            Tok::Selector { var, .. } => format!("selector `@{var}`"),
            Tok::Rel { kind: RelKind::Tilde, .. } => "`~`".to_owned(),
            Tok::Rel { kind: RelKind::Caret, .. } => "`^`".to_owned(),
            Tok::Eof => "end of file".to_owned(),
            Tok::Error => "an invalid token".to_owned(),
            other => format!("`{}`", other.punct()),
        }
    }

    fn punct(&self) -> &'static str {
        match self {
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::Comma => ",",
            Tok::Semi => ";",
            Tok::Colon => ":",
            Tok::ColonColon => "::",
            Tok::Dot => ".",
            Tok::DotDot => "..",
            Tok::DotDotEq => "..=",
            Tok::Arrow => "->",
            Tok::FatArrow => "=>",
            Tok::Pipe => "|",
            Tok::Hash => "#",
            Tok::Eq => "=",
            Tok::EqEq => "==",
            Tok::NotEq => "!=",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::PlusEq => "+=",
            Tok::MinusEq => "-=",
            Tok::StarEq => "*=",
            Tok::SlashEq => "/=",
            Tok::PercentEq => "%=",
            Tok::AndAnd => "&&",
            Tok::OrOr => "||",
            Tok::Bang => "!",
            _ => "?",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Every operator spelling, longest first: editors' grammar generators iterate these.
pub fn operator_texts() -> Vec<&'static str> {
    use Tok::*;
    let toks = [
        Eq, EqEq, NotEq, Lt, Le, Gt, Ge, Plus, Minus, Star, Slash, Percent, PlusEq, MinusEq, StarEq, SlashEq,
        PercentEq, AndAnd, OrOr, Arrow, DotDot, DotDotEq, Bang,
    ];
    let mut texts: Vec<&'static str> = toks.iter().map(|t| t.punct()).collect();
    texts.sort_by_key(|t| std::cmp::Reverse(t.len()));
    texts
}
