//! `slab fmt` on the code in this repository, and through the binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn slab_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            slab_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "slab") {
            out.push(path);
        }
    }
}

fn repo_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in ["examples", "src/stdlib"] {
        slab_files(&root().join(dir), &mut files);
    }
    assert!(files.len() >= 10, "found {} files", files.len());
    files
}

#[test]
fn the_code_in_this_repository_is_formatted() {
    for file in repo_files() {
        let text = fs::read_to_string(&file).unwrap();
        let formatted = slab::fmt::format(&text).unwrap_or_else(|e| panic!("{}: {}", file.display(), e.message));
        assert!(formatted == text, "{} is not formatted; run `slab fmt` on it", file.display());
    }
}

#[test]
fn indentation_is_found_again_from_the_code_alone() {
    // take the indentation off every line: the formatter must put it back where the authors had it
    for file in repo_files() {
        let text = fs::read_to_string(&file).unwrap();
        let flat: String = text.lines().map(|l| format!("{}\n", l.trim_start())).collect();
        let formatted = slab::fmt::format(&flat).unwrap_or_else(|e| panic!("{}: {}", file.display(), e.message));
        if formatted != text {
            let line = formatted.lines().zip(text.lines()).position(|(a, b)| a != b).map_or(0, |n| n + 1);
            panic!("{} differs from line {line}:\n  got:  {}\n  want: {}", file.display(), formatted.lines().nth(line - 1).unwrap_or(""), text.lines().nth(line - 1).unwrap_or(""));
        }
    }
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slab-fmt-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn slab(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_slab")).args(args).output().expect("run slab")
}

#[test]
fn fmt_rewrites_a_project_in_place_and_check_only_reports() {
    let dir = scratch("project");
    fs::write(dir.join("Slab.toml"), "[project]\nname = \"p\"\n").unwrap();
    let main = dir.join("src/main.slab");
    fs::write(&main, "import std::print;\n#[load]\nfn boot(){print( 1 );}\n").unwrap();
    fs::write(dir.join("src/util.slab"), "pub fn  two()->int{return 2;}\n").unwrap();

    let o = slab(&["fmt", "--check", dir.to_str().unwrap()]);
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert_eq!(o.status.code(), Some(1), "{out}");
    assert!(out.contains("would reformat") && out.contains("main.slab") && out.contains("util.slab"), "{out}");
    assert!(fs::read_to_string(&main).unwrap().contains("boot(){"), "--check must not write");

    let o = slab(&["fmt", dir.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(fs::read_to_string(&main).unwrap(), "import std::print;\n#[load]\nfn boot() { print(1); }\n");
    assert_eq!(fs::read_to_string(dir.join("src/util.slab")).unwrap(), "pub fn two() -> int { return 2; }\n");

    let o = slab(&["fmt", "--check", dir.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    assert!(String::from_utf8_lossy(&o.stdout).contains("0 files"), "{}", String::from_utf8_lossy(&o.stdout));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fmt_reports_syntax_errors_and_keeps_the_file() {
    let dir = scratch("broken");
    let file = dir.join("src/main.slab");
    fs::write(&file, "fn f( {").unwrap();
    let o = slab(&["fmt", file.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("error:"), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(fs::read_to_string(&file).unwrap(), "fn f( {");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fmt_stdin_filters_text() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_slab")).args(["fmt", "--stdin"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), b"fn f(){ }").unwrap();
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success());
    assert_eq!(String::from_utf8_lossy(&o.stdout), "fn f() {}\n");
}

#[test]
fn the_examples_in_the_documentation_are_formatted() {
    let mut pages = Vec::new();
    for dir in ["docs/guide", "docs/lang", "docs/reference"] {
        for entry in fs::read_dir(root().join(dir)).unwrap().flatten() {
            if entry.path().extension().is_some_and(|e| e == "md") {
                pages.push(entry.path());
            }
        }
    }
    pages.push(root().join("docs/introduction.md"));
    let mut checked = 0;
    for page in pages {
        let text = fs::read_to_string(&page).unwrap();
        for block in text.split("```slab\n").skip(1) {
            let code = block.split("```").next().unwrap();
            // fragments that are not whole programs are not formatted, only whole ones are
            if let Ok(formatted) = slab::fmt::format(code) {
                assert!(formatted == code, "an example in {} is not formatted:\n{code}\nwould become:\n{formatted}", page.display());
                checked += 1;
            }
        }
    }
    assert!(checked > 20, "only {checked} examples were checked");
}
