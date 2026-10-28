//! Quote-scope evidence for complete legal replacement operations. Source text
//! is never edited. Different parses may share an endpoint, but competing
//! endpoints do not authorize a scope release.
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug)]
pub(crate) struct State {
    pub empty: bool,
    pub valid: bool,
}
impl State {
    pub fn strict(self) -> bool {
        self.empty && self.valid
    }
}
fn number(c: char) -> bool {
    c.is_ascii_digit()
        || ('０'..='９').contains(&c)
        || "〇零一二三四五六七八九十百千万億兆".contains(c)
}
fn starts(c: &[char], at: usize, text: &str) -> bool {
    c.get(at..).is_some_and(|tail| {
        tail.iter().copied().zip(text.chars()).all(|(a, b)| a == b)
            && tail.len() >= text.chars().count()
    })
}
fn ordinal(c: &[char], mut at: usize) -> Option<usize> {
    let first = at;
    while c.get(at).copied().is_some_and(number) {
        at += 1;
    }
    (at > first).then_some(at)
}
fn component(c: &[char], at: usize) -> Option<usize> {
    let unit = |v: char| "条項号編章節款目".contains(v);
    let mut end = if c.get(at) == Some(&'同') && c.get(at + 1).copied().is_some_and(unit) {
        at + 2
    } else if c.get(at) == Some(&'第') {
        let e = ordinal(c, at + 1)?;
        if !c.get(e).copied().is_some_and(unit) {
            return None;
        }
        e + 1
    } else if starts(c, at, "別表") || starts(c, at, "様式") {
        let e = at + 2;
        ordinal(c, e + usize::from(c.get(e) == Some(&'第')))?
    } else {
        return None;
    };
    while c.get(end) == Some(&'の') {
        let Some(e) = ordinal(c, end + 1) else {
            break;
        };
        end = e;
    }
    Some(end)
}
fn location(c: &[char], start: usize) -> Option<usize> {
    let mut at = component(c, start)?;
    loop {
        if c.get(at) == Some(&'中') {
            return Some(at + 1);
        }
        if let Some(end) = component(c, at) {
            at = end;
            continue;
        }
        let connector = ["及び", "並びに", "から", "まで"]
            .iter()
            .find(|v| starts(c, at, v))?;
        at = component(c, at + connector.chars().count())?;
    }
}
fn balanced_end(c: &[char], start: usize) -> Option<usize> {
    if c.get(start) != Some(&'「') {
        return None;
    }
    let mut stack = vec![];
    for (i, &v) in c.iter().enumerate().skip(start) {
        match v {
            '「' => stack.push('」'),
            '『' => stack.push('』'),
            '」' | '』' => {
                if stack.pop() != Some(v) {
                    return None;
                }
                if stack.is_empty() {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}
// A balanced quoted core may be cut at a delimiter boundary: unmatched
// opening delimiters form a prefix and unmatched closers form a suffix. An
// interior unclosed quote is ambiguous with an unfinished outer operand;
// it cannot authorize ending that outer quotation. No payload vocabulary,
// length or nesting-depth threshold is used.
fn boundary_fragment(c: &[char]) -> bool {
    let mut opens = vec![];
    let mut closes = vec![];
    for (i, &v) in c.iter().enumerate() {
        match v {
            '「' | '『' => opens.push((i, if v == '「' { '」' } else { '』' })),
            '」' | '』' => {
                if opens.last().is_some_and(|&(_, end)| end == v) {
                    opens.pop();
                } else if opens.is_empty() {
                    closes.push(i);
                } else {
                    return false;
                }
            }
            _ => {}
        }
    }
    opens.iter().enumerate().all(|(i, &(at, _))| i == at)
        && closes
            .iter()
            .enumerate()
            .all(|(i, &at)| at == c.len() - closes.len() + i)
}
fn closers(c: &[char], start: usize, after: &str) -> Vec<usize> {
    if c.get(start) != Some(&'「') {
        return vec![];
    }
    let fits = |e| starts(c, e, after);
    if let Some(e) = balanced_end(c, start).filter(|&e| fits(e)) {
        return vec![e];
    }
    c.iter()
        .enumerate()
        .skip(start + 1)
        .filter_map(|(i, &v)| {
            (v == '」' && fits(i + 1) && boundary_fragment(&c[start + 1..i])).then_some(i + 1)
        })
        .collect()
}
struct Parser<'a> {
    c: &'a [char],
    lists: HashMap<usize, BTreeSet<usize>>,
    clauses: HashMap<usize, BTreeSet<usize>>,
}
impl Parser<'_> {
    fn clause(&mut self, start: usize) -> BTreeSet<usize> {
        if let Some(v) = self.clauses.get(&start) {
            return v.clone();
        }
        let out = location(self.c, start).map_or_else(BTreeSet::new, |at| self.list(at));
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
                    for a in closers(self.c, at, "を「") {
                        for b in closers(self.c, a + 1, "に") {
                            let next = b + 1;
                            if starts(self.c, next, "改める。") {
                                ends.insert(next + 4);
                            }
                            if starts(self.c, next, "、「") {
                                dependencies.insert(next + 1);
                            }
                            if starts(self.c, next, "改め、") {
                                if let Some(operand) = location(self.c, next + 3) {
                                    dependencies.insert(operand);
                                }
                            }
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
        let operand = location(self.c, start)?;
        if balanced_end(self.c, operand).is_none()
            && (operand + 1..end)
                .any(|i| location(self.c, i).is_some() && self.clause(i).iter().any(|&e| e <= end))
        {
            return None;
        }
        Some(end)
    }
}

/// Only complete original Word/Line ownership can authorize literal-operand
/// interpretation. Unknown input keeps its raw quote events, but supplies no
/// operation certificate. This verifies characters and source identity; native
/// heading typography and domain geometry remain the callers' separate proof.
pub(crate) fn source_lines(page: &crate::trace::PageTrace) -> Vec<(&str, bool)> {
    use crate::trace::{Role, WordStatus};
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let mut seen = std::collections::BTreeSet::new();
    let mut complete = true;
    let rows = page
        .regions
        .iter()
        .flat_map(|r| r.lines.iter().map(move |l| (r.role, l)))
        .collect::<Vec<_>>();
    for &(_, line) in &rows {
        let ids = if let Some(t) = &line.table {
            t.cells
                .iter()
                .flat_map(|c| c.words.iter().copied())
                .collect::<Vec<_>>()
        } else {
            line.words.clone()
        };
        let mut text = String::new();
        for id in ids {
            if let Some(w) = page.words.get(id) {
                complete &= w.id == id
                    && w.status == WordStatus::Used
                    && !w.raw.is_empty()
                    && seen.insert(id);
                text.push_str(&w.text);
            } else {
                complete = false;
            }
        }
        if line.table.is_none() {
            complete &= compact(&text) == compact(&line.text);
        }
    }
    complete &= page
        .words
        .iter()
        .all(|w| w.status != WordStatus::Used || seen.contains(&w.id));
    rows.into_iter()
        .map(|(role, l)| {
            (
                l.text.as_str(),
                complete && role == Role::Body && l.table.is_none(),
            )
        })
        .collect()
}

pub(crate) fn states(lines: &[(&str, bool)]) -> Vec<State> {
    states_impl(lines, None)
}
pub(crate) fn states_with_scopes(
    lines: &[(&str, bool)],
    scopes: &crate::native_quote_scopes::Scopes<'_>,
) -> Vec<State> {
    states_impl(
        lines,
        (!scopes.is_empty() && scopes.matches(lines)).then_some(scopes),
    )
}
fn states_impl(
    lines: &[(&str, bool)],
    scopes: Option<&crate::native_quote_scopes::Scopes<'_>>,
) -> Vec<State> {
    let mut chars = vec![];
    let mut rows = vec![];
    let mut allowed = vec![];
    let mut row_starts=vec![];
    for (row, &(text, eligible)) in lines.iter().enumerate() {
        if scopes.is_some() {row_starts.push(chars.len());}
        for c in text.chars().filter(|c| !c.is_whitespace()) {
            chars.push(c);
            rows.push(row);
            allowed.push(eligible);
        }
        if !eligible {
            chars.push('\0');
            rows.push(row);
            allowed.push(false);
        }
    }
    let mut out = vec![None; lines.len()];
    let mut stack = vec![];
    let mut valid = true;
    let mut at = 0;
    let mut row = 0;
    while at < chars.len() {
        while row <= rows[at] {
            out[row] = Some(State {
                empty: stack.is_empty(),
                valid,
            });
            row += 1;
        }
        if valid && allowed[at] && (location(&chars, at).is_some()
            || scopes.is_some_and(|s|row_starts[rows[at]]==at&&s.end(rows[at]).is_some()&&literal_operations::can_start(&chars,at))) {
            let stop = (at..chars.len())
                .find(|&i| !allowed[i])
                .unwrap_or(chars.len());
            let mut parser = Parser {
                c: &chars[..stop],
                lists: HashMap::new(),
                clauses: HashMap::new(),
            };
            let end = parser.certificate(at).or_else(|| {
                let scopes = scopes?;
                if row_starts[rows[at]] != at {
                    return None;
                }
                let last = scopes.end(rows[at])?;
                if last <= rows[at] || last > lines.len() {
                    return None;
                }
                let bound = row_starts
                    .get(last)
                    .copied()
                    .unwrap_or(chars.len())
                    .min(stop);
                literal_operations::endpoint(&chars, at, bound)
            });
            if let Some(end) = end {
                for r in rows[at] + 1..=rows[end - 1] {
                    out[r] = Some(State {
                        empty: false,
                        valid,
                    });
                }
                row = row.max(rows[end - 1] + 1);
                at = end;
                continue;
            }
        }
        match chars[at] {
            '「' => stack.push('」'),
            '『' => stack.push('』'),
            '」' | '』' => {
                if stack.last() == Some(&chars[at]) {
                    stack.pop();
                } else {
                    valid = false;
                }
            }
            _ => {}
        }
        at += 1;
    }
    while row < out.len() {
        out[row] = Some(State {
            empty: stack.is_empty(),
            valid,
        });
        row += 1;
    }
    out.into_iter().map(Option::unwrap).collect()
}

#[path = "native_literal_operations.rs"]
mod literal_operations;

#[cfg(test)]
mod tests {
    use super::*;
    fn following(text: &str) -> State {
        states(&[(text, true), ("次の本文", true)])[1]
    }
    #[test]
    fn legal_location_grammar_is_not_a_fixed_ordinal_or_context_value() {
        for loc in [
            "第一条",
            "第１２条の３",
            "同条第一項",
            "別表第七",
            "様式第九十七の二",
        ] {
            assert!(
                following(&format!("{loc}中「「旧字句」を「新字句」に改める。")).strict(),
                "{loc}"
            );
        }
    }
    #[test]
    fn a_plain_middle_word_is_not_a_legal_location() {
        for loc in ["本文", "第条", "第三", "同文", "未知の用語"] {
            assert!(!following(&format!("{loc}中「「旧字句」を「新字句」に改める。")).strict());
        }
    }
    #[test]
    fn a_complete_operation_can_wrap_across_source_lines_without_releasing_its_interior() {
        let s = states(&[
            ("第三条中「「旧", true),
            ("字句」を「新字句」に改める。", true),
            ("次の本文", true),
        ]);
        assert!(s[0].strict());
        assert!(!s[1].strict());
        assert!(s[2].strict());
    }
    #[test]
    fn a_whole_table_or_another_role_cannot_supply_the_rest_of_an_operation() {
        for middle in ["", "「新字句」に改める。"] {
            let s = states(&[
                ("第三条中「「旧字句」を", true),
                (middle, false),
                ("「新字句」に改める。", true),
                ("次の本文", true),
            ]);
            assert!(!s[3].strict());
        }
    }
    #[test]
    fn a_true_outer_quote_survives_an_embedded_complete_replacement() {
        let s = states(&[
            ("引用「", true),
            ("第三条中「「旧字句」を「新字句」に改める。", true),
            ("次の本文", true),
            ("」", true),
            ("引用の後", true),
        ]);
        assert!(!s[1].strict());
        assert!(!s[2].strict());
        assert!(s[4].strict());
    }
    #[test]
    fn an_unknown_malformed_closure_before_an_operation_remains_unknown() {
        let s = following("未知の閉じ引用」第三条中「「旧字句」を「新字句」に改める。");
        assert!(!s.valid);
        assert!(!s.strict());
    }
    #[test]
    fn another_true_quote_after_a_complete_operation_is_not_erased() {
        assert!(!following("第三条中「「旧字句」を「新字句」に改める。引用「").strict());
    }
    #[test]
    fn interior_unmatched_closing_delimiters_are_not_a_boundary_fragment() {
        for text in ["甲」乙", "甲』乙", "甲「乙", "甲『乙"] {
            assert!(
                !following(&format!("第三条中「{text}」を「新字句」に改める。")).strict(),
                "{text}"
            );
        }
    }
    #[test]
    fn literal_boundary_depth_has_no_fixed_cap() {
        for n in [1, 3, 17, 65] {
            assert!(following(&format!(
                "第三条中「{}甲」を「新字句」に改める。",
                "「".repeat(n)
            ))
            .strict());
            assert!(following(&format!(
                "第三条中「甲{}」を「新字句」に改める。",
                "」".repeat(n)
            ))
            .strict());
        }
    }
    #[test]
    fn a_later_balanced_quotation_of_an_operation_does_not_change_earlier_operand_scope() {
        assert!(following(
            "第三条中「「旧字句」を「新字句」に改める。引用「第四条中「旧」を「新」に改める。」"
        )
        .strict());
    }
    #[test]
    fn unsupported_operation_grammar_remains_unproven() {
        for suffix in ["に", "と記す。", "に改める", "に改め、未知の続き。"] {
            assert!(!following(&format!("第三条中「「旧字句」を「新字句」{suffix}")).strict());
        }
    }
    #[test]
    fn a_large_complete_operation_list_does_not_recurse_per_replacement() {
        let mut s = String::from("第三条中「「旧」を「新」に、");
        for _ in 0..4097 {
            s.push_str("「別の旧項目」を「別の新項目」に、");
        }
        s.push_str("「末尾の旧項目」を「末尾の新項目」に改める。");
        assert!(following(&s).strict());
    }
}

#[cfg(test)]
mod clause_chain_regression {
    use super::*;
    #[test]
    fn many_legal_clause_transitions_use_no_recursive_call_chain() {
        let mut text = String::from("第一条中「旧」を「新」に改め、");
        for _ in 0..4097 {
            text.push_str("第２条中「別の旧項目」を「別の新項目」に改め、");
        }
        text.push_str("第三条中「末尾の旧項目」を「末尾の新項目」に改める。");
        let chars = text.chars().collect::<Vec<_>>();
        let mut parser = Parser {
            c: &chars,
            lists: HashMap::new(),
            clauses: HashMap::new(),
        };
        assert_eq!(parser.certificate(0), Some(chars.len()));
        assert!(states(&[(&text, true), ("独立した後続本文", true)])[1].strict());
    }
}
