use super::check;
use super::hir::*;
use super::ty::Ty;
use crate::syntax::parse;

/// Every test program starts with these imports (on the same line, so positions don't shift).
const PRELUDE: &str = "import std::*; import game::*; ";

/// (errors, warnings) messages of a program.
fn run(src: &str) -> (Vec<String>, Vec<String>) {
    let src = &format!("{PRELUDE}{src}");
    let program = parse(src).unwrap_or_else(|e| panic!("parse error: {}\n{src}", e.message));
    match check(&program) {
        Ok(c) => (Vec::new(), c.warnings.into_iter().map(|d| d.message).collect()),
        Err(r) => (r.errors.into_iter().map(|d| d.message).collect(), r.warnings.into_iter().map(|d| d.message).collect()),
    }
}

fn errors(src: &str) -> Vec<String> {
    run(src).0
}

fn warnings(src: &str) -> Vec<String> {
    run(src).1
}

fn ok(src: &str) -> Checked {
    let src = &format!("{PRELUDE}{src}");
    let program = parse(src).unwrap_or_else(|e| panic!("parse error: {}\n{src}", e.message));
    match check(&program) {
        Ok(c) => c,
        Err(r) => panic!("unexpected errors: {:#?}\n{src}", r.errors.iter().map(|d| d.message.clone()).collect::<Vec<_>>()),
    }
}

use super::Checked;

/// Asserts that checking `src` yields exactly one error containing `needle`.
fn one_error(src: &str, needle: &str) {
    let e = errors(src);
    assert_eq!(e.len(), 1, "expected one error containing {needle:?}, got {e:#?}\n{src}");
    assert!(e[0].contains(needle), "expected {needle:?} in {:?}\n{src}", e[0]);
}

fn in_fn(body: &str) -> String {
    format!("fn t() {{ {body} }}")
}

// ------------------------------------------------------------------ happy paths

#[test]
fn fib_produces_typed_hir() {
    let c = ok("fn fib(n: int) -> int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }");
    let f = &c.hir.fns[0];
    assert_eq!(f.name, "fib");
    assert_eq!(f.ret, Ty::Int);
    assert_eq!(f.params.len(), 1);
    assert_eq!(f.locals[0].name, "n");
    assert!(f.locals[0].mutable, "parameters can be assigned");
    match &f.body.stmts[1] {
        Stmt::Return(Some(e)) => {
            assert_eq!(e.ty, Ty::Int);
            match &e.kind {
                ExprKind::Binary { lhs, .. } => assert!(matches!(&lhs.kind, ExprKind::Call { func: FuncId(0), .. })),
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn examples_check() {
    for name in ["fib", "tour"] {
        let src = std::fs::read_to_string(format!("{}/examples/slab/{name}.slab", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let (errs, _) = run(&src);
        assert!(errs.is_empty(), "{name}.slab: {errs:#?}");
    }
}

#[test]
fn constants_are_inlined_and_folded() {
    let c = ok("const A: int = 2 + 3 * 4;
                const B: int = A - 4;
                const NAME: str = \"x\";
                const XS: list<int> = [A, B];
                fn f() -> int { let a = A; return B; }");
    let f = &c.hir.fns[0];
    match &f.body.stmts[0] {
        Stmt::Let { init, .. } => assert!(matches!(init.kind, ExprKind::Int(14))),
        other => panic!("{other:?}"),
    }
    match &f.body.stmts[1] {
        Stmt::Return(Some(e)) => assert!(matches!(e.kind, ExprKind::Int(10))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn constants_use_floored_division_and_negative_literals() {
    let c = ok("const A: int = -7 / 2; const B: int = -7 % 2; const C: int = -2147483648; fn f() -> int { return A + B + C - C; }");
    let _ = c;
    one_error("const A: int = 2147483648;", "out of range");
}

#[test]
fn globals_get_const_initialisers() {
    let c = ok("struct P { x: int, n: str } global g: P = P { x: 1 + 1, n: \"a\" }; global xs: list<int> = [1, 2]; global e: list<str> = [];");
    assert_eq!(c.hir.globals.len(), 3);
    assert!(matches!(&c.hir.globals[0].init, ConstVal::Struct(_, v) if v == &[ConstVal::Int(2), ConstVal::Str("a".into())]));
    assert_eq!(c.hir.globals[1].init, ConstVal::List(vec![ConstVal::Int(1), ConstVal::Int(2)]));
    assert_eq!(c.hir.globals[2].init, ConstVal::List(vec![]));
}

#[test]
fn entry_attributes() {
    let c = ok("#[load] fn a() {} #[tick] fn b() {} #[every(1s)] fn c() {} #[every(5)] fn d() {} fn e() {}");
    let entries: Vec<_> = c.hir.fns.iter().map(|f| f.entry).collect();
    assert_eq!(entries, [Some(EntryKind::Load), Some(EntryKind::Tick), Some(EntryKind::Every(20)), Some(EntryKind::Every(5)), None]);
}

#[test]
fn integer_extremes() {
    ok(&in_fn("let a = -2147483648; let b = 2147483647; print(a + b);"));
    one_error(&in_fn("let a = 2147483648; print(a);"), "out of range");
}

// ------------------------------------------------------------- names and types

#[test]
fn unknown_names() {
    one_error(&in_fn("print(x);"), "cannot find `x`");
    one_error(&in_fn("nope();"), "cannot find function `nope`");
    one_error("fn f(a: Nope) {}", "unknown type `Nope`");
    one_error("fn f() -> int { return g; } fn g() {}", "used as a value");
    one_error(&in_fn("let x = print;"), "is used as a value");
}

#[test]
fn let_inference_and_annotations() {
    ok(&in_fn("let a = 1; let b: int = a; let c = true; let d = \"s\"; let e = [1, 2]; print(a + b); print(c); print(d); print(e);"));
    one_error(&in_fn("let a: int = \"s\"; print(a);"), "expected `int`, found `str`");
    one_error(&in_fn("let a: bool = 1; print(a);"), "expected `bool`, found `int`");
    one_error(&in_fn("let x = []; print(x);"), "cannot infer the type of an empty list");
    ok(&in_fn("let x: list<int> = []; print(x);"));
    one_error(&in_fn("let s = @a; print(1);"), "can match several entities");
    one_error("fn g() {} fn f() { let x = g(); }", "no value to store");
}

#[test]
fn shadowing_uses_the_outer_value_in_the_initialiser() {
    let c = ok("fn f() -> int { let x = 1; { let x = x + 1; print(x); } return x; }");
    let f = &c.hir.fns[0];
    assert_eq!(f.locals.len(), 2);
    match &f.body.stmts[1] {
        Stmt::Block(b) => match &b.stmts[0] {
            Stmt::Let { local, init } => {
                assert_eq!(local.0, 1);
                match &init.kind {
                    ExprKind::Binary { lhs, .. } => assert!(matches!(lhs.kind, ExprKind::Local(LocalId(0)))),
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn operators_are_type_checked() {
    ok(&in_fn("let a = 1 + 2 * 3 % 4 - 5 / 6; let b = a < 2 && a >= 1 || !(a == 3); print(b);"));
    one_error(&in_fn("let a = 1 + true; print(a);"), "cannot apply `+` to `int` and `bool`");
    ok(&in_fn("let a = \"a\" + \"b\"; print(a);"));
    one_error(&in_fn("let a = \"a\" + 1; print(a);"), "cannot apply `+` to `str` and `int`");
    one_error(&in_fn("let a = \"a\" - \"b\"; print(a);"), "cannot apply `-` to `str` and `str`");
    one_error(&in_fn("let a = 1 && 2; print(a);"), "cannot apply `&&`");
    one_error(&in_fn("let a = !1; print(a);"), "`!` needs `bool`");
    one_error(&in_fn("let a = -true; print(a);"), "`-` needs `int`");
    one_error(&in_fn("let a = 1 == true; print(a);"), "cannot apply `==` to `int` and `bool`");
    one_error(&in_fn("let a = \"x\" == \"y\"; print(a);"), "not supported yet");
    one_error(&in_fn("if 1 { }"), "expected `bool`, found `int`");
    one_error(&in_fn("let a = 1.5;"), "floating point");
}

#[test]
fn one_mistake_is_reported_once() {
    // `x` is unknown; every use of the poisoned value is silent
    let e = errors(&in_fn("let y = x + 1; let z = y * 2 - x; if z < 3 { print(z); }"));
    assert_eq!(e.len(), 2, "{e:?}"); // `x` twice (two different places), nothing else
    assert!(e.iter().all(|m| m.contains("cannot find `x`")));
}

// ---------------------------------------------------------------- mutability

#[test]
fn mutability() {
    ok(&in_fn("let mut x = 1; x = 2; x += 3; print(x);"));
    one_error(&in_fn("let x = 1; x = 2; print(x);"), "not mutable");
    one_error(&in_fn("for i in 0..3 { i = 5; }"), "not mutable");
    one_error("const C: int = 1; fn f() { C = 2; }", "cannot assign to this expression");
    ok("fn f(n: int) -> int { n -= 1; return n; }");
    ok("global g: int = 0; fn f() { g += 1; }");
    one_error("struct P { x: int } fn f() { let p = P { x: 1 }; p.x = 2; }", "not mutable");
    ok("struct P { x: int } fn f() { let mut p = P { x: 1 }; p.x = 2; print(p.x); }");
    one_error(&in_fn("let xs = [1]; xs.push(2);"), "not mutable");
    // an lvalue-shaped expression whose root is not a variable
    one_error("fn g() -> list<int> { return [1]; } fn f() { g()[0] = 2; }", "cannot assign");
}

#[test]
fn compound_assignment_needs_ints() {
    ok(&in_fn("let mut s = \"a\"; s += \"b\";"));
    one_error(&in_fn("let mut s = \"a\"; s += 1;"), "expected `str`, found `int`");
    one_error(&in_fn("let mut s = \"a\"; s -= \"b\";"), "needs an `int` target");
    one_error(&in_fn("let mut n = 1; n += true;"), "expected `int`, found `bool`");
    one_error(&in_fn("let mut n = 1; n = \"x\";"), "expected `int`, found `str`");
}

// ----------------------------------------------------------- functions and flow

#[test]
fn calls_check_arity_and_argument_types() {
    one_error("fn g(a: int, b: str) {} fn f() { g(1); }", "function `g` takes 2 arguments but 1 was supplied");
    one_error("fn g() {} fn f() { g(1, 2); }", "takes 0 arguments but 2 were supplied");
    one_error("fn g(a: int) {} fn f() { g(\"x\"); }", "expected `int`, found `str`");
    one_error("fn g(a: int) {} fn f() { let x = 1; x(2); }", "`x` is not a function");
    ok("fn g(a: int, b: list<str>) -> bool { return a > 0; } fn f() { let r = g(1, []); print(r); }");
}

#[test]
fn return_checking() {
    one_error("fn f() -> int { return \"a\"; }", "expected `int`, found `str`");
    one_error("fn f() -> int { return; }", "`return` needs a value of type `int`");
    one_error("fn f() { return 1; }", "returns nothing");
    ok("fn f() { return; }");
}

#[test]
fn missing_return_is_detected_by_flow_analysis() {
    one_error("fn f(a: int) -> int { if a > 0 { return 1; } }", "does not return a value on every path");
    one_error("fn f(a: int) -> int { while a > 0 { return 1; } }", "does not return a value on every path");
    one_error("fn f() -> int { }", "does not return a value on every path");
    ok("fn f(a: int) -> int { if a > 0 { return 1; } else { return 2; } }");
    ok("fn f(a: int) -> int { if a > 0 { return 1; } else if a < 0 { return 2; } else { return 3; } }");
    ok("fn f() -> int { loop { return 1; } }");
    ok("fn f() -> int { { return 1; } }");
    // a `loop` with a break can end, so it does not count as returning
    one_error("fn f() -> int { loop { break; } }", "does not return a value on every path");
    // a break in a nested loop only leaves the nested loop
    ok("fn f() -> int { loop { loop { break; } return 1; } }");
}

#[test]
fn break_and_continue_placement() {
    ok(&in_fn("loop { break; } while true { continue; } for i in 0..2 { if i == 1 { break; } }"));
    one_error(&in_fn("break;"), "`break` outside of a loop");
    one_error(&in_fn("continue;"), "`continue` outside of a loop");
    one_error(&in_fn("loop { as @a { break; } }"), "`break` cannot leave a context block");
}

#[test]
fn for_loops() {
    ok(&in_fn("for i in 0..10 { print(i); } for i in 1..=3 { print(i); } let xs = [\"a\"]; for x in xs { print(x); }"));
    one_error(&in_fn("for i in 0.. { }"), "needs both a start and an end");
    one_error(&in_fn("for i in 0..\"a\" { }"), "expected `int`, found `str`");
    one_error(&in_fn("for i in 5 { }"), "cannot iterate over `int`");
    one_error(&in_fn("let r = 0..3;"), "only allowed as the subject of a `for`");
}

#[test]
fn warning_rules() {
    let w = warnings(&in_fn("return; print(1);"));
    assert_eq!(w, ["unreachable statement"]);
    let w = warnings(&in_fn("let unused = 1; let _quiet = 2; let used = 3; print(used);"));
    assert_eq!(w, ["unused variable `unused`"]);
    let w = warnings(&in_fn("1 + 2;"));
    assert_eq!(w, ["this expression has no effect"]);
    let w = warnings("fn f(unused_param: int) {}");
    assert!(w.is_empty(), "parameters never warn: {w:?}");
    // a `break` makes the rest of the loop body unreachable too
    assert_eq!(warnings(&in_fn("loop { break; print(1); }")), ["unreachable statement"]);
    // warnings do not stop compilation
    assert!(run(&in_fn("let a = 1;")).0.is_empty());
}

#[test]
fn errors_are_sorted_by_position() {
    let program = parse(&format!("{PRELUDE}fn a() {{ zzz(); }}\nfn b() {{ let x: int = \"s\"; print(x); }}\nconst C: int = \"c\";")).unwrap();
    let Err(r) = check(&program) else { panic!() };
    let starts: Vec<u32> = r.errors.iter().map(|d| d.span.start).collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted);
    assert_eq!(r.errors.len(), 3);
}

// -------------------------------------------------------------------- structs

#[test]
fn structs() {
    let decl = "struct Arena { name: str, round: int, winners: list<str> }";
    ok(&format!("{decl} fn f() {{ let mut a = Arena {{ name: \"m\", round: 0, winners: [] }}; a.round += 1; a.winners.push(\"x\"); print(a.name); }}"));
    one_error(&format!("{decl} fn f() {{ let a = Arena {{ name: \"m\", round: 0 }}; print(a.round); }}"), "missing field(s) in `Arena`: winners");
    one_error(&format!("{decl} fn f() {{ let a = Arena {{ name: \"m\", round: 0, winners: [], extra: 1 }}; print(a.round); }}"), "has no field `extra`");
    one_error(&format!("{decl} fn f() {{ let a = Arena {{ name: 1, round: 0, winners: [] }}; print(a.round); }}"), "expected `str`, found `int`");
    one_error(&format!("{decl} fn f() {{ let a = Arena {{ name: \"x\", name: \"y\", round: 0, winners: [] }}; print(a.round); }}"), "set twice");
    one_error(&format!("{decl} fn f(a: Arena) {{ print(a.nope); }}"), "has no field `nope`");
    one_error("fn f() { let x = 1; print(x.y); }", "`int` has no field `y`");
    one_error("struct S { s: S }", "contains itself");
    one_error("struct A { b: B } struct B { a: A }", "contains itself");
    one_error("struct S { a: int, a: int }", "declared twice");
    one_error("struct S {} struct S {}", "defined twice");
}

#[test]
fn anonymous_compounds_need_a_struct_type() {
    ok("struct C { a: int } fn f() { let c: C = { a: 1 }; print(c.a); }");
    one_error(&in_fn("let c = { a: 1 }; print(c);"), "needs a struct type");
    ok("struct C { a: int } fn g(c: C) {} fn f() { g({ a: 2 }); }");
}

// ---------------------------------------------------------------------- lists

#[test]
fn lists_and_methods() {
    ok(&in_fn("let mut xs = [1, 2]; xs.push(3); let n = xs.len(); let first = xs[0]; let last = xs[-1]; print(n + first + last); let s = \"abc\"; print(s.len());"));
    one_error(&in_fn("let mut xs = [1]; xs.push(\"a\");"), "expected `int`, found `str`");
    one_error(&in_fn("let xs = [1, \"a\"]; print(xs);"), "expected `int`, found `str`");
    one_error(&in_fn("let xs = [1]; print(xs[\"a\"]);"), "expected `int`, found `str`");
    one_error(&in_fn("let x = 1; print(x[0]);"), "cannot index into `int`");
    one_error(&in_fn("let xs = [1]; xs.pop();"), "has no method `pop`");
    one_error(&in_fn("let mut xs = [1]; xs.push(1, 2);"), "method `push` takes 1 argument but 2 were supplied");
    one_error(&in_fn("let xs = [1]; print(xs.len);"), "has no field `len`");
}

// ------------------------------------------------------- selectors and builtins

#[test]
fn selectors_are_validated_and_typed() {
    let c = ok(&in_fn("kill(@e[type=zombie, tag=!boss, distance=..10, limit=1]);"));
    match &c.hir.fns[0].body.stmts[0] {
        Stmt::Expr(Expr { kind: ExprKind::Builtin { name: "kill", args }, .. }) => match &args[0].kind {
            ExprKind::Selector(s) => assert_eq!(s.to_string(), "@e[type=minecraft:zombie,tag=!boss,distance=..10,limit=1]"),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    let e = errors(&in_fn("kill(@e[tpye=zombie, limit=0]);"));
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(e[0].contains("unknown selector argument `tpye`"));
    assert!(e[1].contains("positive integer"));
    one_error(&in_fn("kill(1);"), "expected a `selector` or an `Entity`, found `int`");
    one_error(&in_fn("let n = @s + 1;"), "cannot apply `+`");
}

#[test]
fn builtins() {
    ok(&in_fn("print(1); print(\"s\"); print(true); say(\"hi\"); tag::add(@s, \"x\"); tag::remove(@s, \"x\"); let b = tag::has(@s, \"x\"); print(b); print(exists(@a)); effect::give(@s, \"speed\", 10s); effect::give(@s, \"speed\", 10s, 2); effect::clear(@s); tp(@s, ~, ~1, ~-1); tp(@s, 1, 2, 3); setblock(~, ~-1, ~, \"minecraft:stone\"); summon(\"minecraft:pig\"); summon(\"minecraft:pig\", ~, ~, ~);"));
    one_error(&in_fn("say();"), "`game::say` takes 1 argument but 0 were supplied");
    one_error(&in_fn("effect::give(@s);"), "takes 2 to 4 arguments but 1 was supplied");
    one_error(&in_fn("tag::add(@s, \"a b\");"), "invalid tag name");
    one_error(&in_fn("summon(\"Bad Id\");"), "invalid id");
    one_error(&in_fn("effect::give(@s, \"speed\", 0);"), "between 1 and 20000000 ticks");
    one_error(&in_fn("effect::give(@s, \"speed\", 10, 300);"), "amplifier");
    // text arguments may be computed at run time; a literal is still checked now
    ok(&in_fn("let m = \"a\"; say(m); tag::add(@s, m); setblock(~, ~, ~, m); summon(m);"));
    one_error(&in_fn("tag::add(@s, \"bad tag\");"), "tag");
    // arguments that must be known at compile time reject runtime values
    one_error(&in_fn("let m = \"a\"; if tag::has(@s, m) { }"), "`m` is not a constant");
}

#[test]
fn builtin_const_arguments_accept_constants() {
    ok("const MSG: str = \"hello\"; const T: int = 5s; fn f() { say(MSG); effect::give(@s, \"speed\", T); }");
    ok("global g: str = \"a\"; fn f() { say(g); }");
}

#[test]
fn world_acting_builtins_return_whether_they_worked() {
    ok(&in_fn("let a = tp(@s, 1, 2, 3); let b = kill(@e[tag=x]); let c = tag::add(@s, \"t\"); let d = effect::give(@s, \"speed\", 10s); let e = setblock(1, 2, 3, \"stone\"); print(a && b && c && d && e);"));
    ok(&in_fn("if !tp(@s, 0, 70, 0) { print(\"gone\"); } while kill(@e[tag=x]) { }"));
    // ignoring the result is fine and warns about nothing
    assert!(warnings(&in_fn("tp(@s, 1, 2, 3); kill(@s);")).is_empty());
    // print and say have no value
    one_error(&in_fn("let x = print(1);"), "no value to store");
    one_error(&in_fn("let x = say(\"a\");"), "no value to store");
    one_error(&in_fn("let n: int = tp(@s, 1, 2, 3);"), "expected `int`, found `bool`");
}

#[test]
fn builtin_names_are_ordinary_names() {
    // a glob import never overrides what the module defines itself
    ok("fn print() {} global tag: int = 0; fn effect() {}");
    one_error("import game::kill; fn kill() {}", "already used");
    // and nothing is in scope until it is imported
    let program = parse("fn f() { print(1); }").unwrap();
    let Err(r) = check(&program) else { panic!("expected an error") };
    assert!(r.errors[0].message.contains("cannot find function `print`"), "{:?}", r.errors);
    assert!(r.errors[0].hint.as_deref().unwrap_or("").contains("std::print"), "{:?}", r.errors);
}

#[test]
fn builtin_modules_use_double_colon_paths() {
    // fully qualified, with no import at all
    let program = parse("fn f() { std::print(1); game::say(\"x\"); game::tag::add(@s, \"t\"); let e: game::Entity = game::summon(\"minecraft:cow\"); game::effect::clear(e); }").unwrap();
    assert!(check(&program).is_ok());
    // module aliases, item imports and globs
    ok("import game::tag; import game::effect as fx; import game::summon as spawn; import game::Entity as Mob; fn f() { tag::add(@s, \"t\"); fx::clear(@s); let m: Mob = spawn(\"minecraft:cow\"); kill(m); }");
    ok("import game::tag::{add, has}; fn f() { add(@s, \"t\"); print(has(@s, \"t\")); }");
    // mistakes
    one_error(&in_fn("game::nope();"), "module `game` has no function `nope`");
    one_error(&in_fn("game::tag::nope(@s);"), "module `game::tag` has no function `nope`");
    one_error(&in_fn("let e: game::Nope = 1;"), "module `game` has no type `Nope`");
    one_error(&in_fn("nope::say(\"x\");"), "cannot find module `nope`");
    one_error(&in_fn("game.say(\"x\");"), "`game` is a module");
    one_error(&in_fn("let e = game::Entity { };"), "cannot be built with a struct literal");
    one_error("import game::nope;", "cannot find module `game::nope`");
    one_error("import game::{nope};", "has no item `nope`");
    one_error("import game::say::*;", "is not a module");
    one_error(&in_fn("let x = game;"), "is a module, not a value");
}

#[test]
fn unknown_builtin_members() {
    one_error(&in_fn("tag::nope(@s);"), "module `game::tag` has no function `nope`");
    one_error(&in_fn("let x = tag::add;"), "is used as a value");
    one_error(&in_fn("effect.give(@s, \"speed\", 10);"), "`effect` is a module: its functions are called with `::`");
}

// ------------------------------------------------------------------ objectives

#[test]
fn objectives() {
    ok("objective kills = \"playerKillCount\"; fn f() { kills[@s] += 1; let k = kills[@s]; kills[\"#total\"] = k; print(kills[@a[tag=x, limit=1]]); }");
    one_error("objective kills = \"dummy\"; fn f() { print(kills); }", "needs an index");
    one_error("objective kills = \"dummy\"; fn f() { kills[1] = 2; }", "a score holder is a selector or a name");
    one_error("objective kills = \"dummy\"; fn f() { kills[\"bad name\"] = 2; }", "invalid score holder name");
    one_error("objective kills = 5;", "must be a string literal");
    one_error("objective kills = \"dummy\"; objective kills = \"dummy\";", "defined more than once");
}

// --------------------------------------------------------------- context blocks

#[test]
fn context_blocks() {
    ok(&in_fn("as @a[tag=ready] at @s positioned ~ ~1 ~ rotated 0 90 align xz anchored eyes { say(\"x\"); } in \"minecraft:the_nether\" { } facing 0 64 0 { }"));
    one_error(&in_fn("as 1 { }"), "expected a `selector` or an `Entity`, found `int`");
    one_error(&in_fn("positioned \"a\" 0 0 { }"), "expected a coordinate");
    one_error(&in_fn("align xx { }"), "invalid axes `xx`");
    one_error(&in_fn("align w { }"), "invalid axes");
    one_error(&in_fn("anchored head { }"), "`eyes` or `feet`");
    one_error(&in_fn("in \"Bad Dim\" { }"), "invalid dimension id");
    one_error(&in_fn("as @a { return; }"), "`return` is not allowed inside a context block");
    // locals declared outside are visible inside
    ok(&in_fn("let mut n = 0; as @a { n += 1; } print(n);"));
}

#[test]
fn context_axes_are_normalised() {
    let c = ok(&in_fn("align zx { }"));
    match &c.hir.fns[0].body.stmts[0] {
        Stmt::Context { clauses, .. } => assert_eq!(clauses[0], CtxClause::Align("xz".into())),
        other => panic!("{other:?}"),
    }
}

// ------------------------------------------------------------ items and attrs

#[test]
fn duplicate_definitions() {
    one_error("fn f() {} fn f() {}", "defined more than once");
    one_error("global a: int = 0; fn a() {}", "defined more than once");
    one_error("const a: int = 0; global a: int = 1;", "defined more than once");
    one_error("fn f(a: int, a: int) {}", "declared twice");
}

#[test]
fn entry_attribute_errors() {
    one_error("#[nope] fn f() {}", "unknown attribute `nope`");
    one_error("#[load] fn f(a: int) {}", "cannot take parameters");
    one_error("#[tick] fn f() -> int { return 1; }", "cannot return a value");
    one_error("#[load] #[tick] fn f() {}", "only one of");
    one_error("#[every(0)] fn f() {}", "at least 1 tick");
    one_error("#[every] fn f() {}", "exactly one argument");
    one_error("#[every(\"a\")] fn f() {}", "needs an integer number of ticks");
    one_error("#[load(1)] fn f() {}", "takes no arguments");
    one_error("#[load] global g: int = 0;", "only be applied to functions");
}

#[test]
fn constant_errors() {
    one_error("const A: int = B; const B: int = A;", "depends on itself");
    one_error("const A: int = 1 / 0;", "division by zero");
    one_error("const A: int = 2147483647 + 1;", "overflows");
    one_error("const A: int = 1 + true;", "cannot apply");
    one_error("global g: int = 0; const A: int = g;", "is a global, not a constant");
    one_error("global a: int = 1; global b: int = a;", "is a global, not a constant");
    one_error("fn f() -> int { return 1; } global g: int = f();", "not a constant expression");
    one_error("const A: str = 1;", "expected `str`, found `int`");
}

#[test]
fn bad_types() {
    one_error("fn f(a: list) {}", "`list` takes exactly one type argument");
    one_error("fn f(a: int<str>) {}", "does not take type arguments");
    one_error("struct S { a: list<Nope> }", "unknown type `Nope`");
    one_error("global g: Entity = 0;", "expected `Entity`, found `int`");
}

// ------------------------------------------------------------------ entities

#[test]
fn entity_values() {
    // a single-entity selector becomes a handle; `summon` returns one
    ok(&in_fn("let me = @p; let boss = summon(\"minecraft:zombie\", 1, 64, 2); let e: Entity = @s; let n = @n[type=zombie]; let f = @a[limit=1]; print(me); print(boss == e && n != f);"));
    ok("struct Fight { boss: Entity, helpers: list<Entity> } fn pick() -> Entity { return @r; } fn hit(e: Entity) -> bool { return kill(e); } fn f() { let fight = Fight { boss: @p, helpers: [@s, pick()] }; hit(fight.boss); for h in fight.helpers { tp(h, 0, 70, 0); } }");
    // and usable as a target anywhere a selector is
    ok(&in_fn("let b = summon(\"minecraft:pig\"); kill(b); tp(b, 1, 2, 3); effect::give(b, \"speed\", 10s); tag::add(b, \"x\"); as b at b { say(\"hi\"); } print(exists(b)); print(tag::has(b, \"x\"));"));
    ok("objective pts = \"dummy\"; fn f(b: Entity) { pts[b] += 1; print(pts[b]); }");
    // it is its own type
    one_error(&in_fn("let e: Entity = 5; print(e);"), "expected `Entity`, found `int`");
    one_error(&in_fn("let e = @p; let n: int = e; print(n);"), "expected `int`, found `Entity`");
    one_error(&in_fn("let e = @p; let b = e + 1; print(b);"), "cannot apply `+` to `Entity` and `int`");
    // a selector that can match several entities is not one entity
    one_error(&in_fn("let all: Entity = @a; print(all);"), "can match several entities");
    one_error("fn f(e: Entity) {} fn g() { f(@e[type=pig]); }", "can match several entities");
    // a plain selector value is still not storable
    one_error(&in_fn("let a = @a[tag=x]; print(1);"), "can match several entities");
}

// ------------------------------------------------------------------ iterating entities

#[test]
fn for_over_a_selector_binds_the_entity() {
    // the loop variable is the entity the body runs as: an `entity` value
    let c = ok("objective pts = \"dummy\"; fn f() { for z in @e[type=zombie] { tp(z, 0, 70, 0); pts[z] += 1; } for _ in @a { print(\"one round\"); } }");
    let f = &c.hir.fns[0];
    let mut saw_ctx = false;
    let mut total = [0, 0];
    for s in &f.body.stmts {
        if let Stmt::Context { clauses, body } = s {
            saw_ctx = true;
            assert_eq!(clauses.len(), 1);
            assert!(matches!(clauses[0], CtxClause::As(_)));
            // the body uses the bound name: `tp(z, ..)` and the `pts[z]` holder, each a handle
            let mut counts = [0, 0];
            find_context_entity(body, &mut counts);
            total[0] += counts[0];
            total[1] += counts[1];
        }
    }
    assert_eq!(total, [2, 1]);
    assert!(saw_ctx, "the desugar is a context block");
}

/// Counts ToEntity expressions and score reads in a statement tree.
fn find_context_entity(b: &Block, counts: &mut [usize; 2]) {
    fn expr(e: &Expr, counts: &mut [usize; 2]) {
        if matches!(e.kind, ExprKind::ToEntity(_)) {
            counts[0] += 1;
        }
        if matches!(e.kind, ExprKind::Score { .. }) {
            counts[1] += 1;
        }
        match &e.kind {
            ExprKind::ToEntity(inner) => expr(inner, counts),
            ExprKind::Unary { expr: inner, .. } => expr(inner, counts),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, counts);
                expr(rhs, counts);
            }
            ExprKind::Index { base, index } => {
                expr(base, counts);
                expr(index, counts);
            }
            ExprKind::Score { holder, .. } => expr(holder, counts),
            ExprKind::Field { base, .. } => expr(base, counts),
            ExprKind::Call { args, .. } | ExprKind::Builtin { args, .. } => {
                for a in args {
                    expr(a, counts);
                }
            }
            _ => {}
        }
    }
    for s in &b.stmts {
        match s {
            Stmt::Context { clauses, body } => {
                for c in clauses {
                    let (CtxClause::As(e) | CtxClause::At(e)) = c else { continue };
                    expr(e, counts);
                }
                find_context_entity(body, counts);
            }
            Stmt::Assign { target, value, .. } => {
                expr(target, counts);
                expr(value, counts);
            }
            Stmt::If { cond, then, els } => {
                expr(cond, counts);
                find_context_entity(then, counts);
                find_context_entity(els, counts);
            }
            Stmt::ForRange { start, end, body, .. } => {
                expr(start, counts);
                expr(end, counts);
                find_context_entity(body, counts);
            }
            Stmt::ForList { list, body, .. } => {
                expr(list, counts);
                find_context_entity(body, counts);
            }
            Stmt::Block(b) => find_context_entity(b, counts),
            Stmt::Expr(e) => expr(e, counts),
            _ => {}
        }
    }
}

#[test]
fn for_over_a_selector_rejects_loop_jumps_and_assignment() {
    one_error(&in_fn("for z in @a { break; }"), "cannot leave a context block");
    one_error(&in_fn("for z in @a { z = @s; }"), "cannot assign to this expression");
}

#[test]
fn for_over_one_entity_selector_works_and_others_error() {
    ok(&in_fn("for e in @s { tp(e, 0, 70, 0); } for me in @p { tag::add(me, \"seen\"); }"));
    one_error(&in_fn("let boss = summon(\"minecraft:zombie\", 0, 64, 0); for e in boss { tp(e, 0, 70, 0); }"), "cannot iterate over an `Entity`");
    one_error(&in_fn("for x in 1 { print(x); }"), "cannot iterate over `int`");
}

// ------------------------------------------------------------------ f-strings

#[test]
fn f_strings_are_text_for_print() {
    ok("struct P { n: str, hp: int } fn f(p: P, xs: list<int>, ok: bool) { print(f\"{p.n} has {p.hp} hp, {xs.len()} items, {xs}, {ok}, {p.hp * 2 + 1}\"); }");
    // a hole is any storable expression, including calls
    ok("fn g() -> int { return 1; } fn f() { print(f\"{g()}\"); }");
    // an f-string is a string: it can be stored, joined and passed to string arguments
    ok(&in_fn("let n = 1; let t = f\"a{n}\"; say(t); say(f\"a{n}\" + \"!\");"));
    one_error("fn g() {} fn f() { print(f\"{g()}\"); }", "cannot format a value of type `()`");
    one_error(&in_fn("print(f\"{@a}\");"), "cannot format a value of type `selector`");
    one_error(&in_fn("print(f\"{nope}\");"), "cannot find `nope`");
    // the type of a hole is checked like any expression
    one_error(&in_fn("print(f\"{1 + true}\");"), "cannot apply `+`");
    let c = ok(&in_fn("let n = 1; print(f\"n = {n}\");"));
    match &c.hir.fns[0].body.stmts[1] {
        Stmt::Expr(Expr { kind: ExprKind::Builtin { name: "print", args }, .. }) => assert_eq!(args[0].ty, Ty::Str),
        other => panic!("{other:?}"),
    }
}

// ------------------------------------------------------------------- modules

use super::{ModuleInput, check_modules};

/// Checks `(module name, source)` pairs; the first is the root. Returns (errors with file, hir).
fn project(files: &[(&str, &str)]) -> Result<Checked, Vec<(usize, String)>> {
    let programs: Vec<_> = files
        .iter()
        .map(|(_, src)| {
            let src = format!("{PRELUDE}{src}");
            parse(&src).unwrap_or_else(|e| panic!("parse error: {}\n{src}", e.message))
        })
        .collect();
    let inputs: Vec<ModuleInput> = files.iter().zip(&programs).enumerate().map(|(i, ((name, _), program))| ModuleInput { name, file: i, program }).collect();
    check_modules(&inputs).map_err(|r| r.errors.into_iter().map(|d| (d.file, d.message)).collect())
}

fn project_errors(files: &[(&str, &str)]) -> Vec<(usize, String)> {
    match project(files) {
        Ok(_) => panic!("expected errors"),
        Err(e) => e,
    }
}

fn project_one_error(files: &[(&str, &str)], needle: &str) -> usize {
    let e = project_errors(files);
    assert_eq!(e.len(), 1, "expected one error containing {needle:?}, got {e:#?}");
    assert!(e[0].1.contains(needle), "expected {needle:?} in {:?}", e[0].1);
    e[0].0
}

const UTIL: &str = "pub fn clamp(x: int, lo: int, hi: int) -> int { if x < lo { return lo; } if x > hi { return hi; } return x; }
                    pub struct Point { x: int, y: int }
                    pub const LIMIT: int = 10;
                    pub global counter: int = 0;
                    pub objective points = \"dummy\";
                    fn secret() -> int { return 1; }
                    struct Hidden { a: int }
                    const PRIVATE: int = 5;
                    pub const DERIVED: int = PRIVATE * 2;";

#[test]
fn qualified_access_to_pub_items() {
    let c = project(&[
        ("main", "import util; fn main() { let p = util::Point { x: 1, y: util::LIMIT }; let c = util::clamp(p.x, 0, util::DERIVED); util::counter += c; util::points[@s] += 1; print(c); }"),
        ("util", UTIL),
    ])
    .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(c.hir.modules, ["main", "util"]);
    let main = c.hir.fns.iter().find(|f| f.name == "main").unwrap();
    let clamp = c.hir.fns.iter().find(|f| f.name == "clamp").unwrap();
    assert_eq!(main.module, 0);
    assert_eq!(clamp.module, 1);
    // `DERIVED = PRIVATE * 2` is evaluated in util's scope and inlined
    match &main.body.stmts[1] {
        Stmt::Let { init, .. } => match &init.kind {
            ExprKind::Call { args, .. } => assert!(matches!(args[2].kind, ExprKind::Int(10))),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    assert_eq!(c.hir.globals[0].module, 1);
    assert_eq!(c.hir.objectives[0].module, 1);
}

#[test]
fn item_imports_and_aliases() {
    ok_project(&[
        ("main", "import util::{clamp, Point, LIMIT}; fn main() { let p = Point { x: 1, y: LIMIT }; print(clamp(p.x, 0, 5)); }"),
        ("util", UTIL),
    ]);
    ok_project(&[("main", "import util as u; fn main() { print(u::clamp(1, 0, 2)); }"), ("util", UTIL)]);
    ok_project(&[("main", "import maps::arena; fn main() { arena::start(); }"), ("maps::arena", "pub fn start() { print(1); }")]);
    ok_project(&[("main", "import maps::arena as a; fn main() { a::start(); }"), ("maps::arena", "pub fn start() { print(1); }")]);
}

fn ok_project(files: &[(&str, &str)]) -> Checked {
    project(files).unwrap_or_else(|e| panic!("unexpected errors: {e:#?}"))
}

#[test]
fn private_items_are_not_visible_across_modules() {
    let main_file = |body: &str| format!("import util; fn main() {{ {body} }}");
    let one = |body: &str, needle: &str| {
        let src = main_file(body);
        project_one_error(&[("main", src.as_str()), ("util", UTIL)], needle);
    };
    one("print(util::secret());", "`secret` is private to module `util`");
    one("let h = util::Hidden { a: 1 }; print(h.a);", "`Hidden` is private to module `util`");
    one("print(util::PRIVATE);", "`PRIVATE` is private to module `util`");
    one("print(util::nope());", "module `util` has no item `nope`");
    one("let x: util::Nope = 1;", "module `util` has no struct `Nope`");
    // the error is attributed to the importing file
    let src = main_file("print(util::secret());");
    assert_eq!(project_one_error(&[("main", src.as_str()), ("util", UTIL)], "private"), 0);
}

#[test]
fn item_import_errors() {
    project_one_error(&[("main", "import util::{secret}; fn main() {}"), ("util", UTIL)], "`secret` is private");
    project_one_error(&[("main", "import util::{nope}; fn main() {}"), ("util", UTIL)], "has no item `nope`");
    project_one_error(&[("main", "import util::{clamp}; fn clamp() {} fn main() {}"), ("util", UTIL)], "already used in this module");
    project_one_error(&[("main", "import util; import other as util; fn main() {}"), ("util", UTIL), ("other", "pub fn f() {}")], "already used");
    project_one_error(&[("main", "import missing; fn main() {}")], "cannot find module `missing`");
    project_one_error(&[("main", "fn main() { nothing::f(); }")], "cannot find module `nothing`");
    project_one_error(&[("main", "import util; fn main() { util::LIMIT(); }"), ("util", UTIL)], "is not a function");
}

#[test]
fn unqualified_names_do_not_leak_between_modules() {
    // without the import, `clamp` is just unknown
    project_one_error(&[("main", "import util; fn main() { print(clamp(1, 0, 2)); }"), ("util", UTIL)], "cannot find function `clamp`");
    // an item list import exposes only what was listed
    project_one_error(&[("main", "import util::{clamp}; fn main() { let p = Point { x: 1, y: 2 }; }"), ("util", UTIL)], "unknown struct `Point`");
}

#[test]
fn types_flow_across_modules() {
    ok_project(&[
        ("main", "import util; fn shift(p: util::Point) -> util::Point { return util::Point { x: p.x + 1, y: p.y }; } fn main() { let l: list<util::Point> = []; print(l.len()); }"),
        ("util", UTIL),
    ]);
    // a struct from another module is compatible with itself, not with a lookalike
    project_one_error(
        &[
            ("main", "import util; struct Point { x: int, y: int } fn f(p: util::Point) {} fn main() { f(Point { x: 1, y: 2 }); }"),
            ("util", UTIL),
        ],
        "mismatched types",
    );
}

#[test]
fn module_cycles_and_cross_module_recursion() {
    ok_project(&[
        ("main", "import a; fn main() { print(a::even(4)); }"),
        ("a", "import b; pub fn even(n: int) -> int { if n == 0 { return 1; } return b::odd(n - 1); }"),
        ("b", "import a; pub fn odd(n: int) -> int { if n == 0 { return 0; } return a::even(n - 1); }"),
    ]);
}

#[test]
fn errors_are_attributed_to_the_right_file() {
    let e = project_errors(&[
        ("main", "import util; fn main() { let a: int = \"x\"; }"),
        ("util", "pub fn f() -> int { return true; }"),
    ]);
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].0, 0);
    assert_eq!(e[1].0, 1);
}

#[test]
fn constants_are_evaluated_in_their_own_module() {
    // `LIMIT` is a different constant in each module
    let c = ok_project(&[
        ("main", "import util; const LIMIT: int = 3; fn main() { print(LIMIT + util::LIMIT); }"),
        ("util", "pub const LIMIT: int = 10;"),
    ]);
    match &c.hir.fns[0].body.stmts[0] {
        Stmt::Expr(Expr { kind: ExprKind::Builtin { args, .. }, .. }) => match &args[0].kind {
            ExprKind::Binary { lhs, rhs, .. } => assert!(matches!((&lhs.kind, &rhs.kind), (ExprKind::Int(3), ExprKind::Int(10)))),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn entry_points_work_in_any_module() {
    let c = ok_project(&[("main", "import tick; #[load] fn boot() {}"), ("tick", "#[tick] fn step() {} pub fn unused() {}")]);
    let step = c.hir.fns.iter().find(|f| f.name == "step").unwrap();
    assert_eq!(step.entry, Some(EntryKind::Tick));
    assert_eq!(step.module, 1);
}

#[test]
fn same_names_in_different_modules_do_not_clash() {
    ok_project(&[
        ("main", "import a; import b; fn helper() -> int { return 1; } fn main() { print(a::helper() + b::helper() + helper()); }"),
        ("a", "pub fn helper() -> int { return 2; }"),
        ("b", "pub fn helper() -> int { return 3; }"),
    ]);
}

#[test]
fn a_module_can_import_itself_never() {
    let e = project_errors(&[("main", "import util; fn main() {}"), ("util", "import util; pub fn f() {}")]);
    assert!(e[0].1.contains("cannot import itself"), "{e:?}");
}
