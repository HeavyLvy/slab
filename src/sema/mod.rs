//! Semantic analysis: name resolution, type checking and flow checks. Rules: docs/lang/semantics.md.

pub mod builtins;
mod check;
pub mod hir;
mod selector;
pub mod ty;

pub use check::{Checked, ModuleInput, Report, check, check_modules};

#[cfg(test)]
mod tests;
