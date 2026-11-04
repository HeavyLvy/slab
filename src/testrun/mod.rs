//! `slab test`: runs the `#[test]` functions of a project on a real Minecraft server.
//!
//! The compiler can check a program, but not what the game does with it: an entity that never
//! spawns, a command that fails, an error that only reaches the server log. So the tests run in
//! the game. The project is built with its tests, a throw-away flat world is created in
//! `build/test-server/`, the real server starts, and each test is started in turn after the world
//! and the pack's state have been reset. A test passes when it ends, none of its assertions
//! failed, and the server logged no error while it ran.
//!
//! The test functions are tasks (see `docs/lang/tasks.md`), so they can `wait` for ticks. They
//! report through the log: `[slab-test] FAIL <message>` for a failed assertion and
//! `[slab-test] end <name>` when they are done.
//!
//! Runs are meant to be repeatable. The world has a fixed seed, noon and clear weather, every
//! random sequence starts over before each test, and the game is frozen and moved forward with
//! `tick sprint` while a test waits, so the ticks a test sees do not depend on how fast the
//! computer is. (Randomness inside the game's own mob behaviour is not controlled.) `--hold` and
//! `--serve` keep the server for you, with a console, to look at a failure yourself.

mod commands;
mod fetch;
mod server;

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::time::{Duration, Instant};

use crate::compile::{self, CompileError, Options as BuildOptions, TestCase};
use crate::datapack::ResourceLocation;
use crate::manifest;
use server::Server;

#[derive(Default)]
pub struct Options {
    pub path: PathBuf,
    /// Only the tests whose name contains this.
    pub filter: Option<String>,
    /// Print the tests without running them.
    pub list: bool,
    /// The user agrees to the Minecraft EULA (`--accept-eula`, `SLAB_ACCEPT_EULA=1`, `[test] accept-eula`).
    pub accept_eula: bool,
    pub java: Option<PathBuf>,
    pub jar: Option<PathBuf>,
    /// Seconds one test may take (`--timeout`, `[test] timeout`; default 60).
    pub timeout: Option<u64>,
    /// Show the server's log lines of every test.
    pub verbose: bool,
    /// The world seed (`--seed`, `[test] seed`; default 0).
    pub seed: Option<i64>,
    /// Let game time pass by itself instead of stepping it (`--realtime`).
    pub realtime: bool,
    /// After the tests, keep the server running for you (`--hold`).
    pub hold: bool,
    /// Start the server with the pack and no tests, for you (`--serve`).
    pub serve: bool,
    /// The port of the server (`--port`; with `--hold`/`--serve` the default is 25565 when free).
    pub port: Option<u16>,
}

/// The result of one test.
struct Outcome {
    /// Failed assertions, as the test wrote them.
    failures: Vec<String>,
    /// Errors the server logged (with their stack traces).
    problems: Vec<String>,
    finished: bool,
    log: Vec<String>,
    took: Duration,
}

impl Outcome {
    fn passed(&self) -> bool {
        self.finished && self.failures.is_empty() && self.problems.is_empty()
    }
}

const FAIL_MARK: &str = "[slab-test] FAIL ";
const END_MARK: &str = "[slab-test] end ";

/// What a log line says, without the time and thread (`[12:00:00] [Server thread/INFO]: text`).
fn message(line: &str) -> &str {
    match line.find("]: ") {
        Some(i) if line.starts_with('[') => &line[i + 3..],
        _ => line,
    }
}

/// Whether a log line is the game complaining: an error, or one of the messages it uses for
/// commands and data it could not run or read.
fn is_problem(line: &str) -> bool {
    if !line.starts_with('[') {
        return false;
    }
    let text = message(line);
    line.contains("/ERROR]")
        || text.starts_with("Failed to")
        || text.starts_with("Couldn't")
        || text.contains("Exception")
        || text.contains("Whilst parsing")
        || text.contains("Command execution stopped")
        || text.contains("Unknown or incomplete command")
}

/// Sorts the log of one test into failed assertions and server problems. A problem includes the
/// indented lines of its stack trace.
fn analyse(lines: &[String], name: &str) -> (Vec<String>, Vec<String>, bool) {
    let (mut failures, mut problems, mut finished) = (Vec::new(), Vec::new(), false);
    let mut in_problem = false;
    for line in lines {
        let text = message(line);
        if let Some(rest) = text.split(FAIL_MARK).nth(1).filter(|_| text.contains(FAIL_MARK)) {
            failures.push(rest.trim().to_owned());
            in_problem = false;
        } else if text.contains(&format!("{END_MARK}{name}")) {
            finished = true;
            in_problem = false;
        } else if is_problem(line) {
            problems.push(text.to_owned());
            in_problem = true;
        } else if in_problem && !line.starts_with('[') {
            // a stack trace: keep the first few lines
            if problems.last().is_some_and(|p| p.lines().count() < 6) {
                let last = problems.len() - 1;
                problems[last].push('\n');
                problems[last].push_str(line.trim_end());
            }
        } else {
            in_problem = false;
        }
    }
    (failures, problems, finished)
}

/// Builds the project's tests and runs them. `Ok(true)` when every test passed.
pub fn run(opts: &Options) -> Result<bool, String> {
    let compiled = compile::compile(&opts.path, &BuildOptions { tests: true, ..BuildOptions::default() }).map_err(|e| render_error(&e))?;
    for w in &compiled.warnings {
        eprintln!("{}", compiled.sources.render(w));
    }
    let manifest = compiled.project_dir.join(manifest::FILE).is_file().then(|| manifest::load(&compiled.project_dir)).transpose()?;
    let all: Vec<&TestCase> = compiled.tests.iter().filter(|t| opts.filter.as_ref().is_none_or(|f| t.name.contains(f.as_str()))).collect();
    let tests: Vec<&TestCase> = if opts.serve { Vec::new() } else { all.clone() };
    if opts.list {
        for t in &all {
            println!("{}  ({})", t.name, short(&compiled.project_dir, &t.at));
        }
        return Ok(true);
    }
    let holding = opts.hold || opts.serve;
    if tests.is_empty() && !holding {
        println!("no tests found{}", if opts.filter.is_some() { " matching the filter" } else { " (mark a function with #[test])" });
        return Ok(true);
    }

    let accepted = opts.accept_eula || manifest.as_ref().and_then(|m| m.accept_eula).unwrap_or(false) || std::env::var_os("SLAB_ACCEPT_EULA").is_some_and(|v| v != "0" && !v.is_empty());
    let timeout = Duration::from_secs(opts.timeout.or_else(|| manifest.as_ref().and_then(|m| m.test_timeout).map(|t| t as u64)).unwrap_or(60));
    let seed = opts.seed.or_else(|| manifest.as_ref().and_then(|m| m.test_seed)).unwrap_or(0);
    // checked first: it is the more basic requirement, and needs no Java
    let jar = fetch::server_jar(compiled.target.name, opts.jar.as_deref(), accepted)?;
    if !accepted && !jar.starts_with(fetch::cache_root()) {
        return Err(format!("running a Minecraft server means accepting the Minecraft EULA ({}): pass `--accept-eula` to agree", fetch::EULA_URL));
    }
    // the server runs in its own folder, so the jar's path must not depend on where we are
    let jar = std::fs::canonicalize(&jar).unwrap_or(jar);
    let java = fetch::find_java(opts.java.as_deref())?;

    let port = pick_port(opts.port, holding)?;
    let dir = compiled.project_dir.join("build").join("test-server");
    prepare_server_dir(&dir, &compiled, accepted, seed, port, holding)?;
    let mode = if opts.realtime { "real time" } else { "stepped time" };
    println!(
        "running {} test{} of `{}` on Minecraft {} (seed {seed}, {mode}; server log: {})",
        tests.len(),
        if tests.len() == 1 { "" } else { "s" },
        compiled.namespace,
        compiled.target.name,
        dir.join("logs").join("latest.log").display()
    );

    let mut server = Server::start(&java, &jar, &dir)?;
    let (startup, up) = server.read_until(Duration::from_secs(180), |l| message(l).starts_with("Done ("));
    if !up {
        return Err(format!("the server did not start:\n{}", startup.iter().rev().take(15).rev().cloned().collect::<Vec<_>>().join("\n")));
    }
    let broken: Vec<&String> = startup.iter().filter(|l| is_problem(l)).collect();
    if !broken.is_empty() {
        server.stop();
        let list: Vec<String> = broken.iter().map(|l| format!("  {}", message(l))).collect();
        return Err(format!("the server logged errors while loading the pack (so no test was run):\n{}", list.join("\n")));
    }

    for c in commands::setup() {
        server.send(&c);
    }
    // forced chunks load a little after the command (and not while time stands still): wait until
    // the middle of the arena is there
    let ready = Instant::now() + Duration::from_secs(60);
    loop {
        server.send(&commands::ready_probe("[slab-test-ready]"));
        let (_, found) = server.read_until(Duration::from_secs(2), |l| l.contains("[slab-test-ready]"));
        if found {
            break;
        }
        if Instant::now() > ready || !server.alive() {
            server.stop();
            return Err("the chunks around the origin did not load".to_owned());
        }
    }
    server.sync(Duration::from_secs(10));
    let stepped = !opts.realtime;
    if stepped {
        server.send(&commands::freeze());
        server.sync(Duration::from_secs(10));
    }

    let reset = ResourceLocation::new(&compiled.namespace, "_test_reset").map_err(|e| e.to_string())?;
    let mut failed = Vec::new();
    for t in &tests {
        let outcome = run_test(&mut server, t, &reset, timeout, stepped);
        report(t, &short(&compiled.project_dir, &t.at), &outcome, opts.verbose);
        if !outcome.passed() {
            failed.push(t.name.clone());
        }
        if !server.alive() {
            return Err("the server stopped while the tests were running; see its log".to_owned());
        }
    }

    if !tests.is_empty() {
        let passed = tests.len() - failed.len();
        if failed.is_empty() {
            println!("\ntest result: ok. {passed} passed");
        } else {
            println!("\nfailures:\n    {}\n\ntest result: FAILED. {passed} passed; {} failed", failed.join("\n    "), failed.len());
            let first = &failed[0];
            println!("\nto look at it yourself: slab test --filter {first} --hold{}   (same seed: --seed {seed})", if opts.realtime { " --realtime" } else { "" });
        }
    }
    if holding {
        interact(&mut server, &all, &reset, port, stepped);
    }
    server.stop();
    Ok(failed.is_empty())
}

/// A free port for the server; the usual 25565 when someone is going to connect.
fn pick_port(wanted: Option<u16>, holding: bool) -> Result<u16, String> {
    let free = |p: u16| std::net::TcpListener::bind(("127.0.0.1", p)).is_ok();
    if let Some(p) = wanted {
        return if free(p) { Ok(p) } else { Err(format!("port {p} is already in use")) };
    }
    if holding && free(25565) {
        return Ok(25565);
    }
    std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).map_err(|e| format!("no free port: {e}"))
}

fn render_error(e: &CompileError) -> String {
    e.render().trim_end().to_owned()
}

/// The server's folder: a flat world with the pack in it. Everything from a previous run goes.
fn prepare_server_dir(dir: &Path, compiled: &compile::Compiled, eula: bool, seed: i64, port: u16, for_player: bool) -> Result<(), String> {
    let io = |e: std::io::Error| format!("cannot prepare {}: {e}", dir.display());
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(io)?;
    }
    std::fs::create_dir_all(dir).map_err(io)?;
    std::fs::write(dir.join("eula.txt"), format!("eula={eula}\n")).map_err(io)?;
    // bedrock, two dirt and one grass: the surface is at `commands::GROUND`
    const FLAT: &str = r#"{"biome":"minecraft:plains","features":false,"lakes":false,"layers":[{"block":"minecraft:bedrock","height":1},{"block":"minecraft:dirt","height":2},{"block":"minecraft:grass_block","height":1}],"structure_overrides":[]}"#;
    let mut props = format!(
        "online-mode=false\nlevel-type=minecraft:flat\ngenerator-settings={FLAT}\nlevel-seed={seed}\ndifficulty=normal\ngamemode=survival\ngenerate-structures=false\n\
         view-distance=4\nsimulation-distance=4\nspawn-protection=0\nfunction-permission-level=2\nmax-tick-time=-1\nserver-ip=127.0.0.1\nserver-port={port}\n\
         spawn-monsters=false\nspawn-animals=false\nspawn-npcs=false\nsync-chunk-writes=false\nenable-jmx-monitoring=false\nmax-players=4\n\
         pause-when-empty-seconds=-1\nenable-status={for_player}\n"
    );
    if for_player {
        // someone is going to walk around in it
        props.push_str("gamemode=creative\nforce-gamemode=true\nallow-flight=true\n");
    }
    std::fs::write(dir.join("server.properties"), props).map_err(io)?;
    let pack_dir = dir.join("world").join("datapacks").join(&compiled.namespace);
    std::fs::create_dir_all(&pack_dir).map_err(io)?;
    compiled.pack.write_dir(&pack_dir, false).map_err(|e| format!("cannot write the test pack: {e}"))?;
    Ok(())
}

/// The same start for every test: the pack as it is after loading, the same sky, the same random
/// numbers, an empty arena, and what the test asked for.
fn start_state(server: &mut Server, test: &TestCase, reset: &ResourceLocation) {
    server.send(&commands::call(reset));
    for c in commands::fix_world().into_iter().chain(commands::clean_arena()) {
        server.send(&c);
    }
    if test.strict_commands
        && let Ok(on) = ResourceLocation::new(reset.namespace(), "_test_strict_on")
    {
        server.send(&commands::call(&on));
    }
    if let Some(n) = test.max_commands {
        server.send(&commands::command_limit(n));
    }
}

fn run_test(server: &mut Server, test: &TestCase, reset: &ResourceLocation, timeout: Duration, stepped: bool) -> Outcome {
    let started = Instant::now();
    let mut log = Vec::new();
    // the same start every time: the pack as it is after loading, the same sky, the same
    // random numbers, an empty arena
    start_state(server, test, reset);
    log.extend(server.sync(Duration::from_secs(20)));

    server.send(&commands::call(&test.function));
    let end = format!("{END_MARK}{}", test.function.path());
    // a test the game cut off (too many commands in a tick) can never end: stop waiting for it
    let over = |l: &str| l.contains(&end) || message(l).contains("Command execution stopped");
    let ended = if stepped {
        // game time only moves when we say so: a few ticks at a time, as fast as the server can
        let mut ticks = 0u64;
        let limit = timeout.as_secs() * 20;
        loop {
            server.send(&commands::sprint(10));
            let (lines, done) = server.read_until(Duration::from_secs(30), |l| l.contains("Sprint completed"));
            let stop = lines.iter().any(|l| over(l));
            log.extend(lines);
            ticks += 10;
            if stop || !done || ticks >= limit {
                break;
            }
        }
        log.iter().any(|l| l.contains(&end))
    } else {
        let (lines, found) = server.read_until(timeout, over);
        let ended = found && lines.last().is_some_and(|l| l.contains(&end));
        log.extend(lines);
        ended
    };
    // whatever the server printed right after the end belongs to the test too
    log.extend(server.sync(Duration::from_secs(10)));

    if test.max_commands.is_some() {
        server.send(&commands::command_limit(commands::DEFAULT_COMMAND_LIMIT));
    }
    // a test that did not finish must not go on in the next one
    if let Ok(step) = ResourceLocation::new(test.function.namespace(), &format!("{}__step", test.function.path())) {
        server.send(&commands::clear_schedule(&step));
    }
    let (mut failures, problems, _) = analyse(&log, test.function.path());
    failures.dedup();
    Outcome { failures, problems, finished: ended, log, took: started.elapsed() }
}

const HELP: &str = "\
  anything         is sent to the server console (`time query gametime`, `summon ...`)
  !tests           list the tests
  !run NAME        reset the world and run the test whose name contains NAME
  !reset           reset the world and the pack, as before every test
  !step N          let N ticks pass (the game stands still otherwise, unless started with --realtime)
  !freeze          stop time          !unfreeze   let time run
  !help            this text          !quit / stop   stop the server";

/// Keeps the server for you: its log is shown, and what you type goes to its console. Lines that
/// start with `!` are slab's own commands (`!help`).
fn interact(server: &mut Server, tests: &[&TestCase], reset: &ResourceLocation, port: u16, stepped: bool) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    if stepped {
        // somebody is looking: let time run
        server.send(&commands::unfreeze());
    }
    println!("\nThe server is running: join it from Minecraft with the address `localhost:{port}` (any name; you start in creative mode).");
    println!("Type commands here. `!help` lists slab's. `!quit` (or Ctrl-D) stops the server.");
    loop {
        while let Some(line) = server.next_line(Duration::from_millis(50)) {
            println!("{line}");
        }
        match rx.try_recv() {
            Ok(line) => {
                let line = line.trim();
                let (word, arg) = line.split_once(' ').map_or((line, ""), |(w, a)| (w, a.trim()));
                match word {
                    "" => {}
                    "!quit" | "stop" => break,
                    "!help" => println!("{HELP}"),
                    "!tests" => tests.iter().for_each(|t| println!("  {}", t.name)),
                    "!reset" => {
                        server.send(&commands::call(reset));
                        for c in commands::fix_world().into_iter().chain(commands::clean_arena()) {
                            server.send(&c);
                        }
                    }
                    "!run" => match tests.iter().filter(|t| t.name.contains(arg)).collect::<Vec<_>>().as_slice() {
                        [t] => {
                            start_state(server, t, reset);
                            server.send(&commands::call(&t.function));
                        }
                        [] => println!("no test matches `{arg}` (`!tests` lists them)"),
                        many => println!("`{arg}` matches {} tests; be more exact", many.len()),
                    },
                    "!step" => match arg.parse::<u32>() {
                        Ok(n) if n >= 1 => server.send(&commands::sprint(n)),
                        _ => println!("!step needs a number of ticks"),
                    },
                    "!freeze" => server.send(&commands::freeze()),
                    "!unfreeze" => server.send(&commands::unfreeze()),
                    _ => server.send(line),
                }
            }
            Err(TryRecvError::Empty) => {}
            // the input ended (Ctrl-D, or no terminal): nothing more can be asked of the server
            Err(TryRecvError::Disconnected) => break,
        }
        if !server.alive() {
            break;
        }
    }
}

/// `file:line` relative to the project folder.
fn short(project: &Path, at: &str) -> String {
    let root = project.to_string_lossy();
    at.strip_prefix(root.as_ref()).map_or_else(|| at.to_owned(), |rest| rest.trim_start_matches(['/', '\\']).to_owned())
}

fn report(test: &TestCase, at: &str, o: &Outcome, verbose: bool) {
    let budget = test.max_commands;
    let secs = format!("{:.1}s", o.took.as_secs_f32());
    if o.passed() {
        println!("  ok      {}  ({secs})", test.name);
    } else {
        println!("  FAILED  {}  ({secs})  at {at}", test.name);
        if !o.finished {
            println!("          did not finish: it never reached its end (a wait too long, or the server stopped it)");
        }
        for f in &o.failures {
            match f.strip_prefix("a command failed at ") {
                Some(rest) => println!("          command failed: {rest}"),
                None => println!("          assertion failed: {f}"),
            }
        }
        for p in &o.problems {
            let mut lines = p.lines();
            let first = lines.next().unwrap_or("");
            println!("          server logged: {first}");
            if let (true, Some(n)) = (first.contains("Command execution stopped"), budget) {
                println!("                         (this test allows {n} commands per tick: `#[test(max_commands({n}))]`)");
            }
            for l in lines {
                println!("                         {l}");
            }
        }
    }
    if verbose || !o.passed() && o.failures.is_empty() && o.problems.is_empty() {
        for l in &o.log {
            println!("          | {l}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn a_log_is_sorted_into_failures_problems_and_the_end() {
        let lines = log(&[
            "[12:00:00] [Server thread/INFO]: [Server] [slab-test] FAIL ring has 3 markers: expected 4, got 3",
            "[12:00:01] [Server thread/ERROR]: Failed to run function t:step",
            "\tat net.minecraft.Foo.bar(Foo.java:1)",
            "\tat net.minecraft.Foo.baz(Foo.java:2)",
            "[12:00:01] [Server thread/INFO]: Killed Pig",
            "[12:00:02] [Server thread/INFO]: [Server] [slab-test] end ring_closes",
        ]);
        let (failures, problems, finished) = analyse(&lines, "ring_closes");
        assert_eq!(failures, ["ring has 3 markers: expected 4, got 3"]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("Failed to run function t:step\n\tat net.minecraft.Foo.bar"), "{problems:?}");
        assert!(finished);
        // another test's end does not count
        assert!(!analyse(&lines, "other").2);
    }

    #[test]
    fn what_counts_as_a_problem() {
        for bad in ["[12:00:00] [Server thread/ERROR]: oops", "[12:00:00] [Server thread/INFO]: Failed to load function a:b", "[12:00:00] [main/WARN]: java.lang.NullPointerException", "[12:00:00] [Server thread/INFO]: Command execution stopped due to limit (executed 65536 commands)"] {
            assert!(is_problem(bad), "{bad}");
        }
        for fine in ["[12:00:00] [Server thread/INFO]: Killed Pig", "[12:00:00] [Server thread/WARN]: Can't keep up! Is the server overloaded?", "[12:00:00] [Server thread/INFO]: [Server] [slab-test] FAIL x", "\tat Foo.bar(Foo.java:1)"] {
            assert!(!is_problem(fine), "{fine}");
        }
    }
}
