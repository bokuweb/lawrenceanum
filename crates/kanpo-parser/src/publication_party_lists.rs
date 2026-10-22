//! Named operands of an unlabelled rights-transfer declaration.
//! Header names are source values, not a company dictionary. One complete
//! first declaration must have a unique parse; incidental mentions are not operands.
use std::collections::{BTreeMap, BTreeSet};
fn advance(stack: &mut Vec<char>, c: char) -> Option<()> {
    if stack.last().is_some_and(|c| matches!(c, '」' | '』')) {
        match c {
            '「' => stack.push('」'),
            '『' => stack.push('』'),
            '」' | '』' => {
                if stack.pop() != Some(c) {
                    return None;
                }
            }
            _ => {}
        }
        return Some(());
    }
    match c {
        '(' => stack.push(')'),
        '（' => stack.push('）'),
        '「' => stack.push('」'),
        '『' => stack.push('』'),
        ')' | '）' | '」' | '』' => {
            if stack.pop() != Some(c) {
                return None;
            }
        }
        _ => {}
    }
    Some(())
}
fn first_sentence(body: &str) -> Option<&str> {
    let mut stack = vec![];
    for (i, c) in body.char_indices() {
        if stack.is_empty() {
            if c == '。' {
                return Some(&body[..i]);
            }
            if matches!(c, '!' | '?' | '！' | '？') {
                return None;
            }
        }
        advance(&mut stack, c)?;
    }
    stack.is_empty().then_some(body)
}
fn annotations_end(s: &str, mut pos: usize) -> Option<usize> {
    while s[pos..].starts_with(['(', '（']) {
        let mut stack = vec![];
        let mut end = None;
        for (i, c) in s[pos..].char_indices() {
            advance(&mut stack, c)?;
            if stack.is_empty() {
                end = Some(pos + i + c.len_utf8());
                break;
            }
        }
        pos = end?;
    }
    Some(pos)
}
// Accept a complete positive assertion. A matching verb stem inside a
// negation, question, condition, hypothesis, or unfinished clause is not one.
// This is deliberately a finite grammatical production, not a substring
// score or a deny-list of observed failures. Unparsed inflections stay unknown.
fn affirmative_transfer(rest: &str) -> bool {
    let rest = rest
        .strip_suffix("ので公告いたします")
        .or_else(|| rest.strip_suffix("ので公告します"))
        .unwrap_or(rest);
    if ["します", "しました", "いたします", "いたしました"].contains(&rest) {
        return true;
    }
    ["することに", "することと"].iter().any(|prefix| {
        rest.strip_prefix(prefix)
            .is_some_and(|v| ["します", "しました", "いたします", "いたしました"].contains(&v))
    })
}
fn rights_predicate(property: &str) -> bool {
    let mut stack = vec![];
    let mut start = 0;
    let mut spans = vec![];
    for (i, c) in property.char_indices() {
        let before = stack.is_empty();
        if before && matches!(c, '(' | '（' | '「' | '『') {
            spans.push((start, i));
        }
        if advance(&mut stack, c).is_none() {
            return false;
        }
        if !before && stack.is_empty() {
            start = i + c.len_utf8();
        }
    }
    if !stack.is_empty() {
        return false;
    }
    spans.push((start, property.len()));
    let marker = "権利義務を承継";
    let clauses = spans
        .into_iter()
        .flat_map(|(a, b)| {
            property[a..b]
                .match_indices(marker)
                .map(move |(i, _)| a + i + marker.len())
        })
        .collect::<Vec<_>>();
    clauses.len() == 1 && affirmative_transfer(&property[clauses[0]..])
}
pub(super) fn rights_transfer_parties(
    body: &str,
    action: &str,
    names: &BTreeSet<String>,
) -> Option<BTreeSet<String>> {
    if action.is_empty() || names.is_empty() {
        return None;
    }
    let sentence = first_sentence(body)?;
    // Labelled declarations have their own explicit-role grammar. This production
    // handles a direct first-person subject and named, optionally annotated operands.
    let tail = sentence
        .strip_prefix("当社は")?
        .trim_start_matches(['、', ','])
        .strip_prefix(action)?
        .strip_prefix("により")?;
    let mut states: BTreeMap<usize, (u8, Option<BTreeSet<String>>)> =
        BTreeMap::from([(0, (1, Some(BTreeSet::new())))]);
    let mut complete = 0u8;
    let mut result = None;
    while let Some((pos, (count, owners))) = states.pop_first() {
        for name in names.iter().filter(|s| !s.is_empty()) {
            if !tail[pos..].starts_with(name) {
                continue;
            }
            let Some(end) = annotations_end(tail, pos + name.len()) else {
                continue;
            };
            let next_owners = owners.as_ref().map(|set| {
                let mut set = set.clone();
                set.insert(name.clone());
                set
            });
            if let Some(property) = tail[end..].strip_prefix('の') {
                if rights_predicate(property) {
                    // Two is the logical "not unique" state, not an empirical retry/count cap.
                    complete = (complete + count).min(2);
                    if complete != 1 {
                        return None;
                    }
                    result = next_owners.clone();
                }
            }
            for separator in [
                "、および",
                "、及び",
                "、並びに",
                "、",
                ",",
                "および",
                "及び",
                "並びに",
                "と",
            ] {
                if tail[end..].starts_with(separator) {
                    let next = end + separator.len();
                    let state = states.entry(next).or_insert((0, None));
                    if state.0 == 0 && count == 1 {
                        state.1 = next_owners.clone();
                    } else {
                        state.1 = None;
                    }
                    state.0 = (state.0 + count).min(2);
                }
            }
        }
    }
    if complete == 1 {
        result
    } else {
        None
    }
}
#[cfg(test)]
#[path = "publication_party_lists_tests.rs"]
mod tests;
