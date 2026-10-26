//! The version is written in several files; a release needs them to agree (the release workflow
//! checks the same things for the tag it publishes).

use std::fs;
use std::path::Path;

fn read(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

/// The first `version = "x"` of a Cargo.toml.
fn cargo_version(path: &str) -> String {
    let text = read(path);
    let line = text.lines().find(|l| l.starts_with("version = ")).unwrap_or_else(|| panic!("no version in {path}"));
    line.split('"').nth(1).unwrap().to_owned()
}

#[test]
fn the_crates_the_extension_and_the_changelog_agree_on_the_version() {
    let version = cargo_version("Cargo.toml");
    assert_eq!(cargo_version("crates/slab-lsp/Cargo.toml"), version, "slab-lsp's version");
    let package = read("editors/vscode/package.json");
    let line = package.lines().find(|l| l.trim_start().starts_with("\"version\":")).expect("version in package.json");
    assert_eq!(line.split('"').nth(3), Some(version.as_str()), "the extension's version");
    assert!(read("CHANGELOG.md").contains(&format!("## [{version}]")), "CHANGELOG.md needs a `## [{version}]` section");
}
