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
    assert!(out.join("data/demo/function/maps__arena__advance.mcfunction").is_file());
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
    fs::write(dir.join("main.slab"), "import std::print; #[load] fn boot() { print(1); }").unwrap();
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
    assert!(String::from_utf8_lossy(&o.stderr).contains("no `Slab.toml`"));
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

#[test]
fn new_creates_a_project_with_a_manifest_and_src() {
    let base = scratch("new");
    fs::create_dir_all(&base).unwrap();
    let project = base.join("my-pack");
    let o = slab(&["new", project.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(project.join("Slab.toml").is_file() && project.join("src/main.slab").is_file() && project.join(".gitignore").is_file());
    assert!(fs::read_to_string(project.join("Slab.toml")).unwrap().contains("name = \"my-pack\""));
    // refuses to overwrite
    assert_eq!(slab(&["new", project.to_str().unwrap()]).status.code(), Some(1));

    // builds from the project directory, from inside src/, and by path; the namespace comes from the name
    for cwd in [project.clone(), project.join("src")] {
        let o = Command::new(env!("CARGO_BIN_EXE_slab")).arg("build").current_dir(&cwd).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert!(project.join("build/my_pack/pack.mcmeta").is_file(), "output goes next to Slab.toml, not into src/");
        assert!(!project.join("src/build").exists());
    }
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn slab_toml_sets_the_namespace_description_and_build_options() {
    let dir = scratch("toml");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("Slab.toml"), "[project]\nname = \"Fancy Name\"\nnamespace = \"fancy\"\ndescription = \"A fancy pack\"\n\n[build]\nabort-on-unwrap = true\npack-id = \"file/custom_id\"\n").unwrap();
    fs::write(dir.join("src/main.slab"), "#[load] fn boot() { let o: Option<int> = None; let v = o.unwrap(); }").unwrap();
    let o = slab(&["build", dir.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let pack = dir.join("build/fancy");
    assert!(fs::read_to_string(pack.join("pack.mcmeta")).unwrap().contains("A fancy pack"));
    let all: String = walk(&pack).iter().filter_map(|p| fs::read_to_string(p).ok()).collect();
    assert!(all.contains("datapack disable \"file/custom_id\""), "abort-on-unwrap and pack-id come from the manifest");
    // the command line wins
    let o = slab(&["build", dir.to_str().unwrap(), "--namespace", "cli", "-o", dir.join("out").to_str().unwrap()]);
    assert!(o.status.success());
    assert!(dir.join("out/data/cli").is_dir());
    // a bad manifest is reported with its line
    fs::write(dir.join("Slab.toml"), "[project]\nname = \"x\"\nnmae = 1\n").unwrap();
    let o = slab(&["check", dir.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(stderr.contains("Slab.toml:3: unknown key `nmae`"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() { out.extend(walk(&p)) } else { out.push(p) }
    }
    out
}

#[test]
fn init_turns_a_flat_directory_into_a_project() {
    let dir = scratch("init");
    fs::create_dir_all(dir.join("maps")).unwrap();
    fs::create_dir_all(dir.join("notes")).unwrap();
    fs::write(dir.join("main.slab"), "import maps::arena; fn main() {}").unwrap();
    fs::write(dir.join("maps/arena.slab"), "pub fn f() {}").unwrap();
    fs::write(dir.join("notes/readme.txt"), "keep me here").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_slab")).arg("init").current_dir(&dir).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("Slab.toml").is_file() && dir.join("src/main.slab").is_file() && dir.join("src/maps/arena.slab").is_file());
    assert!(dir.join("notes/readme.txt").is_file(), "folders without .slab files stay");
    assert!(slab(&["check", dir.to_str().unwrap()]).status.success());
    assert_eq!(Command::new(env!("CARGO_BIN_EXE_slab")).arg("init").current_dir(&dir).output().unwrap().status.code(), Some(1), "already a project");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_target_minecraft_version_sets_the_pack_format() {
    let dir = scratch("target");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/main.slab"), "import std::print; #[load] fn boot() { print(1); }").unwrap();
    let manifest = |extra: &str| fs::write(dir.join("Slab.toml"), format!("[project]\nname = \"t\"\n{extra}")).unwrap();
    let mcmeta = || fs::read_to_string(dir.join("build/t/pack.mcmeta")).unwrap();
    let build = |args: &[&str]| {
        let mut a = vec!["build", dir.to_str().unwrap()];
        a.extend_from_slice(args);
        slab(&a)
    };

    // the default is the newest version, which is the one the compiler is checked against
    manifest("");
    let o = build(&[]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("for Minecraft 26.3"));
    assert!(mcmeta().contains("121") && !String::from_utf8_lossy(&o.stderr).contains("warning"));

    // an older release gets its own format and a warning that it was not tried
    manifest("minecraft = \"26.1.2\"\n");
    let o = build(&[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = mcmeta();
    assert!(text.contains("101") && text.contains("\"max_format\": 101") && !text.contains("121"), "{text}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("only checked against Minecraft 26.3"));

    // the command line wins over the manifest
    let o = build(&["--minecraft", "26.3"]);
    assert!(mcmeta().contains("121") && o.status.success());

    // versions before 26.1 are refused, from either place
    assert_eq!(build(&["--minecraft", "1.21.4"]).status.code(), Some(1));
    manifest("minecraft = \"1.21.4\"\n");
    let o = build(&[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("unsupported Minecraft version `1.21.4`"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn watch_rebuilds_when_a_source_file_changes_and_survives_errors() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = scratch("watch");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("Slab.toml"), "[project]\nname = \"w\"\n").unwrap();
    let main = dir.join("src/main.slab");
    fs::write(&main, "fn one() {}\n").unwrap();
    let out = dir.join("out");

    let mut child = Command::new(env!("CARGO_BIN_EXE_slab"))
        .args(["build", "--watch", dir.to_str().unwrap(), "-o", out.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // both streams into one channel, a line at a time
    let (tx, rx) = mpsc::channel::<String>();
    for stream in [Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>, Box::new(child.stderr.take().unwrap())] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    let wait_for = |needle: &str| -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            if let Ok(line) = rx.recv_timeout(Duration::from_millis(200))
                && line.contains(needle)
            {
                return true;
            }
        }
        false
    };
    let function = |name: &str| out.join(format!("data/w/function/{name}.mcfunction")).is_file();

    assert!(wait_for("[watch 1] done"), "the first build");
    assert!(function("one"));

    // a saved change is built, and the output is replaced
    fs::write(&main, "fn two() {}\n").unwrap();
    assert!(wait_for("[watch 2] done"), "the rebuild after a change");
    assert!(function("two") && !function("one"));

    // a mistake is reported and watching goes on
    fs::write(&main, "fn broken( {\n").unwrap();
    assert!(wait_for("[watch 3] failed"), "a failing rebuild is reported");
    assert!(child.try_wait().unwrap().is_none(), "still running after an error");
    fs::write(&main, "fn three() {}\n").unwrap();
    assert!(wait_for("[watch 4] done"), "the rebuild after the fix");
    assert!(function("three"));

    // a new module file counts as a change too
    fs::write(dir.join("src/extra.slab"), "pub fn x() {}\n").unwrap();
    assert!(wait_for("[watch 5] done"), "a new file");

    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_dir_all(&dir);
}
