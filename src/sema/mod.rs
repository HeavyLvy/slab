//! Semantic analysis: name resolution, type checking and flow checks. Rules: docs/lang/semantics.md.

pub mod builtins;
mod check;
pub mod hir;
mod selector;
pub mod ty;

pub use check::{Checked, ModuleInput, Report, Tolerant, check, check_modules, check_modules_tolerant};

#[cfg(test)]
mod tests;
