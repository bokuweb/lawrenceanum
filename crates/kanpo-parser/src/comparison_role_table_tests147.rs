use super::*;
fn fixture() -> (Table, BTreeSet<usize>, Vec<Vec<Line>>) {
    let cells = [("前", 1), ("後", 2)]
        .into_iter()
        .enumerate()
        .map(|(col, (text, id))| TableCell {
            row: 0,
            col,
            rowspan: 1,
            colspan: 1,
            x0: col as f32,
            y0: 0.,
            x1: col as f32 + 1.,
            y1: 1.,
            text: text.into(),
            words: vec![id],
        })
        .collect();
    let t = Table {
        x0: 0.,
        y0: 0.,
        x1: 2.,
        y1: 1.,
        rows: 1,
        cols: 2,
        cells,
    };
    (
        t,
        [1, 2].into_iter().collect(),
        vec![
            vec![
                Line::text("前".into(), vec![1]),
                Line::text("図面の説明".into(), vec![9]),
            ],
            vec![Line::text("後".into(), vec![2])],
        ],
    )
}
#[test]
fn role_table147_complete_native_ids_rejoin_across_rendered_parts_without_consuming_other_lines() {
    let (t, ids, mut parts) = fixture();
    assert!(restore_parts(&mut parts, &t, &ids));
    assert_eq!(parts[0].len(), 2);
    assert_eq!(parts[0][0].table.as_ref(), Some(&t));
    assert_eq!(parts[0][1].text, "図面の説明");
    assert_eq!(parts[0][1].words, vec![9]);
    assert!(parts[1].is_empty());
    assert!(!restore_parts(&mut parts, &t, &ids));
}
#[test]
fn role_table147_an_independent_publication_heading_is_a_fence() {
    let (t, ids, mut p) = fixture();
    p[0][1].text = "〇別省告示第二号".into();
    let old = p.clone();
    assert!(!restore_parts(&mut p, &t, &ids));
    assert_eq!(p, old);
}
#[test]
fn role_table147_missing_source_id_declines() {
    let (t, ids, mut p) = fixture();
    p[1].clear();
    assert!(!restore_parts(&mut p, &t, &ids));
}
#[test]
fn role_table147_duplicate_source_id_declines() {
    let (t, ids, mut p) = fixture();
    p[1].push(Line::text("前".into(), vec![1]));
    assert!(!restore_parts(&mut p, &t, &ids));
}
#[test]
fn role_table147_mixed_foreign_word_in_one_line_declines() {
    let (t, ids, mut p) = fixture();
    p[1][0].words.push(77);
    assert!(!restore_parts(&mut p, &t, &ids));
}
#[test]
fn role_table147_same_surface_with_foreign_ids_does_not_authorize_restoration() {
    let (t, ids, mut p) = fixture();
    p[1][0].words = vec![77];
    assert!(!restore_parts(&mut p, &t, &ids));
}
#[test]
fn role_table147_native_character_loss_declines() {
    let (t, ids, mut p) = fixture();
    p[1][0].text = "欠落".into();
    assert!(!restore_parts(&mut p, &t, &ids));
}
#[test]
fn role_table147_an_existing_intervening_table_is_not_crossed() {
    let (t, ids, mut p) = fixture();
    p[0][1].table = Some(t.clone());
    assert!(!restore_parts(&mut p, &t, &ids));
}
#[test]
fn role_table147_source_id_bijection_preserves_the_rule() {
    let (mut t, _, mut p) = fixture();
    for c in &mut t.cells {
        for id in &mut c.words {
            *id = *id * 19 + 400;
        }
    }
    for part in &mut p {
        for l in part {
            for id in &mut l.words {
                *id = *id * 19 + 400;
            }
        }
    }
    let ids = t
        .cells
        .iter()
        .flat_map(|c| c.words.iter().copied())
        .collect();
    assert!(restore_parts(&mut p, &t, &ids));
}
