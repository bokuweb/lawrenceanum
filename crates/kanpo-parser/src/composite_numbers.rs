//! DF Mincho parenthesized numbers printed as two overlaid font components.
use crate::normalize::word_text;
type Raw = (f32, f32, f32, f32, String);
/// Retain both source words and their IDs; assign each its visible half only
/// when the known left/right font components share the same final em square.
pub(crate) fn overlaid_private_numbers(raws: &[Raw]) -> Vec<Option<String>> {
    let mut out = vec![None; raws.len()];
    let mut pairs = Vec::new();
    for (i, a) in raws.iter().enumerate() {
        let Some(t) = a.4.chars().last() else {
            continue;
        };
        if !matches!(t, '\u{EAC1}' | '\u{EAC2}') {
            continue;
        }
        let matching: Vec<_> = raws
            .iter()
            .enumerate()
            .filter_map(|(j, b)| {
                let halves = match (t, b.4.as_str()) {
                    ('\u{EAC1}', "\u{EACB}") => ("（１", "１）"),
                    ('\u{EAC1}', "\u{EACC}") => ("（１", "２）"),
                    ('\u{EAC2}', "\u{EACD}") => ("（２", "３）"),
                    _ => return None,
                };
                let em = b.2 - b.0;
                if i == j
                    || em <= 0.
                    || !(em * 0.9..=em * 1.2).contains(&(b.3 - b.1))
                    || a.2 - a.0 < em * 0.9
                    || (a.2 - b.2).abs() > em * 0.05
                    || (a.1 - b.1).abs() > em * 0.05
                    || (a.3 - b.3).abs() > em * 0.05
                {
                    return None;
                }
                Some((j, halves))
            })
            .collect();
        if let [(j, (left, right))] = matching.as_slice() {
            pairs.push((i, *j, *left, *right));
        }
    }
    for &(i, j, left, right) in &pairs {
        if pairs.iter().filter(|p| p.1 == j).count() != 1 {
            continue;
        }
        let a = &raws[i].4;
        let prefix = a.char_indices().next_back().map_or("", |(n, _)| &a[..n]);
        let prefix = word_text(prefix);
        // A standalone item marker needs an adjacent continuation on its own
        // baseline. Otherwise a tier/table boundary can assign it to the
        // preceding record; leave such unanchored markers unchanged.
        let em = raws[j].2 - raws[j].0;
        if prefix.is_empty()
            && !raws.iter().enumerate().any(|(k, c)| {
                let text = word_text(&c.4);
                k != i
                    && k != j
                    && c.0 >= raws[j].2 - em * 0.05
                    && c.0 <= raws[j].2 + em * 1.5
                    && (c.1 - raws[j].1).abs() <= em * 0.1
                    && (c.3 - raws[j].3).abs() <= em * 0.2
                    && text
                        .chars()
                        .filter(|c| ('\u{3040}'..='\u{30ff}').contains(c) || ('\u{3400}'..='\u{9fff}').contains(c))
                        .count()
                        >= 3
            })
        {
            continue;
        }
        out[i] = Some(format!("{prefix}{left}"));
        out[j] = Some(right.into());
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    fn r(x: f32, y: f32, w: f32, t: &str) -> Raw {
        (x, y, x + w, y + 8.184, t.into())
    }
    #[test]
    fn three_visually_verified_pairs_restore_both_word_halves() {
        for (a, b, l, rh) in [
            ("\u{EAC1}", "\u{EACB}", "（１", "１）"),
            ("\u{EAC1}", "\u{EACC}", "（１", "２）"),
            ("\u{EAC2}", "\u{EACD}", "（２", "３）"),
        ] {
            assert_eq!(
                overlaid_private_numbers(&[r(20., 30., 8., a), r(20., 30., 8., b), r(36., 30., 80., "比較推奨販売")]),
                vec![Some(l.into()), Some(rh.into()), None]
            );
        }
    }
    #[test]
    fn a_prefix_keeps_its_text_and_both_source_ids() {
        assert_eq!(
            overlaid_private_numbers(&[r(12., 30., 16., "［\u{EAC1}"), r(20., 30., 8., "\u{EACC}")]),
            vec![Some("［（１".into()), Some("２）".into())]
        );
    }
    #[test]
    fn unverified_pairs_are_not_partially_restored() {
        for (a, b) in [("\u{EAC2}", "\u{EACB}"), ("\u{EAC1}", "\u{EACE}"), ("\u{EAC3}", "\u{EACD}")] {
            assert!(
                overlaid_private_numbers(&[r(20., 30., 8., a), r(20., 30., 8., b)])
                    .iter()
                    .all(Option::is_none)
            );
        }
    }
    #[test]
    fn components_at_different_positions_are_not_combined() {
        for b in [r(28., 30., 8., "\u{EACB}"), r(20., 39., 8., "\u{EACB}"), r(20., 30., 6., "\u{EACB}")] {
            assert!(overlaid_private_numbers(&[r(20., 30., 8., "\u{EAC1}"), b]).iter().all(Option::is_none));
        }
    }
    #[test]
    fn incomplete_components_stay_unmapped() {
        assert_eq!(overlaid_private_numbers(&[r(20., 30., 8., "\u{EAC1}")]), vec![None]);
    }
    #[test]
    fn ambiguous_duplicate_components_stay_unmapped() {
        let a = r(20., 30., 8., "\u{EAC1}");
        let b = r(20., 30., 8., "\u{EACB}");
        assert!(overlaid_private_numbers(&[a.clone(), a, b.clone(), b]).iter().all(Option::is_none));
    }
}

#[cfg(test)]
mod inline_guard_tests {
    use super::*;
    fn r(x: f32, y: f32, t: &str) -> Raw {
        (x, y, x + 8., y + 8.184, t.into())
    }
    #[test]
    fn unanchored_standalone_marker_does_not_enter_the_layout() {
        let raws = vec![r(20., 100., "\u{EAC1}"), r(20., 100., "\u{EACC}"), r(20., 112., "届出日")];
        assert!(overlaid_private_numbers(&raws).iter().all(Option::is_none));
    }
    #[test]
    fn a_distant_same_baseline_article_is_not_an_inline_anchor() {
        let raws = vec![r(20., 100., "\u{EAC1}"), r(20., 100., "\u{EACC}"), r(120., 100., "比較推奨販売")];
        assert!(overlaid_private_numbers(&raws).iter().all(Option::is_none));
    }
}
