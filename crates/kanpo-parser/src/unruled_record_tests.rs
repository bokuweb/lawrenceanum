use super::*;
fn word(ws: &mut Vec<Col>, x: f32, end: f32, y: f32, text: &str) {
    ws.push(Col {
        id: ws.len(),
        x,
        x1: end,
        y,
        y1: y + 8.,
        text: text.into(),
    });
}
fn shape() -> Vec<Col> {
    let mut ws = vec![];
    for g in 0..2 {
        let top = g as f32 * 160.;
        word(&mut ws, 20., 220., top + 10., "令和8年6月1日付の対象");
        for (x, end, t) in [
            (20., 44., "番号"),
            (80., 120., "種類"),
            (150., 220., "名称"),
            (250., 330., "業者住所"),
        ] {
            word(&mut ws, x, end, top + 24., t);
        }
        for r in 0..3 {
            let y = top + 40. + r as f32 * 34.;
            word(&mut ws, 24., 44., y, &format!("{}", 10000 + g * 10 + r));
            word(&mut ws, 80., 120., y, "種別同文");
            word(&mut ws, 150., 210., y, "名称同文");
            if r == 1 {
                word(&mut ws, 250., 258., y, "〃");
            } else {
                word(&mut ws, 250., 330., y, "製造者甲株式会社");
                word(&mut ws, 250., 330., y + 10., "東京都試験町1番地");
                word(&mut ws, 250., 290., y + 20., "代表者乙");
            }
        }
    }
    ws
}
fn read(w: &[Col]) -> Option<Vec<Line>> {
    repeated_header_records(w, &[], &[], &[])
}
#[test]
fn complete_repeated_headers_certify_record_rows_without_losing_duplicate_names_or_ditto() {
    let w = shape();
    let lines = read(&w).expect("certified table");
    let ts: Vec<_> = lines.iter().filter_map(|l| l.table.as_ref()).collect();
    assert_eq!(ts.len(), 2);
    assert!(ts.iter().all(|t| t.rows == 4 && t.cols == 4));
    let ids: Vec<_> = lines
        .iter()
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words)))
        .copied()
        .collect();
    let got: BTreeSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), w.len());
    assert_eq!(got, (0..w.len()).collect());
    assert_eq!(ts.iter().flat_map(|t| &t.cells).filter(|c| c.text == "名称同文").count(), 6);
    assert_eq!(ts.iter().flat_map(|t| &t.cells).filter(|c| c.text == "〃").count(), 2);
    for t in ts {
        for r in 1..4 {
            assert_eq!(t.cells.iter().filter(|c| c.row == r).count(), 4);
        }
    }
}
#[test]
fn a_true_two_column_article_with_no_repeated_complete_header_is_rejected() {
    let mut w = shape();
    w.retain(|w| w.y < 160.);
    assert!(read(&w).is_none());
}
#[test]
fn missing_a_field_cannot_be_inferred_from_the_next_record() {
    let mut w = shape();
    w.retain(|w| !(w.x == 150. && w.y == 74.));
    assert!(read(&w).is_none());
}
#[test]
fn every_field_must_start_on_its_key_baseline() {
    let mut w = shape();
    let z = w.iter_mut().find(|w| w.x == 250. && w.y == 40.).unwrap();
    z.y += 4.;
    z.y1 += 4.;
    assert!(read(&w).is_none());
}
#[test]
fn a_word_crossing_the_proposed_field_boundary_is_not_split() {
    let mut w = shape();
    w.iter_mut().find(|w| w.x == 150. && w.y == 40.).unwrap().x1 = 252.;
    assert!(read(&w).is_none());
}
#[test]
fn wrapped_text_reaching_the_next_key_has_no_full_width_gap() {
    let mut w = shape();
    word(&mut w, 250., 330., 71., "未完の折返し");
    assert!(read(&w).is_none());
}
#[test]
fn a_following_notice_heading_is_not_an_owner_field() {
    let mut w = shape();
    word(&mut w, 250., 330., 146., "〇次の告示第1号");
    assert!(read(&w).is_none());
}
#[test]
fn unrelated_text_above_the_first_caption_is_not_dropped() {
    let mut w = shape();
    word(&mut w, 20., 220., 0., "別記事の末尾");
    assert!(read(&w).is_none());
}
#[test]
fn two_different_header_signatures_do_not_form_a_family() {
    let mut w = shape();
    w.iter_mut().find(|w| w.y == 184. && w.x == 150.).unwrap().text = "別の欄".into();
    assert!(read(&w).is_none());
}
#[test]
fn shifted_repeated_fields_are_not_the_same_physical_columns() {
    let mut w = shape();
    for z in w.iter_mut().filter(|w| w.y >= 160.) {
        z.x += 10.;
        z.x1 += 10.;
    }
    assert!(read(&w).is_none());
}
#[test]
fn a_native_column_divider_is_not_crossed() {
    let w = shape();
    assert!(repeated_header_records(&w, &[(135., 0., 310.)], &[], &[]).is_none());
}
#[test]
fn native_horizontal_table_rules_are_not_overwritten() {
    let w = shape();
    assert!(repeated_header_records(&w, &[], &[(73., 20., 330.)], &[]).is_none());
}
#[test]
fn an_existing_table_domain_is_not_rebuilt() {
    let w = shape();
    assert!(repeated_header_records(&w, &[], &[], &[(10., 20., 340., 300.)]).is_none());
}
#[test]
fn duplicate_source_ids_are_rejected() {
    let mut w = shape();
    w[20].id = w[19].id;
    assert!(read(&w).is_none());
}
#[test]
fn an_unknown_private_glyph_is_not_invented_as_field_text() {
    let mut w = shape();
    w.iter_mut().find(|w| w.x == 150. && w.y == 40.).unwrap().text = "\u{eabc}".into();
    assert!(read(&w).is_none());
}
#[test]
fn one_numeric_key_field_is_required_not_digit_paragraphs() {
    let mut w = shape();
    w.iter_mut().find(|w| w.x == 24. && w.y == 40.).unwrap().text = "1 条の本文".into();
    assert!(read(&w).is_none());
}
