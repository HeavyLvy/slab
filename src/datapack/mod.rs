//! Data pack generation for Minecraft Java 26.3 (data pack format 121.0).
//!
//! This module is the only place that turns program structure into game syntax. The compiler
//! lowers to the typed values here ([`Command`], [`Function`], [`DataPack`]) and never
//! formats command text itself. Reference docs: docs/mc/.

mod args;
mod command;
mod error;
mod function;
mod json;
mod names;
mod nbt;
mod pack;
mod target;
mod text;
mod zip;

pub use args::*;
pub use command::*;
pub use error::{Check, Error, Result};
pub use function::{Function, Line};
pub use json::Json;
pub use names::{HolderName, Hole, IdOrTag, Objective, ResourceLocation, Word, holder, rl, word};
pub use nbt::{Compound, Nbt, NbtPath, PathNode};
pub use target::{TARGETS, Target};
pub use pack::{BuildError, DataPack, Diagnostic, PACK_FORMAT, Tag, TagEntry};
pub use text::{ClickEvent, Color, Content, HoverEvent, Style, Text};
