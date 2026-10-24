//! Whole-project builds through the library and through the `slab` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use slab::compile::{self, Options};

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/projects").join(name)
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slab-it-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn slab(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_slab")).args(args).output().expect("run slab")
}

#[test]
fn example_projects_compile() {
    for name in ["hello", "e2e"] {
        let c = compile::compile(&example(name), &Options::default()).ok().unwrap_or_else(|| panic!("{name} should compile"));
        assert_eq!(c.namespace, name);
        assert!(c.warnings.is_empty(), "{name}: {} warning(s)", c.warnings.len());
        let files = c.pack.files().unwrap();
        assert!(files.contains_key("pack.mcmeta"));
        assert!(files.contains_key(&format!("data/{name}/function/_init.mcfunction")));
        assert!(files.contains_key("data/minecraft/tags/function/load.json"));
    }
}

#[test]
fn build_command_writes_a_folder_and_a_zip() {
    let out = scratch("build").join("pack");
    let o = slab(&["build", example("hello").to_str().unwrap(), "-o", out.to_str().unwrap(), "--zip", "--namespace", "demo"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert!(stdout.contains("built `demo`"), "{stdout}");
    assert!(out.join("pack.mcmeta").is_file());
    assert!(out.join("data/demo/function/boot.mcfunction").is_file());
    assert!(out.join("data/demo/function/util__fib.mcfunction").is_file());
    assert!(out.join("data/demo/function/game__arena__advance.mcfunction").is_file());
    let zip = fs::read(format!("{}.zip", out.display())).unwrap();
    assert_eq!(&zip[..4], b"PK\x03\x04");
    // rebuilding replaces the previous output instead of accumulating stale files
    fs::write(out.join("data/demo/function/stale.mcfunction"), "say old").unwrap();
    assert!(slab(&["build", example("hello").to_str().unwrap(), "-o", out.to_str().unwrap(), "--namespace", "demo"]).status.success());
    assert!(!out.join("data/demo/function/stale.mcfunction").exists());
    let _ = fs::remove_dir_all(out.parent().unwrap());
}

#[test]
fn default_output_goes_under_the_project() {
    let dir = scratch("default");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.slab"), "#[load] fn boot() { print(1); }").unwrap();
    let o = slab(&["build", dir.to_str().unwrap(), "--namespace", "mine"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("build/mine/pack.mcmeta").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn errors_exit_nonzero_with_file_positions() {
    let dir = scratch("errors");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.slab"), "import util;\nfn main() { util::oops(); }\n").unwrap();
    fs::write(dir.join("util.slab"), "pub fn oops() -> int {\n    return \"no\";\n}\n").unwrap();
    let o = slab(&["build", dir.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(stderr.contains("util.slab:2:12"), "{stderr}");
    assert!(stderr.contains("mismatched types: expected `int`, found `str`"), "{stderr}");
    assert!(stderr.contains("could not compile due to 1 error"), "{stderr}");
    assert!(!dir.join("build").exists(), "nothing is written when compilation fails");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_module_and_missing_main() {
    let dir = scratch("missing");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.slab"), "import ghost;\nfn main() {}\n").unwrap();
    let o = slab(&["check", dir.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(stderr.contains("cannot find module `ghost`") && stderr.contains("main.slab:1:8"), "{stderr}");
    assert!(stderr.contains("ghost.slab"), "{stderr}");

    let empty = scratch("nomain");
    fs::create_dir_all(&empty).unwrap();
    let o = slab(&["build", empty.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("no `main.slab`"));
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&empty);
}

#[test]
fn usage_errors() {
    assert_eq!(slab(&[]).status.code(), Some(2));
    assert_eq!(slab(&["frobnicate"]).status.code(), Some(2));
    assert_eq!(slab(&["build", "--nope"]).status.code(), Some(2));
    assert_eq!(slab(&["build", "--namespace"]).status.code(), Some(2));
    assert!(slab(&["--help"]).status.success());
}

#[test]
fn check_reports_warnings_but_succeeds() {
    let dir = scratch("warn");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.slab"), "fn main() { let unused = 1; }\n").unwrap();
    let o = slab(&["check", dir.to_str().unwrap()]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("unused variable `unused`"));
    assert!(String::from_utf8_lossy(&o.stdout).contains("1 warning"));
    let _ = fs::remove_dir_all(&dir);
}
