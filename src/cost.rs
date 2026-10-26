//! A static estimate of how many commands a function runs in one go.
//!
//! The game stops a function chain after 65,536 commands (`max_command_sequence_length`), without
//! a message. This reads the finished data pack: every line of a function counts one, and a line
//! that calls another function adds that function's cost. A loop is a function that calls itself,
//! so its cost is the cost of one trip times how often it runs, when the front end knew that
//! (`for i in 0..360`; see `ir::LoopInfo`); a loop of unknown length counts once.
//!
//! Which branch runs is not known, so there are two numbers: `low` leaves out every call that
//! sits under an `if`/`unless` (what always runs: a warning built on it is never wrong, but it
//! can miss code whose work is all in branches), and `high` counts them all (an upper bound,
//! too high wherever the branches exclude each other).

use std::collections::{HashMap, HashSet};

use crate::datapack::{DataPack, ResourceLocation};
use crate::diag::Span;
use crate::ir::LoopInfo;

/// Commands one function chain may run per tick (`max_command_sequence_length`'s default).
pub const LIMIT: u64 = 65_536;

#[derive(Clone, Debug, PartialEq)]
pub struct Estimate {
    /// Commands that run on every path (a lower bound, unless a loop of unknown length runs more than once).
    pub low: u64,
    /// Commands if every branch ran.
    pub high: u64,
    /// The loop that adds the most to `low` among those whose length is known.
    pub heaviest: Option<Heaviest>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Heaviest {
    pub trips: u64,
    pub per_trip: u64,
    /// `per_trip * trips`, always-run commands only.
    pub total: u64,
    pub origin: Option<(usize, Span)>,
}

struct Estimator<'a> {
    bodies: HashMap<String, (u64, Vec<Call>)>,
    loops: HashMap<String, &'a LoopInfo>,
    memo: HashMap<String, (u64, u64)>,
    stack: HashSet<String>,
    heaviest: Option<Heaviest>,
}

/// A call to a function, and whether it only happens when some condition holds.
#[derive(Clone)]
struct Call {
    id: String,
    conditional: bool,
}

/// The functions a line calls: `function ns:x`, `run function ns:x with ...`, `if function ns:x`.
/// `schedule function` runs in a later tick, so it is not part of this chain. A call is
/// conditional when an `if`/`unless` other than the call itself comes before it.
fn calls_in(line: &str) -> Vec<Call> {
    let mut out = Vec::new();
    let mut rest = line;
    let mut consumed = 0;
    while let Some(i) = rest.find("function ") {
        let before = &line[..consumed + i];
        let after = &rest[i + "function ".len()..];
        let id: String = after.chars().take_while(|c| !c.is_whitespace()).collect();
        let scheduled = before.trim_end().ends_with("schedule");
        let boundary = before.is_empty() || before.ends_with(' ');
        // text of a message (`say`, or inside quotes) is not a call
        let message = line.starts_with("say ") || before.matches('"').count() % 2 == 1;
        if boundary && !scheduled && !message && id.contains(':') {
            // `... if function x`: the `if` that tests the call does not make it conditional
            let own = before.trim_end().strip_suffix("unless").or_else(|| before.trim_end().strip_suffix("if")).unwrap_or(before);
            let conditional = own.split_whitespace().any(|w| w == "if" || w == "unless");
            out.push(Call { id, conditional });
        }
        consumed += i + "function ".len();
        rest = after;
    }
    out
}

impl Estimator<'_> {
    /// `(low, high)` for one function, loops included.
    fn cost(&mut self, id: &str) -> (u64, u64) {
        if let Some(c) = self.memo.get(id) {
            return *c;
        }
        let Some((lines, calls)) = self.bodies.get(id).cloned() else { return (0, 0) };
        self.stack.insert(id.to_owned());
        let (mut low, mut high) = (lines, lines);
        for call in &calls {
            if call.id == id || self.stack.contains(&call.id) {
                continue;
            }
            let (l, h) = self.cost(&call.id);
            if !call.conditional {
                low = low.saturating_add(l);
            }
            high = high.saturating_add(h);
        }
        self.stack.remove(id);
        let result = match self.loops.get(id).and_then(|l| l.trips) {
            Some(n) => {
                let total = low.saturating_mul(n);
                if self.heaviest.as_ref().is_none_or(|h| total > h.total) {
                    self.heaviest = Some(Heaviest { trips: n, per_trip: low, total, origin: self.loops[id].origin });
                }
                (total, high.saturating_mul(n))
            }
            None => (low, high),
        };
        self.memo.insert(id.to_owned(), result);
        result
    }
}

/// The estimate for `entry` and everything it calls.
pub fn estimate(pack: &DataPack, loops: &HashMap<ResourceLocation, LoopInfo>, entry: &ResourceLocation) -> Estimate {
    let mut bodies = HashMap::new();
    for (id, f) in pack.functions() {
        let text = f.render();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).collect();
        let calls = lines.iter().flat_map(|l| calls_in(l)).collect();
        bodies.insert(id.to_string(), (lines.len() as u64, calls));
    }
    let loops = loops.iter().map(|(k, v)| (k.to_string(), v)).collect();
    let mut e = Estimator { bodies, loops, memo: HashMap::new(), stack: HashSet::new(), heaviest: None };
    let (low, high) = e.cost(&entry.to_string());
    Estimate { low, high, heaviest: e.heaviest }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_calls_in_a_line() {
        let ids = |l: &str| calls_in(l).into_iter().map(|c| (c.id, c.conditional)).collect::<Vec<_>>();
        assert_eq!(ids("function t:a"), [("t:a".to_owned(), false)]);
        assert_eq!(ids("execute as @a at @s run function t:f/b0"), [("t:f/b0".to_owned(), false)]);
        assert_eq!(ids("execute store success score #x t run function t:m with storage t:macro"), [("t:m".to_owned(), false)]);
        assert_eq!(ids("return run function t:loop"), [("t:loop".to_owned(), false)]);
        // a call under a condition is conditional; the test `if function x` of an unconditional line is not
        assert_eq!(ids("execute if score #a t matches 1.. run function t:b"), [("t:b".to_owned(), true)]);
        assert_eq!(ids("execute unless entity @s run function t:b"), [("t:b".to_owned(), true)]);
        assert_eq!(ids("execute if function t:b run return 1"), [("t:b".to_owned(), false)]);
        assert_eq!(ids("execute if score #a t matches 1.. if function t:b run return 1"), [("t:b".to_owned(), true)]);
        assert!(ids("schedule function t:later 1t").is_empty());
        assert!(ids("say function t:nope").is_empty());
        assert!(ids("tellraw @a {text:\"function t:nope\"}").is_empty());
        assert!(ids("scoreboard players set #a t 1").is_empty());
    }
}
