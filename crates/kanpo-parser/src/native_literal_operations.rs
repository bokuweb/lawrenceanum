//! Literal edit grammar is available only inside a separately certified source paragraph.
use super::{balanced_end, boundary_fragment, component, starts};
use std::collections::{BTreeSet, HashMap};
#[derive(Clone, Copy)]
enum Target {
    Partial(usize),
    Whole(usize),
}
fn target(c: &[char], start: usize) -> Option<Target> {
    let mut at = component(c, start)?;
    loop {
        if starts(c, at, "の見出し") {
            at += 4;
        }
        match c.get(at) {
            Some('中') => return Some(Target::Partial(at + 1)),
            Some('を') => return Some(Target::Whole(at + 1)),
            _ => {}
        }
        if let Some(end) = component(c, at) {
            at = end;
            continue;
        }
        let connector = ["及び", "並びに", "から", "まで"]
            .iter()
            .find(|s| starts(c, at, s))?;
        at = component(c, at + connector.chars().count())?;
    }
}
pub(super) fn can_start(c: &[char], at: usize) -> bool {
    target(c, at).is_some()
}
fn quote_free(c: &[char]) -> bool {
    !c.iter().any(|c| matches!(c, '「' | '」' | '『' | '』'))
}
enum Resolved {
    Operands(usize),
    End(usize),
}
fn closers(c: &[char], start: usize, after: &str) -> Vec<usize> {
    if c.get(start) != Some(&'「') {
        return vec![];
    }
    let fits = |e| starts(c, e, after);
    if let Some(e) = balanced_end(c, start) {
        if fits(e) {
            return vec![e];
        }
        // A balanced operand already followed by another supported operator
        // cannot borrow a later operation's delimiter for this operator.
        if ["を「", "に改め", "に、「", "の下に「", "と、「", "を加え"]
            .iter()
            .any(|tail| starts(c, e, tail))
        {
            return vec![];
        }
    }
    c.iter()
        .enumerate()
        .skip(start + 1)
        .filter_map(|(i, &v)| (v == '」' && fits(i + 1)).then_some(i + 1))
        .collect()
}
// A fragment acts on its surrounding quote stack by consuming unmatched
// closers, then leaving unmatched openers. Equal actions preserve context even
// when the unmatched delimiters occur inside the literal operand.
fn quote_action(c: &[char]) -> Option<(Vec<char>, Vec<char>)> {
    let mut closes = vec![];
    let mut opens = vec![];
    for &ch in c {
        match ch {
            '「' => opens.push('」'),
            '『' => opens.push('』'),
            '」' | '』' => {
                if let Some(expected) = opens.pop() {
                    if expected != ch {
                        return None;
                    }
                } else {
                    closes.push(ch);
                }
            }
            _ => {}
        }
    }
    Some((closes, opens))
}
fn replacement_operands(a: &[char], b: &[char]) -> bool {
    (boundary_fragment(a) && boundary_fragment(b))
        || quote_action(a)
            .zip(quote_action(b))
            .is_some_and(|(x, y)| x == y)
}
fn insertion_anchors(c: &[char], start: usize) -> BTreeSet<usize> {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    let mut out = BTreeSet::new();
    while let Some(at) = pending.pop() {
        if !seen.insert(at) {
            continue;
        }
        for e in closers(c, at, "の下に「") {
            if quote_action(&c[at + 1..e - 1]).is_some() {
                out.insert(e + 3);
            }
        }
        for e in closers(c, at, "と、「") {
            if quote_action(&c[at + 1..e - 1]).is_some() {
                pending.push(e + 2);
            }
        }
    }
    out
}
struct Parser<'a> {
    c: &'a [char],
    lists: HashMap<usize, BTreeSet<usize>>,
    clauses: HashMap<usize, BTreeSet<usize>>,
    neutral_suffix: bool,
}
impl Parser<'_> {
    // Whole-object replacements can prefix partial edits. Resolve their
    // balanced literal steps iteratively, without a call frame per heading.
    fn resolve(&self, start: usize) -> Option<Resolved> {
        let mut at = start;
        let mut completed = false;
        loop {
            if completed && self.neutral_suffix && quote_free(&self.c[at..]) {
                return Some(Resolved::End(self.c.len()));
            }
            match target(self.c, at)? {
                Target::Partial(operand) => return Some(Resolved::Operands(operand)),
                Target::Whole(operand) => {
                    let end = balanced_end(self.c, operand)?;
                    if starts(self.c, end, "に改める。") {
                        return Some(Resolved::End(end + 5));
                    }
                    if !starts(self.c, end, "に改め、") {
                        return None;
                    }
                    at = end + 4;
                    completed = true;
                }
            }
        }
    }
    fn clause(&mut self, start: usize) -> BTreeSet<usize> {
        if let Some(v) = self.clauses.get(&start) {
            return v.clone();
        }
        let out = match self.resolve(start) {
            Some(Resolved::Operands(at)) => self.list(at),
            Some(Resolved::End(end)) => BTreeSet::from([end]),
            None => BTreeSet::new(),
        };
        self.clauses.insert(start, out.clone());
        out
    }
    fn list(&mut self, start: usize) -> BTreeSet<usize> {
        // Every dependency starts strictly after its caller. Evaluate this DAG
        // with an explicit work stack; a long amendment has no Rust call frame
        // per operand or per legal clause. Retain every competing endpoint.
        enum Work {
            Enter(usize),
            Finish(usize, BTreeSet<usize>, BTreeSet<usize>),
        }
        let mut work = vec![Work::Enter(start)];
        while let Some(task) = work.pop() {
            match task {
                Work::Enter(at) => {
                    if self.lists.contains_key(&at) {
                        continue;
                    }
                    let mut ends = BTreeSet::new();
                    let mut dependencies = BTreeSet::new();
                    let following =
                        |next: usize,
                         verb: &str,
                         ends: &mut BTreeSet<usize>,
                         dependencies: &mut BTreeSet<usize>| {
                            if starts(self.c, next, &format!("{verb}る。")) {
                                ends.insert(next + 4);
                            }
                            if starts(self.c, next, &format!("{verb}、")) {
                                let at = next + 3;
                                // The completed quoted step is the only parsed
                                // authority here. The rest of the independently
                                // bounded paragraph has no quotation events.
                                if self.neutral_suffix && quote_free(&self.c[at..]) {
                                    ends.insert(self.c.len());
                                }
                                if self.c.get(at) == Some(&'「') {
                                    dependencies.insert(at);
                                }
                                match self.resolve(at) {
                                    Some(Resolved::Operands(operand)) => {
                                        dependencies.insert(operand);
                                    }
                                    Some(Resolved::End(end)) => {
                                        ends.insert(end);
                                    }
                                    None => {}
                                }
                            }
                        };
                    let mut pairs = vec![];
                    for a in closers(self.c, at, "を「") {
                        for b in closers(self.c, a + 1, "に") {
                            if replacement_operands(&self.c[at + 1..a - 1], &self.c[a + 2..b - 1]) {
                                pairs.push((a, b));
                            }
                        }
                    }
                    let anchors = insertion_anchors(self.c, at);
                    // Bind the first proved operand/operator boundary. A later
                    // insertion anchor cannot absorb an already bound literal
                    // replacement and subsequent statements as its own text.
                    // Keep every competing end of that same bound operand.
                    let boundary = pairs
                        .iter()
                        .map(|&(a, _)| a)
                        .chain(anchors.iter().map(|&a| a - 3))
                        .min();
                    for (a, b) in pairs {
                        if Some(a) != boundary {
                            continue;
                        }
                        let next = b + 1;
                        following(next, "改め", &mut ends, &mut dependencies);
                        if starts(self.c, next, "、「") {
                            dependencies.insert(next + 1);
                        }
                    }
                    for operand in anchors {
                        if Some(operand - 3) != boundary {
                            continue;
                        }
                        for b in closers(self.c, operand, "を") {
                            if quote_action(&self.c[operand + 1..b - 1]) != Some((vec![], vec![])) {
                                continue;
                            }
                            following(b + 1, "加え", &mut ends, &mut dependencies);
                        }
                    }
                    work.push(Work::Finish(at, ends, dependencies.clone()));
                    work.extend(dependencies.into_iter().map(Work::Enter));
                }
                Work::Finish(at, mut ends, dependencies) => {
                    for dep in dependencies {
                        ends.extend(self.lists.get(&dep).expect("forward dependency evaluated"));
                    }
                    self.lists.insert(at, ends);
                }
            }
        }
        self.lists.get(&start).cloned().unwrap_or_default()
    }
    fn certificate(&mut self, start: usize) -> Option<usize> {
        let ends = self.clause(start);
        if ends.len() != 1 {
            return None;
        }
        let end = *ends.first()?;
        // An incomplete outer operand must not borrow a complete operation
        // from a nested legal context. Balanced operands can explicitly quote
        // arbitrary complete operations; their real delimiters remain binding.
        let operand = match target(self.c, start)? {
            Target::Partial(at) | Target::Whole(at) => at,
        };
        if balanced_end(self.c, operand).is_none()
            && (operand + 1..end)
                .any(|i| target(self.c, i).is_some() && self.clause(i).iter().any(|&e| e <= end))
        {
            return None;
        }
        Some(end)
    }
}

pub(super) fn endpoint(c: &[char], start: usize, limit: usize) -> Option<usize> {
    if start >= limit || limit > c.len() {
        return None;
    }
    let mut p = Parser {
        c: &c[..limit],
        lists: HashMap::new(),
        clauses: HashMap::new(),
        neutral_suffix: true,
    };
    p.certificate(start)
}
#[cfg(test)]
#[path = "native_literal_operation_tests.rs"]
mod tests;
