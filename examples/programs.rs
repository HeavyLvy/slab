//! IR programs whose *results* are checked on a real server:
//!   cargo run --example programs -- target/programs
//!   tools/verify_pack.py target/programs --run prog:run_all --checks target/programs.checks.json

use std::path::PathBuf;

use slab::datapack::Nbt;
use slab::ir::*;
use slab::lower::lower;

fn int(n: i32) -> Expr {
    Expr::Int(n)
}
fn v(name: &str) -> Expr {
    Expr::var(name)
}
fn g(name: &str) -> Expr {
    Expr::global(name)
}
fn add(a: Expr, b: Expr) -> Expr {
    Expr::bin(BinOp::Add, a, b)
}
fn sub(a: Expr, b: Expr) -> Expr {
    Expr::bin(BinOp::Sub, a, b)
}
fn mul(a: Expr, b: Expr) -> Expr {
    Expr::bin(BinOp::Mul, a, b)
}
fn cmp(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::bin(op, a, b)
}
fn call(f: &str, args: Vec<Expr>) -> Expr {
    Expr::call(f, args)
}
fn let_(name: &str, e: Expr) -> Stmt {
    Stmt::Let { name: name.into(), kind: Kind::Int, init: Some(e) }
}
fn set(name: &str, e: Expr) -> Stmt {
    Stmt::Assign { target: VarRef::Local(name.into()), value: e }
}
fn setg(name: &str, e: Expr) -> Stmt {
    Stmt::Assign { target: VarRef::Global(name.into()), value: e }
}
fn ret(e: Expr) -> Stmt {
    Stmt::Return(Some(e))
}
fn if_(c: Expr, then: Vec<Stmt>, els: Vec<Stmt>) -> Stmt {
    Stmt::If { cond: c, then, els }
}
fn while_(c: Expr, body: Vec<Stmt>) -> Stmt {
    Stmt::While { cond: c, body }
}

fn program() -> Program {
    let mut p = Program::new("prog");
    for name in [
        "fib_result", "fact_result", "sum_result", "early_result", "else_x", "else_y", "fdiv", "fmod", "fdiv2", "logic1", "logic2", "tri_result",
        "even_result", "gcd_result", "logic3", "loop_huge", "loaded", "count", "cfg_a", "loop_big", "neg_result", "cmp_edge", "const_fold",
    ] {
        p = p.global(name, Kind::Int, int(0));
    }
    p = p.global("greeting", Kind::Data, Expr::Data(Nbt::str("unset")));
    p = p.global("names", Kind::Data, Expr::Data(Nbt::List(vec![])));
    p = p.global("cfg", Kind::Data, Expr::Data(Nbt::Compound(slab::datapack::Compound::new().with("a", 1).with("b", "x"))));

    // recursion: fib(n-1) + fib(n-2) needs the first result to survive the second call
    p = p.function(
        slab::ir::Function::new("fib")
            .param("n", Kind::Int)
            .returns(Kind::Int)
            .body(vec![
                if_(cmp(BinOp::Lt, v("n"), int(2)), vec![ret(v("n"))], vec![]),
                ret(add(call("fib", vec![sub(v("n"), int(1))]), call("fib", vec![sub(v("n"), int(2))]))),
            ]),
    );

    // iterative factorial: while loop
    p = p.function(
        slab::ir::Function::new("fact")
            .param("n", Kind::Int)
            .returns(Kind::Int)
            .body(vec![
                let_("acc", int(1)),
                while_(cmp(BinOp::Gt, v("n"), int(1)), vec![set("acc", mul(v("acc"), v("n"))), set("n", sub(v("n"), int(1)))]),
                ret(v("acc")),
            ]),
    );

    // sum 1..=n with a loop of 100 iterations
    p = p.function(
        slab::ir::Function::new("sum_to")
            .param("n", Kind::Int)
            .returns(Kind::Int)
            .body(vec![
                let_("i", int(1)),
                let_("s", int(0)),
                while_(cmp(BinOp::Le, v("i"), v("n")), vec![set("s", add(v("s"), v("i"))), set("i", add(v("i"), int(1)))]),
                ret(v("s")),
            ]),
    );

    // return from inside a loop inside an if: the signal has to climb out of both blocks
    p = p.function(
        slab::ir::Function::new("first_square_over")
            .param("limit", Kind::Int)
            .returns(Kind::Int)
            .body(vec![
                let_("i", int(0)),
                while_(
                    cmp(BinOp::Lt, v("i"), int(1000)),
                    vec![
                        if_(cmp(BinOp::Gt, mul(v("i"), v("i")), v("limit")), vec![ret(v("i"))], vec![]),
                        set("i", add(v("i"), int(1))),
                    ],
                ),
                ret(int(-1)),
            ]),
    );

    // locals must survive a recursive call: a is read after tri(n-1) returns
    p = p.function(
        slab::ir::Function::new("tri")
            .param("n", Kind::Int)
            .returns(Kind::Int)
            .body(vec![
                if_(cmp(BinOp::Eq, v("n"), int(0)), vec![ret(int(0))], vec![]),
                let_("a", v("n")),
                let_("r", call("tri", vec![sub(v("n"), int(1))])),
                ret(add(v("a"), v("r"))),
            ]),
    );

    // mutual recursion
    p = p.function(
        slab::ir::Function::new("is_even").param("n", Kind::Int).returns(Kind::Int).body(vec![
            if_(cmp(BinOp::Eq, v("n"), int(0)), vec![ret(int(1))], vec![]),
            ret(call("is_odd", vec![sub(v("n"), int(1))])),
        ]),
    );
    p = p.function(
        slab::ir::Function::new("is_odd").param("n", Kind::Int).returns(Kind::Int).body(vec![
            if_(cmp(BinOp::Eq, v("n"), int(0)), vec![ret(int(0))], vec![]),
            ret(call("is_even", vec![sub(v("n"), int(1))])),
        ]),
    );

    // gcd(b, a % b): the arguments swap, so passing them one at a time would clobber a
    p = p.function(
        slab::ir::Function::new("gcd").param("a", Kind::Int).param("b", Kind::Int).returns(Kind::Int).body(vec![
            if_(cmp(BinOp::Eq, v("b"), int(0)), vec![ret(v("a"))], vec![]),
            ret(call("gcd", vec![v("b"), Expr::bin(BinOp::Mod, v("a"), v("b"))])),
        ]),
    );

    // else branch must not re-test a condition the then-branch changed
    p = p.function(slab::ir::Function::new("else_semantics").body(vec![
        let_("x", int(1)),
        let_("y", int(0)),
        if_(cmp(BinOp::Lt, v("x"), int(3)), vec![set("x", int(5))], vec![set("y", int(99))]),
        setg("else_x", v("x")),
        setg("else_y", v("y")),
    ]));

    // floored division and modulo at runtime (not constant folded: operands are variables)
    p = p.function(slab::ir::Function::new("floor_ops").body(vec![
        let_("a", int(-7)),
        let_("b", int(2)),
        setg("fdiv", Expr::bin(BinOp::Div, v("a"), v("b"))),
        setg("fmod", Expr::bin(BinOp::Mod, v("a"), v("b"))),
        let_("c", int(7)),
        let_("d", int(-2)),
        setg("fdiv2", Expr::bin(BinOp::Div, v("c"), v("d"))),
    ]));

    // boolean logic, including negation of conjunctions/disjunctions
    p = p.function(slab::ir::Function::new("logic").body(vec![
        let_("a", int(3)),
        let_("b", int(7)),
        // (a<5 && b>5) || a==100  -> 1
        Stmt::Let {
            name: "r1".into(),
            kind: Kind::Bool,
            init: Some(Expr::bin(
                BinOp::Or,
                Expr::bin(BinOp::And, cmp(BinOp::Lt, v("a"), int(5)), cmp(BinOp::Gt, v("b"), int(5))),
                cmp(BinOp::Eq, v("a"), int(100)),
            )),
        },
        // !(a<5 && b>5) -> 0
        Stmt::Let {
            name: "r2".into(),
            kind: Kind::Bool,
            init: Some(Expr::logical_not(Expr::bin(BinOp::And, cmp(BinOp::Lt, v("a"), int(5)), cmp(BinOp::Gt, v("b"), int(5))))),
        },
        setg("logic1", v("r1")),
        setg("logic2", v("r2")),
        // used as an if condition: a != 3 || b != 7 is false -> neither branch sets 1
        if_(Expr::bin(BinOp::Or, cmp(BinOp::Ne, v("a"), int(3)), cmp(BinOp::Ne, v("b"), int(7))), vec![setg("logic3", int(111))], vec![setg("logic3", int(222))]),
    ]));

    // arithmetic helpers: negation, constant folding, edge comparisons
    p = p.function(slab::ir::Function::new("arith").body(vec![
        let_("x", int(5)),
        setg("neg_result", Expr::negate(add(v("x"), int(1)))),
        setg("const_fold", add(mul(int(6), int(7)), Expr::bin(BinOp::Div, int(-7), int(2)))),
        // x > i32::MAX is never true; x < i32::MIN never true; ranges can't express them
        Stmt::Let {
            name: "e1".into(),
            kind: Kind::Bool,
            init: Some(cmp(BinOp::Gt, v("x"), int(i32::MAX))),
        },
        Stmt::Let {
            name: "e2".into(),
            kind: Kind::Bool,
            init: Some(cmp(BinOp::Ge, v("x"), int(i32::MIN))),
        },
        setg("cmp_edge", add(mul(v("e1"), int(10)), v("e2"))),
    ]));

    // storage-backed data: strings, lists, compounds, conversions
    p = p.function(
        slab::ir::Function::new("greet")
            .param("s", Kind::Data)
            .returns(Kind::Data)
            .body(vec![ret(v("s"))]),
    );
    p = p.function(slab::ir::Function::new("data_ops").body(vec![
        Stmt::Assign { target: VarRef::Global("greeting".into()), value: call("greet", vec![Expr::Data(Nbt::str("hello"))]) },
        Stmt::Append { target: VarRef::Global("names".into()), path: slab::datapack::NbtPath::new(), value: Expr::Data(Nbt::str("ann")) },
        Stmt::Append { target: VarRef::Global("names".into()), path: slab::datapack::NbtPath::new(), value: Expr::Data(Nbt::str("bob")) },
        Stmt::Append { target: VarRef::Global("names".into()), path: slab::datapack::NbtPath::new(), value: Expr::ToData(Box::new(add(int(40), int(2)))) },
        setg("count", Expr::Len(VarRef::Global("names".into()), slab::datapack::NbtPath::new())),
        // cfg.a = cfg.a + 41
        Stmt::SetField {
            target: VarRef::Global("cfg".into()),
            path: slab::datapack::NbtPath::new().key("a"),
            value: Expr::ToData(Box::new(add(Expr::ToInt(Box::new(Expr::Field(VarRef::Global("cfg".into()), slab::datapack::NbtPath::new().key("a")))), int(41)))),
        },
        setg("cfg_a", Expr::ToInt(Box::new(Expr::Field(VarRef::Global("cfg".into()), slab::datapack::NbtPath::new().key("a"))))),
    ]));

    // a long loop to see how far one tick gets
    p = p.function(slab::ir::Function::new("big_loop").body(vec![
        let_("i", int(0)),
        while_(cmp(BinOp::Lt, v("i"), int(5000)), vec![set("i", add(v("i"), int(1)))]),
        setg("loop_big", v("i")),
    ]));

    // not part of run_all: documents what happens when a loop exceeds the per-tick command limit
    p = p.function(slab::ir::Function::new("huge_loop").body(vec![
        let_("i", int(0)),
        while_(cmp(BinOp::Lt, v("i"), int(100000)), vec![set("i", add(v("i"), int(1)))]),
        setg("loop_huge", v("i")),
        Stmt::Print(int(1)),
    ]));

    // load entry
    p = p.function(slab::ir::Function::new("on_load").entry(Entry::Load).body(vec![setg("loaded", int(1))]));

    p = p.function(slab::ir::Function::new("run_all").body(vec![
        setg("fib_result", call("fib", vec![int(10)])),
        setg("fact_result", call("fact", vec![int(10)])),
        setg("sum_result", call("sum_to", vec![int(100)])),
        setg("early_result", call("first_square_over", vec![int(50)])),
        setg("tri_result", call("tri", vec![int(10)])),
        setg("even_result", add(mul(call("is_even", vec![int(10)]), int(10)), call("is_even", vec![int(7)]))),
        setg("gcd_result", call("gcd", vec![int(48), int(18)])),
        Stmt::Expr(call("else_semantics", vec![])),
        Stmt::Expr(call("floor_ops", vec![])),
        Stmt::Expr(call("logic", vec![])),
        Stmt::Expr(call("arith", vec![])),
        Stmt::Expr(call("data_ops", vec![])),
        Stmt::Expr(call("big_loop", vec![])),
        Stmt::Print(g("fib_result")),
        Stmt::Print(Expr::Global("greeting".into())),
    ]));
    p
}

fn main() {
    let out: PathBuf = std::env::args().nth(1).unwrap_or_else(|| "target/programs".into()).into();
    let pack = match lower(&program()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("lowering failed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = pack.write_dir(&out, true) {
        eprintln!("{e}");
        std::process::exit(1);
    }

    let score = |name: &str, want: &str| (format!("scoreboard players get #g.{name} prog"), format!("has {want} [prog]"));
    let data = |name: &str, want: &str| (format!("data get storage prog:globals {name}"), want.to_owned());
    let checks = vec![
        score("loaded", "1"),
        score("fib_result", "55"),
        score("fact_result", "3628800"),
        score("sum_result", "5050"),
        score("early_result", "8"),
        score("tri_result", "55"),
        score("even_result", "10"),
        score("gcd_result", "6"),
        score("else_x", "5"),
        score("else_y", "0"),
        score("fdiv", "-4"),
        score("fmod", "1"),
        score("fdiv2", "-4"),
        score("logic1", "1"),
        score("logic2", "0"),
        score("logic3", "222"),
        score("neg_result", "-6"),
        score("const_fold", "38"),
        score("cmp_edge", "1"),
        score("count", "3"),
        score("cfg_a", "42"),
        score("loop_big", "5000"),
        data("greeting", "\"hello\""),
        data("names", "[\"ann\", \"bob\", 42]"),
    ];
    let json: Vec<String> = checks
        .iter()
        .map(|(cmd, expect)| format!("  {{\"cmd\": {cmd:?}, \"expect\": {expect:?}}}"))
        .collect();
    let path = format!("{}.checks.json", out.display());
    std::fs::write(&path, format!("[\n{}\n]\n", json.join(",\n"))).unwrap();
    println!("wrote {} and {path}", out.display());
}
