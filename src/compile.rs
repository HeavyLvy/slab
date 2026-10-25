//! The whole pipeline: project files -> parse -> check -> HIR -> IR -> data pack.

use std::path::Path;

use crate::codegen;
use crate::datapack::{DataPack, ResourceLocation, Word};
use crate::diag::{Diagnostic, Severity, SourceMap};
use crate::lower;
use crate::project::{self, LoadError};
use crate::sema::{self, ModuleInput};

#[derive(Default)]
pub struct Options {
    /// The data pack namespace (also the objective and storage prefix). Defaults to the project
    /// directory's name.
    pub namespace: Option<String>,
    pub description: Option<String>,
}

pub struct Compiled {
    pub pack: DataPack,
    pub namespace: String,
    pub warnings: Vec<Diagnostic>,
    pub sources: SourceMap,
}

pub enum CompileError {
    /// The project could not be found or read.
    Io(String),
    /// Mistakes in the program, with the files to render them against.
    Source { sources: SourceMap, errors: Vec<Diagnostic>, warnings: Vec<Diagnostic> },
    /// The generated pack was rejected: a compiler bug or a limit of the backend.
    Backend(String),
}

impl CompileError {
    /// The error text as the command line prints it.
    pub fn render(&self) -> String {
        match self {
            CompileError::Io(m) => format!("error: {m}\n"),
            CompileError::Backend(m) => format!("error: {m}\n"),
            CompileError::Source { sources, errors, warnings } => {
                let mut out = String::new();
                for d in errors.iter().chain(warnings) {
                    out.push_str(&sources.render(d));
                    out.push('\n');
                }
                let n = errors.len();
                out.push_str(&format!("error: could not compile due to {n} error{}\n", if n == 1 { "" } else { "s" }));
                out
            }
        }
    }
}

/// `my-project` -> `my_project`: a valid namespace derived from the directory name.
pub fn default_namespace(root: &Path) -> String {
    let dir = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ns: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    if ns.is_empty() { "slab".to_owned() } else { ns }
}

pub fn validate_namespace(ns: &str) -> Result<(), String> {
    ResourceLocation::new(ns, "x").map_err(|e| format!("invalid namespace `{ns}`: {}", e.message()))?;
    // it doubles as the name of the scoreboard objective and of the codegen's storage ids
    Word::new(ns).map_err(|_| format!("invalid namespace `{ns}`: use letters, digits, `_`, `-`, `.` or `+`"))?;
    if ns.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(format!("invalid namespace `{ns}`: namespaces are lowercase"));
    }
    Ok(())
}

/// Result of checking without generating anything.
pub struct Checked {
    pub sources: SourceMap,
    pub warnings: Vec<Diagnostic>,
    pub functions: usize,
    pub modules: usize,
}

fn load_and_check(entry: &Path) -> Result<(project::Project, sema::Checked), CompileError> {
    let root_file = project::find_root_file(entry).map_err(CompileError::Io)?;
    let project = match project::load(&root_file) {
        Ok(p) => p,
        Err(LoadError::Io(m)) => return Err(CompileError::Io(m)),
        Err(LoadError::Invalid { sources, diagnostics }) => return Err(CompileError::Source { sources, errors: diagnostics, warnings: Vec::new() }),
    };
    let inputs: Vec<ModuleInput> = project.modules.iter().map(|m| ModuleInput { name: &m.name, file: m.file, program: &m.program, std: m.std }).collect();
    match sema::check_modules(&inputs) {
        Ok(checked) => Ok((project, checked)),
        Err(report) => Err(CompileError::Source { sources: project.sources, errors: report.errors, warnings: report.warnings }),
    }
}

/// Parses and type-checks a project.
pub fn check(entry: &Path) -> Result<Checked, CompileError> {
    let (project, checked) = load_and_check(entry)?;
    Ok(Checked { functions: checked.hir.fns.len(), modules: project.modules.iter().filter(|m| !m.std).count(), warnings: checked.warnings, sources: project.sources })
}

/// Compiles a project to a data pack.
pub fn compile(entry: &Path, opts: &Options) -> Result<Compiled, CompileError> {
    let (project, checked) = load_and_check(entry)?;
    let namespace = opts.namespace.clone().unwrap_or_else(|| default_namespace(&project.root));
    validate_namespace(&namespace).map_err(CompileError::Io)?;
    let description = opts.description.clone().unwrap_or_else(|| format!("{namespace} (compiled by slab)"));

    let program = match codegen::generate(&checked.hir, &namespace, &description) {
        Ok(p) => p,
        Err(errors) => return Err(CompileError::Source { sources: project.sources, errors, warnings: checked.warnings }),
    };
    let pack = lower::lower(&program).map_err(|e| CompileError::Backend(format!("the backend rejected the program: {e}")))?;
    let problems = pack.validate();
    if !problems.is_empty() {
        let list: Vec<String> = problems.iter().map(|d| format!("  {d}")).collect();
        return Err(CompileError::Backend(format!("the generated data pack is invalid (this is a bug or a backend limit):\n{}", list.join("\n"))));
    }
    let warnings = checked.warnings.into_iter().filter(|d| d.severity == Severity::Warning).collect();
    Ok(Compiled { pack, namespace, warnings, sources: project.sources })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A throwaway project directory.
    pub(crate) struct Dir(pub PathBuf);

    impl Dir {
        pub fn new(tag: &str, files: &[(&str, &str)]) -> Dir {
            // tests run in parallel inside one process, so every directory needs its own name
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("slab-compile-{}-{n}-{tag}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            for (name, text) in files {
                let path = dir.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, text).unwrap();
            }
            Dir(dir)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn build(files: &[(&str, &str)]) -> Compiled {
        let d = Dir::new("t", files);
        match compile(&d.0, &Options { namespace: Some("t".into()), ..Options::default() }) {
            Ok(c) => c,
            Err(e) => panic!("{}", e.render()),
        }
    }

    fn fn_text(c: &Compiled, id: &str) -> String {
        c.pack.function(&ResourceLocation::parse(id).unwrap()).unwrap_or_else(|| panic!("no function {id}")).render()
    }

    #[test]
    fn compiles_a_project_with_modules() {
        let c = build(&[
            ("main.slab", "import util; global answer: int = 0; #[load] fn boot() { answer = util::double(21); }"),
            ("util.slab", "pub fn double(x: int) -> int { return x * 2; }"),
        ]);
        let boot = fn_text(&c, "t:boot");
        assert!(boot.contains("function t:util__double"), "{boot}");
        assert!(fn_text(&c, "t:util__double").contains("scoreboard players operation #ret t"));
        assert!(fn_text(&c, "t:_init").contains("function t:boot"));
    }

    #[test]
    fn errors_name_the_right_file() {
        let d = Dir::new("err", &[("main.slab", "import util; fn main() { util::f(); }"), ("util.slab", "pub fn f() { let x: int = \"s\"; }")]);
        let Err(e) = compile(&d.0, &Options::default()) else { panic!("expected an error") };
        let text = e.render();
        assert!(text.contains("util.slab:1:"), "{text}");
        assert!(text.contains("mismatched types"), "{text}");
        assert!(text.contains("due to 1 error"), "{text}");
    }

    /// Test programs start with these imports (on the same line, so positions don't shift).
    const PRELUDE: &str = "import std::*; import game::*; ";

    fn build_main(src: &str) -> Compiled {
        build(&[("main.slab", &format!("{PRELUDE}{src}"))])
    }

    fn build_err(src: &str) -> String {
        let d = Dir::new("e", &[("main.slab", &format!("{PRELUDE}{src}"))]);
        match compile(&d.0, &Options { namespace: Some("t".into()), ..Options::default() }) {
            Ok(_) => panic!("expected an error"),
            Err(e) => e.render(),
        }
    }

    #[test]
    fn break_and_continue_become_guarded_flags() {
        let c = build_main("fn f() { let mut s = 0; for i in 0..10 { if i == 3 { continue; } if i == 7 { break; } s += i; } print(s); }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("scoreboard players set #f.v_brk t 0") && f.contains("scoreboard players set #f.v_cont t 0"), "{f}");
        // the counter increment is outside the guard so `continue` still advances the loop
        let body = fn_text(&c, "t:f/b0");
        assert!(body.contains("scoreboard players set #f.v_cont t 0"), "{body}");
        assert!(body.contains("scoreboard players add #f.v_i t 1"), "{body}");
    }

    #[test]
    fn loops_without_jumps_get_no_flags() {
        let c = build_main("fn f() { let mut i = 0; while i < 3 { i += 1; } print(i); }");
        assert!(!fn_text(&c, "t:f").contains("brk"));
    }

    #[test]
    fn names_are_mangled_and_kept_unique() {
        let c = build(&[
            ("main.slab", "import a; fn helper() {} fn Helper() {} fn main() { a::helper(); helper(); Helper(); }"),
            ("a.slab", "pub fn helper() {}"),
        ]);
        for id in ["t:helper", "t:helper_2", "t:a__helper", "t:main"] {
            assert!(c.pack.function(&ResourceLocation::parse(id).unwrap()).is_some(), "missing {id}");
        }
    }

    #[test]
    fn every_n_becomes_a_counter_and_a_tick_function() {
        let c = build_main("global n: int = 0; #[every(1s)] fn sec() { n += 1; } #[every(1)] fn each() { n += 1; }");
        let tick = fn_text(&c, "t:sec__every_tick");
        assert!(tick.contains("scoreboard players add #g.sec__every t 1"), "{tick}");
        assert!(tick.contains("matches 20.."), "{tick}");
        // every(1) is just a tick function
        let files = c.pack.files().unwrap();
        let tick_tag = String::from_utf8_lossy(&files["data/minecraft/tags/function/tick.json"]).into_owned();
        assert!(tick_tag.contains("t:each") && tick_tag.contains("t:sec__every_tick") && !tick_tag.contains("\"t:sec\""), "{tick_tag}");
    }

    #[test]
    fn scalars_are_converted_at_storage_boundaries() {
        let c = build_main("struct P { x: int } fn f() { let mut p = P { x: 1 }; let n = 5; p.x = n + 1; let y = p.x; print(y); }");
        let f = fn_text(&c, "t:f");
        // storing an int into a field converts it to a tag in a temporary, then copies it in
        // (fusing these into one `execute store` is a possible optimisation)
        assert!(f.contains("execute store result storage t:vars f.t_0 int 1 run scoreboard players get #f.t_0 t"), "{f}");
        assert!(f.contains("data modify storage t:vars f.v_p.x set from storage t:vars f.t_0"), "{f}");
        // reading goes back through a score
        assert!(f.contains("execute store result score #f.v_y t run data get storage t:vars f.v_p.x"), "{f}");
    }

    #[test]
    fn nonconstant_aggregates_are_built_in_a_temporary() {
        let c = build_main("fn f(a: int) { let xs = [a, a + 1]; print(xs.len()); }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("data modify storage t:vars f.v_tmp set value []"), "{f}");
        assert_eq!(f.matches("append").count(), 2, "{f}");
        // an all-constant literal is emitted directly
        let c = build_main("fn f() { let xs = [1, 2]; print(xs.len()); }");
        assert!(fn_text(&c, "t:f").contains("set value [1,2]"));
    }

    #[test]
    fn context_blocks_become_execute_calls() {
        let c = build_main("fn f() { as @a[tag=x] at @s positioned ~ ~1 ~ { say(\"hi\"); } }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("execute as @a[tag=x] at @s positioned ~ ~1 ~ run function t:f/b0"), "{f}");
        assert_eq!(fn_text(&c, "t:f/b0"), "say hi\n");
    }

    #[test]
    fn builtins_become_typed_commands() {
        let c = build_main("fn f() { effect::give(@s, \"speed\", 10s, 1); tp(@s, ~, ~2, ~); setblock(1, 64, -3, \"stone\"); tag::add(@s, \"x\"); kill(@e[type=zombie]); }");
        let f = fn_text(&c, "t:f");
        for line in [
            "effect give @s minecraft:speed 10 1",
            "teleport @s ~ ~2 ~",
            "setblock 1 64 -3 minecraft:stone",
            "tag @s add x",
            "kill @e[type=minecraft:zombie]",
        ] {
            assert!(f.contains(line), "missing `{line}` in\n{f}");
        }
    }

    #[test]
    fn backend_limits_have_clear_messages() {
        assert!(build_err("fn f(i: int) { let mut x = [[1]]; x[i].push(2); }").contains("variable index"));
        assert!(build_err("objective o = \"dummy\"; fn f(n: str) { o[n] = 1; }").contains("score holder must be a selector or a string literal"));
        assert!(build_err("fn f() { tp(@s, ~, ^, ~); }").contains("cannot mix world"));
    }

    #[test]
    fn summoned_entities_are_owned_and_removed_by_uninstall() {
        let c = build_main("fn f() { summon(\"minecraft:marker\"); summon(\"minecraft:pig\", 1, 64, 2); tag::add(@s, \"seen\"); tag::add(@a, \"seen\"); tag::add(@s, \"other\"); }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("summon minecraft:marker ~ ~ ~ {Tags:[\"t.owned\"]}"), "{f}");
        assert!(f.contains("summon minecraft:pig 1 64 2 {Tags:[\"t.owned\"]}"), "{f}");
        let u = fn_text(&c, "t:_uninstall");
        assert!(u.contains("kill @e[tag=t.owned]"), "{u}");
        // each added tag is removed once, however often it was added
        assert_eq!(u.matches("tag @e[tag=seen] remove seen").count(), 1, "{u}");
        assert!(u.contains("tag @e[tag=other] remove other"), "{u}");
    }

    #[test]
    fn a_program_that_summons_and_tags_nothing_removes_no_entities() {
        let c = build_main("fn f() { print(1); }");
        let u = fn_text(&c, "t:_uninstall");
        assert!(!u.contains("kill") && !u.contains("tag @e"), "{u}");
    }

    #[test]
    fn locals_of_every_function_are_dropped() {
        let c = build_main("fn f(s: str) -> str { let t = s; return t; } fn main() { let r = f(\"a\"); print(r); }");
        // two locals (`s`, `t`) share a drop function, called after the return value has been copied out
        let f = fn_text(&c, "t:f");
        assert!(f.find("data modify storage t:ret v").unwrap() < f.find("function t:f/_drop").unwrap(), "{f}");
        let d = fn_text(&c, "t:f/_drop");
        assert!(d.contains("data remove storage t:vars f.v_s") && d.contains("data remove storage t:vars f.v_t"), "{d}");
        // `main` cleans up its own local (`r`) after the print that reads it
        let main = fn_text(&c, "t:main");
        let print = main.find("tellraw").expect(&main);
        let drop = main.rfind("data remove storage t:vars main.v_r").expect(&main);
        assert!(print < drop, "{main}");
    }

    #[test]
    fn variable_coordinates_go_through_a_macro_function() {
        let c = build_main("fn f(x: int) { tp(@s, x, 64, x + 1); }");
        let f = fn_text(&c, "t:f");
        // the values are written to the scratch storage, the helper called with them, and the keys removed
        assert!(f.contains("execute store result storage t:macro h0 int 1 run scoreboard players get #f.v_x t"), "{f}");
        assert!(f.contains("execute store result storage t:macro h1 int 1 run scoreboard players get #f.v_hv t"), "{f}");
        assert!(f.contains("function t:f/m0 with storage t:macro"), "{f}");
        assert!(f.contains("data remove storage t:macro h0") && f.contains("data remove storage t:macro h1"), "{f}");
        assert_eq!(fn_text(&c, "t:f/m0"), "$teleport @s $(h0) 64 $(h1)\n");
        // constants keep compiling to plain commands
        let c = build_main("fn f() { tp(@s, 1, 64, 2); }");
        assert_eq!(fn_text(&c, "t:f"), "teleport @s 1 64 2\n");
    }

    #[test]
    fn variable_list_indices_read_and_write_through_macros() {
        let c = build_main("fn f(i: int) -> int { let mut xs = [10, 20, 30]; xs[i] = 5; xs[i] += 1; return xs[i + 1]; }");
        let f = fn_text(&c, "t:f");
        assert_eq!(fn_text(&c, "t:f/m0"), "$data modify storage t:vars f.v_xs[$(h0)] set value 5\n", "constant write");
        assert_eq!(fn_text(&c, "t:f/m2"), "$data modify storage t:vars f.v_xs[$(h0)] set from storage t:vars f.t_0\n", "write back after +=");
        // `+=` reads the element and writes it back with the same index value
        assert!(fn_text(&c, "t:f/m1").starts_with("$data modify storage t:vars f.v_tmp set from storage t:vars f.v_xs[$(h0)]"), "read in +=");
        assert!(f.matches("function t:f/m").count() >= 4, "{f}");
        // reading comes back through a score
        assert!(f.contains("execute store result score"), "{f}");
    }

    #[test]
    fn context_blocks_accept_variable_coordinates() {
        let c = build_main("fn f(x: int, z: int) { positioned x 64 z { say(\"here\"); } }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("function t:f/m0 with storage t:macro"), "{f}");
        assert_eq!(fn_text(&c, "t:f/m0"), "$execute positioned $(h0) 64 $(h1) run function t:f/b0\n");
    }

    #[test]
    fn effect_values_can_be_variables() {
        let c = build_main("fn f(secs: int, level: int) { effect::give(@s, \"speed\", secs * 20, level); effect::give(@s, \"speed\", 10s, 1); }");
        let f = fn_text(&c, "t:f");
        // a variable duration is in ticks and rounded up to seconds: (ticks + 19) / 20
        assert_eq!(fn_text(&c, "t:f/m0"), "$effect give @s minecraft:speed $(h0) $(h1)\n");
        assert!(f.contains("scoreboard players add #f.t_0 t 19") && f.contains("scoreboard players operation #f.v_hv t /= #c_20 t"), "{f}");
        // literals stay static
        assert!(f.contains("effect give @s minecraft:speed 10 1"), "{f}");
    }

    #[test]
    fn a_literal_dollar_in_text_is_not_a_macro() {
        let c = build_main("fn f() { print(\"cost $(5)\"); say(\"cost $5\"); }");
        let f = fn_text(&c, "t:f");
        assert!(!f.contains("$(") || f.contains("\\u0024("), "{f}");
        assert!(!f.lines().any(|l| l.starts_with('$')), "{f}");
        assert!(build_err("fn f() { say(\"a $(b)\"); }").contains("`$(`"));
    }

    #[test]
    fn f_strings_become_one_tellraw_component_list() {
        let c = build_main("fn f(n: int, name: str, xs: list<int>) { print(f\"{name} has {n + 1} items {xs} {{ok}}\"); }");
        let f = fn_text(&c, "t:f");
        // ints are read through a `score` component, strings and lists through `nbt` ones
        assert!(f.contains("tellraw @a {text:\"\",extra:["), "{f}");
        assert!(f.contains(r#"{nbt:"f.v_name",storage:"t:vars"}"#), "{f}");
        assert!(f.contains(r##"{score:{name:"#f.t_0",objective:"t"}}"##), "{f}");
        assert!(f.contains(r#"{nbt:"f.v_xs",storage:"t:vars"}"#), "{f}");
        assert!(f.contains("\" has \"") && f.contains("\" items \"") && f.contains("\" {ok}\""), "{f}");
        // one message, and a single `tellraw`
        assert_eq!(f.matches("tellraw").count(), 1, "{f}");
    }

    #[test]
    fn f_string_temporaries_live_until_the_message_is_sent() {
        let c = build_main("fn g() -> str { return \"x\"; } fn f() { print(f\"{g()}{g()}\"); }");
        let f = fn_text(&c, "t:f");
        let tellraw = f.find("tellraw").expect(&f);
        // the two results sit in different temporaries (not reused too early) and are removed after
        let removes: Vec<usize> = f.match_indices("data remove storage t:vars f.t_").map(|(i, _)| i).collect();
        assert!(removes.len() >= 2 && removes.iter().rev().take(2).all(|i| *i > tellraw), "{f}");
        assert!(f.contains("f.t_0") && f.contains("f.t_1"), "{f}");
    }

    #[test]
    fn an_ignored_result_costs_nothing_and_a_used_one_stores_success() {
        // ignored: exactly the plain command
        let c = build_main("fn f() { tp(@s, 1, 64, 2); }");
        assert_eq!(fn_text(&c, "t:f"), "teleport @s 1 64 2\n");
        // used: cleared, then `execute store success` around the same command
        let c = build_main("fn f() -> bool { return tp(@s, 1, 64, 2); }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("scoreboard players set #f.t_0 t 0\nexecute store success score #f.t_0 t run teleport @s 1 64 2\n"), "{f}");
        assert!(f.contains("scoreboard players operation #ret t = #f.t_0 t"), "{f}");
    }

    #[test]
    fn a_used_result_with_variables_goes_through_a_return_run_helper() {
        let c = build_main("fn f(x: int) -> bool { let ok = tp(@s, x, 64, 2); return ok; }");
        let f = fn_text(&c, "t:f");
        // the helper forwards the command's own success, and the call stores it
        assert_eq!(fn_text(&c, "t:f/m0"), "$return run teleport @s $(h0) 64 2\n");
        assert!(f.contains("execute store success score #f.t_0 t run function t:f/m0 with storage t:macro"), "{f}");
    }

    #[test]
    fn equality_of_stored_values_copies_one_side_then_sets_it_from_the_other() {
        // `set` fails when it changes nothing, so "did it change" is "are they different"
        let c = build_main("fn f(a: str, b: str) -> bool { return a == b; }");
        let f = fn_text(&c, "t:f");
        assert!(
            f.contains("data modify storage t:vars f.t_0 set from storage t:vars f.v_a\nexecute store success score #f.t_0 t run data modify storage t:vars f.t_0 set from storage t:vars f.v_b\ndata remove storage t:vars f.t_0\nexecute store success score #ret t unless score #f.t_0 t matches 1\n"),
            "{f}"
        );
        let c = build_main("fn f(a: list<int>) -> bool { return a != [1, 2]; }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("run data modify storage t:vars f.t_0 set value [1,2]\ndata remove storage t:vars f.t_0\nexecute store success score #ret t if score #f.t_0 t matches 1\n"), "{f}");
        // a variable against a text literal is a single `if data` test, no scratch copy
        let c = build_main("fn f(a: str) -> bool { if a == \"hi\" { return true; } return a != \"yo\"; }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("execute if data storage t:vars {f:{v_a:\"hi\"}} if function t:f/b0"), "{f}");
        assert!(f.contains("execute store success score #ret t unless data storage t:vars {f:{v_a:\"yo\"}}"), "{f}");
        assert!(!f.contains("f.t_0"), "{f}");
        // two literals are compared by the compiler
        let c = build_main("fn f(a: str) -> bool { return a != \"b\" && \"a\" == \"a\"; }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("unless data storage t:vars {f:{v_a:\"b\"}}") && !f.contains("data modify") && f.matches("execute").count() == 1, "the literal comparison folds away: {f}");
        // an entity and a one-entity selector are compared as handles
        let c = build_main("fn f(e: Entity) -> bool { return e == @s; }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("execute as @s run function t:_ensure_id") && f.contains("if score #f.v_e t = #f.t_0 t"), "{f}");
    }

    #[test]
    fn enum_values_are_compounds_with_a_variant_tag_and_numbered_fields() {
        let c = build_main("enum Shape { Dot, Line(int, int), Named(str) } fn f(n: int) { let a: Option<int> = None; let b = Some(7); let c = Some(n + 1); let d = Shape::Line(1, n); let e = Shape::Named(\"x\"); let u: Result<(), str> = Ok(()); let r: Result<int, str> = Err(\"no\"); }");
        let f = fn_text(&c, "t:f");
        // constant payloads are one literal; the tag is the variant's index in declaration order
        assert!(f.contains("f.v_a set value {t:0}\n"), "{f}");
        assert!(f.contains("f.v_b set value {t:1,f0:7}\n"), "{f}");
        assert!(f.contains("f.v_e set value {t:2,f0:\"x\"}\n"), "{f}");
        // `()` stores nothing; `Ok` is variant 0 and `Err` variant 1
        assert!(f.contains("f.v_u set value {t:0}\n"), "{f}");
        assert!(f.contains("f.v_r set value {t:1,f0:\"no\"}\n"), "{f}");
        // a payload that is only known at run time is set into a temporary built from `{t:N}`
        assert!(f.contains("data modify storage t:vars f.v_tmp set value {t:1}\n"), "{f}");
        assert!(f.contains("data modify storage t:vars f.v_tmp.f0 set from storage t:vars f.t_0"), "{f}");
        assert!(f.contains("data modify storage t:vars f.v_tmp_2 set value {t:1,f0:1}\n") || f.contains("v_tmp_2 set value {t:1}"), "{f}");
        // globals and constants render the same layout
        let c = build_main("global g: Option<int> = Some(3); global h: Result<(), int> = Ok(());");
        let init = fn_text(&c, "t:_init");
        assert!(init.contains("storage t:globals g run data modify storage t:globals g set value {t:1,f0:3}"), "{init}");
        assert!(init.contains("set value {t:0}"), "{init}");
    }

    #[test]
    fn selector_variables_become_macro_holes() {
        let c = build_main("fn f(t: str, r: int) { for e in @e[tag=$t, distance=..${r + 1}] { kill(e); } kill(@a[tag=$t, limit=1]); if exists(@e[type=$t]) { print(1); } }");
        let f = fn_text(&c, "t:f");
        // the loop runs `as` through a macro; the strings and numbers are passed in storage
        assert!(f.contains("data modify storage t:macro h0 set from storage t:vars f.v_t"), "{f}");
        let helpers: Vec<String> = (0..6).filter_map(|i| c.pack.function(&ResourceLocation::parse(&format!("t:f/m{i}")).unwrap()).map(|h| h.render())).collect();
        let has = |line: &str| helpers.iter().any(|h| h.contains(line));
        assert!(has("$execute as @e[tag=$(h0),distance=..$(h1)] run function t:f/b0\n"), "{helpers:?}");
        assert!(has("$kill @a[tag=$(h0),limit=1]\n"), "{helpers:?}");
        assert!(has("$return run execute if entity @e[type=$(h0)]"), "{helpers:?}");
        // a selector without variables is unchanged
        let c = build_main("fn f() { kill(@e[tag=a]); }");
        assert_eq!(fn_text(&c, "t:f"), "kill @e[tag=a]\n");
    }

    #[test]
    fn exists_and_tag_has_still_test_directly() {
        let c = build_main("fn f() -> bool { return exists(@e[tag=a]) && tag::has(@s, \"b\"); }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("execute store success score") && f.contains("if entity @e[tag=a] if entity @s[tag=b]"), "{f}");
    }

    #[test]
    fn a_selector_becomes_a_handle_through_the_id_helper() {
        let c = build_main("fn f() -> Entity { let me = @p; return me; }");
        let f = fn_text(&c, "t:f");
        // the id is read back from #new_id after the helper ran for the matched entity
        assert!(f.contains("scoreboard players set #new_id t 0\nexecute as @p run function t:_ensure_id\n"), "{f}");
        assert!(f.contains("execute as @p run function t:_ensure_id\nscoreboard players operation #f.v_me t = #new_id t\n"), "{f}");
        assert_eq!(
            fn_text(&c, "t:_ensure_id").lines().collect::<Vec<_>>(),
            ["# generated by slab: the executing entity's id, allocated on first use", "execute unless score @s t_id matches 1.. run function t:_alloc_id", "scoreboard players operation #new_id t = @s t_id"]
        );
        assert!(fn_text(&c, "t:_alloc_id").contains("scoreboard players operation @s t_id = #next_id t"));
        let init = fn_text(&c, "t:_init");
        assert!(init.contains("scoreboard objectives add t_id dummy"), "{init}");
        assert!(init.contains("execute unless score #next_id t matches -2147483648..2147483647 run scoreboard players set #next_id t 0"), "{init}");
        assert!(fn_text(&c, "t:_uninstall").contains("scoreboard objectives remove t_id"));
    }

    #[test]
    fn programs_without_entities_get_no_entity_machinery() {
        let c = build_main("fn f() { kill(@e[tag=x]); }");
        assert!(c.pack.function(&ResourceLocation::parse("t:_ensure_id").unwrap()).is_none());
        assert!(!fn_text(&c, "t:_init").contains("t_id"));
    }

    #[test]
    fn summon_as_a_value_creates_and_numbers_the_entity() {
        let c = build_main("fn f(x: int) -> Entity { return summon(\"minecraft:zombie\", x, 64, 3); }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("scoreboard players set #new_id t 0"), "{f}");
        assert_eq!(fn_text(&c, "t:f/m0"), "$execute positioned $(h0) 64 3 summon minecraft:zombie run function t:_new_entity\n");
        let new = fn_text(&c, "t:_new_entity");
        assert!(new.contains("function t:_alloc_id") && new.contains("tag @s add t.owned") && new.contains("scoreboard players operation #new_id t = @s t_id"), "{new}");
        // the player/ownership tag is only put on entities the program created
        assert!(!fn_text(&c, "t:_ensure_id").contains("t.owned"));
        assert!(fn_text(&c, "t:_uninstall").contains("kill @e[tag=t.owned]"));
    }

    #[test]
    fn a_handle_is_found_again_by_its_id() {
        let c = build_main("fn f(b: Entity) -> bool { tp(b, 1, 2, 3); effect::give(b, \"speed\", 10s); return kill(b); }");
        let f = fn_text(&c, "t:f");
        assert_eq!(fn_text(&c, "t:f/m0"), "$teleport @e[scores={t_id=$(h0)},limit=1] 1 2 3\n");
        assert_eq!(fn_text(&c, "t:f/m1"), "$effect give @e[scores={t_id=$(h0)},limit=1] minecraft:speed 10\n");
        assert_eq!(fn_text(&c, "t:f/m2"), "$return run kill @e[scores={t_id=$(h0)},limit=1]\n");
        // every use feeds the handle's value in
        assert!(f.contains("execute store result storage t:macro h0 int 1 run scoreboard players get #f.v_b t"), "{f}");
    }

    #[test]
    fn handles_work_as_context_targets_scores_and_existence_tests() {
        let c = build_main("objective pts = \"dummy\"; fn f(b: Entity) { as b at b { say(\"x\"); } pts[b] += 2; print(exists(b)); }");
        let f = fn_text(&c, "t:f");
        assert!(f.contains("function t:f/m0 with storage t:macro"), "{f}");
        assert_eq!(fn_text(&c, "t:f/m0"), "$execute as @e[scores={t_id=$(h0)},limit=1] at @e[scores={t_id=$(h1)},limit=1] run function t:f/b0\n");
        assert_eq!(fn_text(&c, "t:f/m1"), "$scoreboard players add @e[scores={t_id=$(h0)},limit=1] pts 2\n");
        assert_eq!(fn_text(&c, "t:f/m2"), "$return run execute if entity @e[scores={t_id=$(h0)},limit=1]\n");
    }

    #[test]
    fn handles_are_ints_inside_lists_and_structs() {
        let c = build_main("struct S { e: Entity } fn f(a: Entity) { let xs = [a, a]; let s = S { e: a }; kill(xs[0]); kill(s.e); }");
        let f = fn_text(&c, "t:f");
        // stored as int tags like any int
        assert!(f.contains("execute store result storage t:vars f.t_0 int 1 run scoreboard players get #f.v_a t") || f.contains("int 1 run scoreboard players get #f.v_a t"), "{f}");
    }

    #[test]
    fn namespaces() {
        assert_eq!(default_namespace(Path::new("/tmp/My-Cool Game")), "my_cool_game");
        assert!(validate_namespace("ok_name").is_ok());
        assert!(validate_namespace("Bad").is_err());
        assert!(validate_namespace("a b").is_err());
        let d = Dir::new("ns", &[("main.slab", "fn main() {}")]);
        let Err(e) = compile(&d.0, &Options { namespace: Some("Bad".into()), ..Options::default() }) else { panic!() };
        assert!(e.render().contains("invalid namespace"));
    }

    #[test]
    fn backend_limits_are_reported_with_spans() {
        let d = Dir::new("limit", &[("main.slab", "fn f(i: int) { let mut xs = [[1], [2]]; xs[i].push(3); }")]);
        let Err(e) = compile(&d.0, &Options::default()) else { panic!() };
        let text = e.render();
        assert!(text.contains("cannot `push` to a list chosen by a variable index"), "{text}");
        assert!(text.contains("main.slab:1:"), "{text}");
    }

    #[test]
    fn check_without_generating() {
        let d = Dir::new("check", &[("main.slab", "import u; fn main() { u::f(); }"), ("u.slab", "pub fn f() {}")]);
        let c = check(&d.0).ok().unwrap();
        assert_eq!((c.modules, c.functions), (2, 2));
    }
}
