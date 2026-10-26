//! slab: a programming language that compiles to a Minecraft Java 26.3 data pack.
//!
//! The compiler frontend never builds command strings itself. It lowers to the
//! typed values in [`datapack`] and lets that module render, validate and write them.

pub mod codegen;
pub mod compile;
pub mod cost;
pub mod datapack;
pub mod diag;
pub mod ir;
pub mod lower;
pub mod manifest;
pub mod project;
pub mod sema;
pub mod stdlib;
pub mod stdref;
pub mod syntax;
