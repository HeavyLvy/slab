//! `docs/reference/std.md` is written from the library's own sources (`src/stdref.rs`).

use std::path::Path;

#[test]
fn the_standard_library_reference_is_current() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/reference/std.md");
    let generated = slab::stdref::reference();
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(current, generated, "docs/reference/std.md is out of date; run `UPDATE_DOCS=1 cargo test --test stdref` and review the diff");
}
