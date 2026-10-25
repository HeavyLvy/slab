//! The slab language frontend: lexer, syntax tree and parser. Grammar: docs/lang/syntax.md.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod token;

pub use parser::{Parsed, parse, parse_tolerant};
