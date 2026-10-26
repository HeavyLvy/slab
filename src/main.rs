use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use slab::compile::{self, Options};
use slab::diag::SourceFile;
use slab::syntax::parse;

const USAGE: &str = "usage:
  slab new NAME                 create a project: NAME/Slab.toml and NAME/src/main.slab
  slab init                     make the current directory a project (moves .slab files into src/)
  slab build [PATH] [options]   compile a project to a data pack
  slab check [PATH]             parse and type-check a project, reporting errors and warnings
  slab fmt [PATH...] [options]  format source files in place (a project, a directory or files)
  slab parse <file.slab>        parse one file and print its syntax tree

A project is a directory with a Slab.toml and its code in src/, where src/main.slab is the root
module. PATH is that directory, or any directory inside it (default: .), or the path of main.slab.
Other .slab files are modules: `import a::b;` loads src/a/b.slab. (A bare directory with a
main.slab and no Slab.toml also works.)

fmt options:
  --check                change nothing; list the files that are not formatted and exit with 1
  --stdin                read one file from standard input and write it formatted to standard output

build options (they override Slab.toml):
  -o, --out DIR          write the data pack folder here (default: <project>/build/<namespace>)
  --zip                  also write DIR.zip
  -w, --watch            keep running and build again whenever a source file is saved
  --namespace NAME       data pack namespace (default: from Slab.toml, else the directory's name)
  --description TEXT     description shown by /datapack list
  --abort-on-unwrap      a failed unwrap()/expect() also runs `datapack disable` for the pack
  --pack-id ID           the pack id for that (default: file/<namespace>)
  --task-budget N        commands a `task fn` may use per tick (default 30000)
  --command-limit N      set the game's commands-per-tick limit to N when the pack loads (see docs)
  --minecraft VERSION    the game version to build for: 26.1, 26.2 or 26.3 (default: Slab.toml, else 26.3)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match cmd.as_str() {
        "new" => match rest {
            [name] => new_project(Path::new(name)),
            _ => usage_error("`slab new` takes the name of the project"),
        },
        "init" => match rest {
            [] => init_project(Path::new(".")),
            _ => usage_error("`slab init` takes no arguments"),
        },
        "build" => build(rest),
        "check" => match rest {
            [] => check(Path::new(".")),
            [path] => check(Path::new(path)),
            _ => usage_error("`slab check` takes at most one path"),
        },
        "fmt" => fmt(rest),
        "parse" => match rest {
            [path] => parse_file(path),
            _ => usage_error("`slab parse` takes one file"),
        },
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => usage_error(&format!("unknown command `{other}`")),
    }
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("error: {msg}\n\n{USAGE}");
    ExitCode::from(2)
}

fn print_warnings(sources: &slab::diag::SourceMap, warnings: &[slab::diag::Diagnostic]) {
    for w in warnings {
        eprintln!("{}", sources.render(w));
    }
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn check(path: &Path) -> ExitCode {
    match compile::check(path) {
        Ok(c) => {
            print_warnings(&c.sources, &c.warnings);
            println!("ok: {}, {}, {}", plural(c.modules, "module"), plural(c.functions, "function"), plural(c.warnings.len(), "warning"));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprint!("{}", e.render());
            ExitCode::from(1)
        }
    }
}

struct BuildArgs {
    path: PathBuf,
    out: Option<PathBuf>,
    zip: bool,
    /// Rebuild whenever a source file changes (`--watch`).
    watch: bool,
    opts: Options,
}

fn parse_build_args(args: &[String]) -> Result<BuildArgs, String> {
    let mut b = BuildArgs { path: PathBuf::from("."), out: None, zip: false, watch: false, opts: Options::default() };
    let mut path_set = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or_else(|| format!("`{name}` needs a value"));
        match a.as_str() {
            "-o" | "--out" => b.out = Some(PathBuf::from(value(a)?)),
            "--zip" => b.zip = true,
            "-w" | "--watch" => b.watch = true,
            "--namespace" => b.opts.namespace = Some(value(a)?),
            "--description" => b.opts.description = Some(value(a)?),
            "--abort-on-unwrap" => b.opts.abort_on_unwrap = true,
            "--pack-id" => b.opts.pack_id = Some(value(a)?),
            "--minecraft" => b.opts.minecraft = Some(value(a)?),
            "--task-budget" => b.opts.task_budget = Some(value(a)?.parse::<i32>().ok().filter(|n| *n >= 100).ok_or("`--task-budget` needs a number of commands, at least 100")?),
            "--command-limit" => b.opts.command_limit = Some(value(a)?.parse::<i32>().ok().filter(|n| *n >= 1).ok_or("`--command-limit` needs a number from 1 to 2147483647")?),
            flag if flag.starts_with('-') => return Err(format!("unknown option `{flag}`")),
            p => {
                if path_set {
                    return Err("`slab build` takes one project path".into());
                }
                b.path = PathBuf::from(p);
                path_set = true;
            }
        }
    }
    Ok(b)
}

fn build(args: &[String]) -> ExitCode {
    let b = match parse_build_args(args) {
        Ok(b) => b,
        Err(m) => return usage_error(&m),
    };
    if b.watch { watch(&b) } else { build_once(&b) }
}

/// The files a build reads: every `.slab` file under the source root, and `Slab.toml`, with
/// their modification times and sizes. A change in this list means "build again".
fn source_snapshot(path: &Path) -> Vec<(PathBuf, Option<std::time::SystemTime>, u64)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                // build output and tool folders are not source
                if !name.starts_with('.') && !matches!(name.as_str(), "build" | "target" | "node_modules") {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "slab") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    // when the entry cannot be found there is nothing to watch yet; the build says why
    if let Ok(root_file) = slab::project::find_root_file(path)
        && let Some(root) = root_file.parent()
    {
        walk(root, &mut files);
        if let Some(manifest) = root.parent().map(|p| p.join(slab::manifest::FILE)).filter(|m| m.is_file()) {
            files.push(manifest);
        }
    }
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let meta = std::fs::metadata(&p).ok();
            let (modified, len) = meta.map_or((None, 0), |m| (m.modified().ok(), m.len()));
            (p, modified, len)
        })
        .collect()
}

/// `slab build --watch`: build, then build again whenever a source file is saved, added or
/// removed. Mistakes are shown and watching goes on; stop it with Ctrl+C.
fn watch(b: &BuildArgs) -> ExitCode {
    let mut runs = 0;
    loop {
        // taken before the build, so a save during the build triggers the next one
        let snapshot = source_snapshot(&b.path);
        runs += 1;
        let started = std::time::Instant::now();
        let status = build_once(b);
        if status == ExitCode::SUCCESS {
            println!("[watch {runs}] done in {} ms; `/reload` in the game to load it", started.elapsed().as_millis());
        } else {
            eprintln!("[watch {runs}] failed, fix the errors above and save again");
        }
        if runs == 1 {
            println!("watching {} source file(s); Ctrl+C stops", snapshot.len());
        }
        loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if source_snapshot(&b.path) != snapshot {
                // an editor may write several files; let it finish before building
                std::thread::sleep(std::time::Duration::from_millis(150));
                break;
            }
        }
    }
}

fn build_once(b: &BuildArgs) -> ExitCode {
    let compiled = match compile::compile(&b.path, &b.opts) {
        Ok(c) => c,
        Err(e) => {
            eprint!("{}", e.render());
            return ExitCode::from(1);
        }
    };
    print_warnings(&compiled.sources, &compiled.warnings);
    for n in &compiled.notices {
        eprintln!("warning: {n}");
    }

    let out = b.out.clone().unwrap_or_else(|| compiled.project_dir.join("build").join(&compiled.namespace));
    if let Err(e) = compiled.pack.write_dir(&out, true) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }
    let functions = compiled.pack.functions().count();
    println!("built `{}` for Minecraft {}: {} -> {}", compiled.namespace, compiled.target, plural(functions, "function"), out.display());
    if b.zip {
        let zip = PathBuf::from(format!("{}.zip", out.display()));
        if let Err(e) = compiled.pack.write_zip(&zip) {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
        println!("zipped: {}", zip.display());
    }
    ExitCode::SUCCESS
}

/// The `.slab` files under `path`: the file itself, or every file in the directory (a project's
/// `src/`, when it has a `Slab.toml`), without descending into hidden folders, `build` or `target`.
fn slab_files(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    let dir = if path.join(slab::manifest::FILE).is_file() { path.join(slab::manifest::SOURCE_DIR) } else { path.to_path_buf() };
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for entry in entries {
        let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if entry.is_dir() {
            if !name.starts_with('.') && name != "build" && name != "target" {
                slab_files(&entry, out)?;
            }
        } else if entry.extension().is_some_and(|e| e == "slab") {
            out.push(entry);
        }
    }
    Ok(())
}

fn fmt(args: &[String]) -> ExitCode {
    let (mut check, mut stdin, mut paths) = (false, false, Vec::new());
    for a in args {
        match a.as_str() {
            "--check" => check = true,
            "--stdin" => stdin = true,
            flag if flag.starts_with('-') => return usage_error(&format!("unknown option `{flag}` for `slab fmt`")),
            path => paths.push(PathBuf::from(path)),
        }
    }
    if stdin {
        if !paths.is_empty() || check {
            return usage_error("`slab fmt --stdin` reads standard input and takes no paths or --check");
        }
        let mut text = String::new();
        if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text) {
            eprintln!("error: cannot read standard input: {e}");
            return ExitCode::from(1);
        }
        return match slab::fmt::format(&text) {
            Ok(formatted) => {
                let _ = write!(std::io::stdout(), "{formatted}");
                ExitCode::SUCCESS
            }
            Err(d) => {
                eprint!("{}", SourceFile::new("<stdin>", text).render(&d));
                ExitCode::from(1)
            }
        };
    }
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }
    let mut files = Vec::new();
    for path in &paths {
        if let Err(e) = slab_files(path, &mut files) {
            eprintln!("error: cannot read {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if files.is_empty() {
        eprintln!("error: no .slab files found");
        return ExitCode::from(1);
    }
    let (mut changed, mut failed) = (0, 0);
    for file in &files {
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: cannot read {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        match slab::fmt::format(&text) {
            Ok(formatted) if formatted == text => {}
            Ok(formatted) => {
                changed += 1;
                if check {
                    println!("would reformat {}", file.display());
                } else if let Err(e) = std::fs::write(file, formatted) {
                    eprintln!("error: cannot write {}: {e}", file.display());
                    failed += 1;
                } else {
                    println!("formatted {}", file.display());
                }
            }
            Err(d) => {
                eprint!("{}", SourceFile::new(file.display().to_string(), text).render(&d));
                failed += 1;
            }
        }
    }
    let unchanged = files.len() - changed - failed;
    let verb = if check { "would change" } else { "formatted" };
    println!("{verb} {}, {unchanged} already formatted{}", plural(changed, "file"), if failed > 0 { format!(", {} with errors", plural(failed, "file")) } else { String::new() });
    if failed > 0 || (check && changed > 0) { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn parse_file(path: &str) -> ExitCode {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let file = SourceFile::new(path, text);
    match parse(&file.text) {
        Ok(program) => {
            // ignore a closed pipe (`slab parse x.slab | head`)
            let _ = writeln!(std::io::stdout(), "{program:#?}");
            ExitCode::SUCCESS
        }
        Err(d) => {
            eprint!("{}", file.render(&d));
            ExitCode::from(1)
        }
    }
}

const HELLO: &str = "import std::print;

#[load]
fn hello() {
    print(\"hello from NAME\");
}
";

fn write_new(dir: &Path, name: &str) -> Result<(), String> {
    let write = |path: PathBuf, text: String| std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()));
    std::fs::create_dir_all(dir.join(slab::manifest::SOURCE_DIR)).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    write(dir.join(slab::manifest::FILE), slab::manifest::template(name))?;
    let main = dir.join(slab::manifest::SOURCE_DIR).join(slab::project::ROOT_FILE);
    if !main.exists() {
        write(main, HELLO.replace("NAME", name))?;
    }
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        write(ignore, "/build\n".to_owned())?;
    }
    Ok(())
}

fn new_project(dir: &Path) -> ExitCode {
    let Some(name) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) else { return usage_error("give the project a name: `slab new my_pack`") };
    if dir.exists() {
        eprintln!("error: {} already exists (use `slab init` inside it to make it a project)", dir.display());
        return ExitCode::from(1);
    }
    match write_new(dir, &name) {
        Ok(()) => {
            println!("created project `{name}` in {}\nbuild it with: cd {} && slab build", dir.display(), dir.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

/// Whether the directory (or something below it) holds a `.slab` file.
fn has_slab_files(dir: &Path) -> bool {
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
        let p = e.path();
        if p.is_dir() { has_slab_files(&p) } else { p.extension().is_some_and(|x| x == "slab") }
    })
}

fn init_project(dir: &Path) -> ExitCode {
    if dir.join(slab::manifest::FILE).exists() {
        eprintln!("error: {} already has a {}", dir.display(), slab::manifest::FILE);
        return ExitCode::from(1);
    }
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let name = canonical.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "slab".to_owned());
    // an existing flat project: its .slab files and the folders of modules move into src/
    let src = dir.join(slab::manifest::SOURCE_DIR);
    let mut moved = Vec::new();
    if dir.join(slab::project::ROOT_FILE).is_file() {
        if let Err(e) = std::fs::create_dir_all(&src) {
            eprintln!("error: cannot create {}: {e}", src.display());
            return ExitCode::from(1);
        }
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let is_module = if p.is_dir() { !matches!(file_name.as_str(), "src" | "build" | "target") && !file_name.starts_with('.') && has_slab_files(&p) } else { p.extension().is_some_and(|x| x == "slab") };
            if is_module {
                if let Err(e) = std::fs::rename(&p, src.join(&file_name)) {
                    eprintln!("error: cannot move {}: {e}", p.display());
                    return ExitCode::from(1);
                }
                moved.push(file_name);
            }
        }
    }
    match write_new(dir, &name) {
        Ok(()) => {
            println!("created {} for `{name}`", slab::manifest::FILE);
            if !moved.is_empty() {
                moved.sort();
                println!("moved into {}/: {}", slab::manifest::SOURCE_DIR, moved.join(", "));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
