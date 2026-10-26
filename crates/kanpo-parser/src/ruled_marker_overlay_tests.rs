use super::*;

fn pair(after: usize, before: usize) -> Unit {
    Unit::Shinkyu {labels:None,
        preamble: vec![],
        after: vec![vec![Line::text("alpha".into(), vec![after])]],
        before: vec![vec![Line::text("（新設）".into(), vec![before])]],
        outside: vec![],
    }
}
fn proposal() -> NativeLayout {
    NativeLayout {
        units: vec![pair(0, 1)],
        members: vec![],
    }
}
fn mixed() -> Line {
    Line::text("alpha（新設）".into(), vec![0, 1])
}
fn snapshot(units: &[Unit]) -> serde_json::Value {
    serde_json::Value::Array(units.iter().map(|u| match u {
        Unit::Plain(ls) => serde_json::json!({"plain":ls}),
        Unit::Shinkyu { preamble, after, before, outside ,..} => serde_json::json!({"preamble":preamble,"after":after,"before":before,"outside":outside}),
    }).collect())
}

#[test]
fn split_only_the_complete_plain_block_keep_prefix_and_suffix_line_bytes() {
    let prefix = Line::text("corrected prefix".into(), vec![90]);
    let suffix = Line::text("corrected suffix".into(), vec![91]);
    let mut units = vec![Unit::Plain(vec![prefix.clone(), mixed(), suffix.clone()])];
    let mut plan = proposal();
    plan.units.push(Unit::Plain(vec![Line::text(
        "uncorrected remainder".into(),
        vec![90, 91],
    )]));
    assert!(apply(&mut units, plan, &[]).is_some());
    assert_eq!(units.len(), 3);
    let Unit::Plain(a) = &units[0] else { panic!() };
    assert_eq!(a, &vec![prefix]);
    let Unit::Plain(b) = &units[2] else { panic!() };
    assert_eq!(b, &vec![suffix]);
    assert_eq!(
        pair_roots(&units[1]).unwrap(),
        [(0, true), (1, false)].into_iter().collect()
    );
}

#[test]
fn an_existing_comparison_in_the_remainder_cannot_become_plain_text() {
    let mut units = vec![pair(10, 11), Unit::Plain(vec![mixed()])];
    let known = snapshot(&units)[0].clone();
    let mut plan = proposal();
    plan.units.insert(
        0,
        Unit::Plain(vec![Line::text("alpha（新設）".into(), vec![10, 11])]),
    );
    assert!(apply(&mut units, plan, &[]).is_some());
    assert_eq!(snapshot(&units)[0], known);
    assert_eq!(
        pair_roots(&units[1]).unwrap(),
        [(0, true), (1, false)].into_iter().collect()
    );
}

#[test]
fn a_previously_classified_comparison_is_not_rebuilt() {
    let mut units = vec![pair(0, 1)];
    let before = snapshot(&units);
    assert!(apply(&mut units, proposal(), &[]).is_none());
    assert_eq!(snapshot(&units), before);
}

#[test]
fn a_plain_line_with_an_unproved_source_member_is_not_split() {
    let mut l = mixed();
    l.words.push(90);
    let mut units = vec![Unit::Plain(vec![l])];
    let before = snapshot(&units);
    assert!(apply(&mut units, proposal(), &[]).is_none());
    assert_eq!(snapshot(&units), before);
}

#[test]
fn a_foreign_notice_between_the_matched_lines_is_not_crossed() {
    let mut units = vec![Unit::Plain(vec![
        Line::text("alpha".into(), vec![0]),
        Line::text("foreign notice".into(), vec![90]),
        Line::text("（新設）".into(), vec![1]),
    ])];
    let before = snapshot(&units);
    assert!(apply(&mut units, proposal(), &[]).is_none());
    assert_eq!(snapshot(&units), before);
}

#[test]
fn a_completed_table_is_kept_with_all_its_cell_attributes() {
    let table = crate::trace::Table {
        x0: 10.,
        y0: 0.,
        x1: 30.,
        y1: 120.,
        rows: 2,
        cols: 1,
        cells: ["alpha", "（新設）"]
            .iter()
            .enumerate()
            .map(|(i, t)| crate::trace::TableCell {
                row: i,
                col: 0,
                rowspan: 1,
                colspan: 1,
                x0: 10.,
                y0: i as f32 * 60.,
                x1: 30.,
                y1: (i + 1) as f32 * 60.,
                text: (*t).into(),
                words: vec![i],
            })
            .collect(),
    };
    let line = Line {
        text: table.render(),
        words: vec![],
        table: Some(table),
    };
    let mut units = vec![Unit::Plain(vec![line])];
    let before = snapshot(&units);
    assert!(apply(&mut units, proposal(), &[]).is_none());
    assert_eq!(snapshot(&units), before);
}

#[test]
fn a_correction_from_an_existing_pass_cannot_be_lost() {
    let mut l = mixed();
    l.text.push('①');
    let mut units = vec![Unit::Plain(vec![l])];
    let before = snapshot(&units);
    assert!(apply(&mut units, proposal(), &[]).is_none());
    assert_eq!(snapshot(&units), before);
}

#[test]
fn source_members_are_expanded_only_for_newly_accepted_roots() {
    let mut units = vec![Unit::Plain(vec![
        Line::text("remainder".into(), vec![90, 91]),
        mixed(),
    ])];
    let mut plan = proposal();
    plan.members = vec![(7, 0), (6, 0), (8, 90), (9, 91)];
    let inline = [(8, 90), (7, 0)];
    let members = apply(&mut units, plan, &inline).unwrap();
    assert_eq!(members, vec![(8, 90), (7, 0), (6, 0)]);
    let mut regions: Vec<_> = units
        .into_iter()
        .enumerate()
        .flat_map(|(i, u)| u.regions(i))
        .collect();
    super::super::enclosed_warichu::expand(&mut regions, &members, &[(6, 0), (9, 91)]);
    let roots: std::collections::HashMap<_, _> = regions
        .iter()
        .flat_map(|r| {
            r.lines
                .iter()
                .flat_map(move |l| l.words.iter().map(move |&id| (id, r.role)))
        })
        .collect();
    for id in [0, 6, 7] {
        assert_eq!(roots[&id], crate::trace::Role::After)
    }
    assert_eq!(roots[&8], crate::trace::Role::Body);
    assert!(!roots.contains_key(&9));
}
