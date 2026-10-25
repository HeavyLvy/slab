//! slab-lsp: a language server for `.slab` files. Reuses the slab compiler as a library for
//! parsing (`slab::syntax`), whole-project checking (`slab::sema`, tolerant of errors) and
//! module discovery (`slab::project`), and speaks the Language Server Protocol over stdio.
//!
//! Also embeds the `grammar` subcommand: it generates the VS Code TextMate grammar from the
//! same keyword/builtin lists the compiler uses.

pub mod analysis;
pub mod completion;
pub mod defs;
pub mod docs;
pub mod grammar;
pub mod help;
pub mod lines;
pub mod signature;
pub mod resolve;
pub mod selector;
pub mod server;
pub mod state;
pub mod symbols;
pub mod tokens;