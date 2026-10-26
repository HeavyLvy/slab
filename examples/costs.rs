//! Prints what every function of a project costs in commands, to find what is expensive in the
//! code the compiler generates: `cargo run --example costs -- PATH [function-prefix]`.
//!
//! The numbers come from `cost::estimate` (src/cost.rs): `low` counts what always runs and `high`
//! counts every branch, each function's loop trips counted once.

use std::collections::HashMap;
use std::path::Path;

use slab::compile::{self, Options};
use slab::cost;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| ".".to_owned());
    let prefix = args.next().unwrap_or_default();
    let compiled = match compile::compile(Path::new(&path), &Options::default()) {
        Ok(c) => c,
        Err(e) => {
            eprint!("{}", e.render());
            std::process::exit(1);
        }
    };
    let none = HashMap::new();
    let mut rows: Vec<(u64, u64, usize, String)> = compiled
        .pack
        .functions()
        .filter(|(id, _)| id.path().starts_with(&prefix))
        .map(|(id, f)| {
            let e = cost::estimate(&compiled.pack, &none, id);
            let lines = f.render().lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).count();
            (e.low, e.high, lines, id.to_string())
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    println!("{:>8} {:>8} {:>6}  function", "high", "low", "lines");
    for (low, high, lines, id) in rows.iter().take(40) {
        println!("{high:>8} {low:>8} {lines:>6}  {id}");
    }
}
