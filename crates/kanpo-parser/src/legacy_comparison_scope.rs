//! The scope retained by legacy inference, independently of native frame proof.
use crate::trace::Unit;
pub(crate) fn continues(units: &[Unit]) -> bool {
    match units.last() {
        Some(Unit::Shinkyu { outside, .. }) => outside.iter().flatten().all(|line| {
            line.text.trim().is_empty() && line.words.is_empty() && line.table.is_none()
        }),
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Line, Table};
    fn comparison(outside: Vec<Vec<Line>>) -> Unit {
        Unit::Shinkyu {
            labels: None,
            preamble: vec![Line::text("任意の前文。".into(), vec![])],
            after: vec![vec![Line::text("任意の左欄".into(), vec![])]],
            before: vec![vec![Line::text("任意の右欄".into(), vec![])]],
            outside,
        }
    }
    #[test]
    fn the_last_comparison_without_outside_content_can_continue() {
        assert!(continues(&[comparison(vec![])]));
        assert!(continues(&[comparison(vec![
            vec![],
            vec![Line::text("　 \n".into(), vec![])]
        ])]));
    }
    #[test]
    fn emitted_outside_text_ends_legacy_scope_for_arbitrary_wording() {
        for text in ["独立した後置き。", "別紙", "α", "注"] {
            assert!(
                !continues(&[comparison(vec![vec![Line::text(text.into(), vec![])]])]),
                "{text}"
            );
        }
    }
    #[test]
    fn an_outside_source_occurrence_is_content_even_if_normalized_text_is_empty() {
        assert!(!continues(&[comparison(vec![vec![Line::text(
            String::new(),
            vec![17]
        )]])]));
    }
    #[test]
    fn an_outside_table_does_not_need_visible_text_to_end_scope() {
        let table = Table {
            x0: 0.,
            y0: 0.,
            x1: 10.,
            y1: 10.,
            rows: 1,
            cols: 1,
            cells: vec![],
        };
        assert!(!continues(&[comparison(vec![vec![Line {
            text: String::new(),
            words: vec![],
            table: Some(table)
        }]])]));
    }
    #[test]
    fn only_the_last_emitted_unit_can_supply_legacy_continuation() {
        assert!(!continues(&[]));
        assert!(!continues(&[
            comparison(vec![]),
            Unit::Plain(vec![Line::text("後続本文".into(), vec![])])
        ]));
        assert!(continues(&[Unit::Plain(vec![]), comparison(vec![])]));
    }
}
