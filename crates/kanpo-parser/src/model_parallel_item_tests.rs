use super::*;
fn row(a: &str, b: &str) -> ShinkyuRow {
    ShinkyuRow {
        after: vec![Run::plain(a)],
        before: vec![Run::plain(b)],
        ..Default::default()
    }
}
fn joined(a: ShinkyuRow, b: ShinkyuRow) -> Vec<ShinkyuRow> {
    let r = merge_continued_tables(vec![
        Block::Shinkyu {labels:None, rows: vec![a] },
        Block::Shinkyu {labels:None, rows: vec![b] },
    ]);
    let Block::Shinkyu { rows ,..} = r.into_iter().next().unwrap() else {
        panic!()
    };
    rows
}
#[test]
fn adjacent_declared_items_keep_their_two_own_rows() {
    let a = row("十七　先行する項目。", "九　従前の項目。");
    let b = row("十八　次の項目。", "十　従前の次の項目。");
    assert_eq!(joined(a.clone(), b.clone()), vec![a, b]);
}
#[test]
fn individually_added_items_do_not_become_the_previous_added_items_tail() {
    let a = row("三十二　先行する追加項目。", "〔号を加える。〕");
    let b = row("三十三　次の追加項目。", "〔号を加える。〕");
    assert_eq!(joined(a.clone(), b.clone()), vec![a, b]);
}
#[test]
fn an_article_keeps_its_continuing_arabic_paragraphs() {
    let rs = joined(
        row("第三条　本文。", "第二条　旧本文。"),
        row("２　続く段落。", "２　旧段落。"),
    );
    assert_eq!(rs.len(), 1);
    assert!(runs_plain(&rs[0].after).contains("第三条　本文。\n２　続く段落。"));
}
#[test]
fn items_inside_an_existing_article_can_continue_that_article() {
    let rs = joined(
        row("第三条　次に掲げる。", "第二条　次に掲げる。"),
        row("一　同じ条の号。", "一　同じ旧条の号。"),
    );
    assert_eq!(rs.len(), 1);
    assert!(runs_plain(&rs[0].after).contains("第三条　次に掲げる。\n一　同じ条の号。"));
}
#[test]
fn an_item_reference_is_a_sentence_continuation_not_a_new_declaration() {
    let rs = joined(
        row("当該手続は、", "旧手続は、"),
        row("二十八号に規定するもの。", "二十六号に規定するもの。"),
    );
    assert_eq!(rs.len(), 1);
    assert!(runs_plain(&rs[0].after).contains("二十八号に規定するもの。"));
}
#[test]
fn an_asymmetric_field_continuation_does_not_borrow_a_new_joint_row_boundary() {
    let rs = joined(
        row("十七　前段の本文。", "九　従前の本文"),
        row("十八　後続する本文。", "の続きである。"),
    );
    assert_eq!(rs.len(), 1);
    assert!(runs_plain(&rs[0].before).contains("従前の本文の続きである。"));
}
#[test]
fn declared_item_numbers_have_no_two_digit_or_character_count_limit() {
    let a = row("千二百三十四　先行する項目。", "百二　従前の項目。");
    let b = row("千二百三十五　次の項目。", "百三　従前の次の項目。");
    assert_eq!(joined(a.clone(), b.clone()), vec![a, b]);
}
#[test]
fn a_declared_next_item_keeps_its_original_cell_table_and_run_style() {
    let a = row("十七　前の項目。", "九　前の項目。");
    let mut b = row("十八　次の項目。", "十　次の項目。");
    b.after[0].underline = true;
    b.after_table = Some(NestedTable {
        rows: vec![vec![vec![Run::plain("元の表セル")]]],
        merges: vec![],
    });
    assert_eq!(joined(a.clone(), b.clone()), vec![a, b]);
}
