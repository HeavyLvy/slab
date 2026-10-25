//! Semantic analysis: name resolution, type checking and flow checks. Rules: docs/lang/semantics.md.

pub mod builtins;
mod check;
mod matching;
pub mod hir;
mod selector;
pub mod ty;

pub use check::{Checked, ModuleInput, Report, STD_FILE_BASE, Tolerant, check, check_modules, check_modules_tolerant, std_inputs};

#[cfg(test)]
mod tests;
