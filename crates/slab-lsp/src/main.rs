//! The slab-lsp binary: the language server on stdio, and the `grammar` subcommand that
//! generates the VS Code TextMate grammar from the compiler's definitions.

use std::path::PathBuf;
use std::process::ExitCode;

use slab_lsp::{grammar, server};

const USAGE: &str = "usage:
  slab-lsp                run the language server (stdio)
  slab-lsp grammar [TGT]  write the generated TextMate grammar;
                          TGT is a directory (writes slab.tmLanguage.json in it) or a path
                          ending in .tmLanguage.json (default: the extension's syntaxes/)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("server") => crate::server::run(),
        Some("grammar") => grammar(args.get(1).map(String::as_str)),
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown command `{other}`\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn grammar(target: Option<&str>) -> ExitCode {
    let default = default_target();
    let target = target.map_or_else(|| default.clone(), PathBuf::from);
    let file = if target.extension().and_then(|e| e.to_str()) == Some("json") {
        target
    } else {
        target.join("slab.tmLanguage.json")
    };
    if let Some(parent) = file.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("error: cannot create {}: {e}", parent.display());
            return ExitCode::from(1);
        }
    }
    let text = format!("{}\n", crate::grammar::tm_language_text());
    if let Err(e) = std::fs::write(&file, &text) {
        eprintln!("error: cannot write {}: {e}", file.display());
        return ExitCode::from(1);
    }
    println!("grammar: {}", file.display());
    ExitCode::SUCCESS
}

/// Where `slab-lsp grammar` writes when no target is given: the bundled extension.
fn default_target() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../editors/vscode/syntaxes")
}