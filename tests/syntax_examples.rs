//! Every example program must parse (docs/lang/syntax.md is the grammar).

use std::fs;
use std::path::Path;

use slab::diag::SourceFile;
use slab::syntax::parse;

#[test]
fn example_programs_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/slab");
    let mut count = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "slab") {
            let text = fs::read_to_string(&path).unwrap();
            let file = SourceFile::new(path.display().to_string(), text.clone());
            if let Err(d) = parse(&text) {
                panic!("{}", file.render(&d));
            }
            count += 1;
        }
    }
    assert!(count >= 2, "expected the example programs in {}", dir.display());
}
