use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use slab::compile::{self, Options};
use slab::diag::SourceFile;
use slab::syntax::parse;

const USAGE: &str = "usage:
  slab build [PATH] [options]   compile a project to a data pack
  slab check [PATH]             parse and type-check a project, reporting errors and warnings
  slab parse <file.slab>        parse one file and print its syntax tree

PATH is a project directory containing main.slab, or the path of main.slab (default: .).
Other .slab files are modules: `import a::b;` loads a/b.slab next to main.slab.

build options:
  -o, --out DIR          write the data pack folder here (default: PATH/build/<namespace>)
  --zip                  also write DIR.zip
  --namespace NAME       data pack namespace (default: the project directory's name)
  --description TEXT     description shown by /datapack list";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match cmd.as_str() {
        "build" => build(rest),
        "check" => match rest {
            [] => check(Path::new(".")),
            [path] => check(Path::new(path)),
            _ => usage_error("`slab check` takes at most one path"),
        },
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
    opts: Options,
}

fn parse_build_args(args: &[String]) -> Result<BuildArgs, String> {
    let mut b = BuildArgs { path: PathBuf::from("."), out: None, zip: false, opts: Options::default() };
    let mut path_set = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or_else(|| format!("`{name}` needs a value"));
        match a.as_str() {
            "-o" | "--out" => b.out = Some(PathBuf::from(value(a)?)),
            "--zip" => b.zip = true,
            "--namespace" => b.opts.namespace = Some(value(a)?),
            "--description" => b.opts.description = Some(value(a)?),
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
    let compiled = match compile::compile(&b.path, &b.opts) {
        Ok(c) => c,
        Err(e) => {
            eprint!("{}", e.render());
            return ExitCode::from(1);
        }
    };
    print_warnings(&compiled.sources, &compiled.warnings);

    let root = if b.path.is_dir() { b.path.clone() } else { b.path.parent().map(Path::to_path_buf).unwrap_or_default() };
    let out = b.out.clone().unwrap_or_else(|| root.join("build").join(&compiled.namespace));
    if let Err(e) = compiled.pack.write_dir(&out, true) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }
    let functions = compiled.pack.functions().count();
    println!("built `{}`: {} -> {}", compiled.namespace, plural(functions, "function"), out.display());
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
