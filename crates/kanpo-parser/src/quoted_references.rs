//! A complete leaf quotation may certify an internal physical line break in a
//! hierarchical legal identifier. The original text is never normalized here.
use std::collections::{BTreeMap, BTreeSet};
fn number(c: char) -> bool {
    c.is_ascii_digit()
        || ('０'..='９').contains(&c)
        || "〇零一二三四五六七八九十百千万億兆".contains(c)
}
fn ordinal(c: &[char], at: &mut usize) -> bool {
    let start = *at;
    while c.get(*at).copied().is_some_and(number) {
        *at += 1;
    }
    *at > start
}
fn component(c: &[char], at: &mut usize) -> Option<u8> {
    if c.get(*at) != Some(&'第') {
        return None;
    }
    *at += 1;
    if !ordinal(c, at) {
        return None;
    }
    let rank = match c.get(*at)? {
        '条' => 0,
        '項' => 1,
        '号' => 2,
        _ => return None,
    };
    *at += 1;
    while c.get(*at) == Some(&'の') {
        *at += 1;
        if !ordinal(c, at) {
            return None;
        }
    }
    Some(rank)
}
fn identifier(payload: &[char]) -> bool {
    // Two numeral pieces can be either a split number or separate list items.
    // Text alone cannot choose one. No length or column-fill threshold decides.
    for (i, &c) in payload.iter().enumerate() {
        if c == '\n'
            && i > 0
            && number(payload[i - 1])
            && payload.get(i + 1).copied().is_some_and(number)
        {
            return false;
        }
    }
    let c = payload
        .iter()
        .copied()
        .filter(|c| *c != '\n')
        .collect::<Vec<_>>();
    let mut at = 0;
    let Some(mut rank) = component(&c, &mut at) else {
        return false;
    };
    while at < c.len() {
        let Some(next) = component(&c, &mut at) else {
            return false;
        };
        if next <= rank {
            return false;
        }
        rank = next;
    }
    true
}
pub(crate) fn boundaries(lines: &[&str]) -> BTreeSet<usize> {
    let mut c = vec![];
    let mut separators = BTreeMap::new();
    for (row, line) in lines.iter().enumerate() {
        if row > 0 {
            separators.insert(c.len(), row - 1);
            c.push('\n');
        }
        for ch in line.chars() {
            c.push(ch);
        }
    }
    let mut stack: Vec<(usize, char, bool)> = vec![];
    let mut out = BTreeSet::new();
    for (at, &ch) in c.iter().enumerate() {
        match ch {
            '「' | '『' => {
                if let Some(parent) = stack.last_mut() {
                    parent.2 = true;
                }
                stack.push((at, if ch == '「' { '」' } else { '』' }, false));
            }
            '」' | '』' => {
                let Some((start, close, nested)) = stack.pop() else {
                    continue;
                };
                if close != ch {
                    stack.clear();
                    continue;
                }
                if nested {
                    continue;
                }
                let payload = &c[start + 1..at];
                if !identifier(payload) {
                    continue;
                }
                // Only separators inserted between input elements correspond
                // to a boundary controlled by the renderer. An embedded LF
                // is unknown provenance and cannot authorize another boundary.
                let Some(breaks) = (start + 1..at)
                    .filter(|&i| c[i] == '\n')
                    .map(|i| separators.get(&i).copied())
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                if breaks.iter().all(|&r| {
                    !lines[r].is_empty() && lines.get(r + 1).is_some_and(|l| !l.is_empty())
                }) {
                    out.extend(breaks);
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod source_boundary_tests {
    use super::*;
    #[test]
    fn an_internal_break_in_one_item_cannot_authorize_joining_the_following_item() {
        assert!(boundaries(&["「第1条の\n２」", "第三項"]).is_empty());
    }
}

#[cfg(test)]
#[path = "quoted_reference_tests.rs"]
mod reference_contract_tests;
