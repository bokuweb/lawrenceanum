use super::*;
use scraper::Html;
fn word(id: usize, text: &str, b: [f32; 4]) -> Word {
    Word {
        source_box: [b[0], b[1], b[2], b[3]],
        id,
        x0: b[0],
        y0: b[1],
        x1: b[2],
        y1: b[3],
        raw: text.into(),
        text: text.into(),
        status: WordStatus::Used,
    }
}
fn cell(row: usize, col: usize, text: &str, ids: Vec<usize>) -> TableCell {
    TableCell {
        row,
        col,
        rowspan: 1,
        colspan: 1,
        x0: if col == 0 { 0. } else { 100. },
        x1: if col == 0 { 100. } else { 150. },
        y0: if row == 0 { 0. } else { 10. },
        y1: if row == 0 { 10. } else { 40. },
        text: text.into(),
        words: ids,
    }
}
fn fixture() -> (Table, Vec<Word>, [f32; 4]) {
    let w = vec![
        word(0, "科目", [10., 1., 26., 9.]),
        word(1, "金額", [105., 1., 121., 9.]),
        word(2, "前の科目", [10., 13., 42., 21.]),
        word(3, "正しい科目", [10., 23., 50., 31.]),
        word(4, "△", [96., 19., 104., 27.]),
    ];
    let t = Table {
        x0: 0.,
        y0: 0.,
        x1: 150.,
        y1: 40.,
        rows: 3,
        cols: 2,
        cells: vec![
            cell(0, 0, "科目", vec![0]),
            cell(0, 1, "金額", vec![1]),
            cell(1, 0, "前の科目△", vec![2, 4]),
            cell(2, 0, "正しい科目", vec![3]),
        ],
    };
    (t, w, [104., 23., 112., 31.])
}
#[test]
fn moves_only_triangle_to_unique_empty_amount_cell() {
    let (mut t, w, b) = fixture();
    let old = t.clone();
    assert!(move_in_table(&mut t, &w, 4, b));
    assert_eq!(t.rows, old.rows);
    assert_eq!(t.cols, old.cols);
    assert_eq!(t.cells[0], old.cells[0]);
    assert_eq!(t.cells[1], old.cells[1]);
    assert_eq!(t.cells[3], old.cells[3]);
    assert_eq!(t.cells[2].text, "前の科目");
    assert_eq!(t.cells[2].words, vec![2]);
    assert_eq!(t.cells[4].row, 2);
    assert_eq!(t.cells[4].col, 1);
    assert_eq!(t.cells[4].words, vec![4]);
    assert_eq!(t.cells[4].text, "△");
    assert!(!move_in_table(&mut t, &w, 4, b));
}
#[test]
fn existing_amount_is_never_overwritten() {
    let (mut t, mut w, b) = fixture();
    w.push(word(5, "42", [120., 23., 128., 31.]));
    t.cells.push(cell(2, 1, "42", vec![5]));
    let before = t.clone();
    assert!(!move_in_table(&mut t, &w, 4, b));
    assert_eq!(t, before);
}
#[test]
fn merged_explanatory_label_is_not_a_sign_source() {
    let (mut t, w, b) = fixture();
    t.cells[2].colspan = 2;
    let before = t.clone();
    assert!(!move_in_table(&mut t, &w, 4, b));
    assert_eq!(t, before);
}
#[test]
fn multiple_printed_labels_are_not_split() {
    let (mut t, mut w, b) = fixture();
    w.push(word(5, "もう一つの科目", [10., 33., 66., 41.]));
    t.cells[3].words.push(5);
    t.cells[3].text.push_str("\nもう一つの科目");
    let before = t.clone();
    assert!(!move_in_table(&mut t, &w, 4, b));
    assert_eq!(t, before);
}
#[test]
fn a_different_baseline_is_not_a_record_match() {
    let (mut t, w, _) = fixture();
    let before = t.clone();
    assert!(!move_in_table(&mut t, &w, 4, [104., 31., 112., 39.]));
    assert_eq!(t, before);
}
#[test]
fn a_non_amount_header_is_not_guessed() {
    let (mut t, w, b) = fixture();
    t.cells[1].text = "番号".into();
    let before = t.clone();
    assert!(!move_in_table(&mut t, &w, 4, b));
    assert_eq!(t, before);
}
#[test]
fn source_text_must_be_fully_explained_by_source_words() {
    let (mut t, w, b) = fixture();
    t.cells[2].text.push('0');
    let before = t.clone();
    assert!(!move_in_table(&mut t, &w, 4, b));
    assert_eq!(t, before);
}
#[test]
fn duplicate_source_owner_is_refused() {
    let (mut t, w, b) = fixture();
    t.cells[3].words.push(4);
    let before = t.clone();
    assert!(!move_in_table(&mut t, &w, 4, b));
    assert_eq!(t, before);
}
#[test]
fn two_matching_label_rows_are_ambiguous() {
    let (mut t, w, b) = fixture();
    let mut c = t.cells[3].clone();
    c.row = 1;
    t.cells.push(c);
    let before = t.clone();
    assert!(!move_in_table(&mut t, &w, 4, b));
    assert_eq!(t, before);
}
fn metadata(
    mode: &str,
    ax: &str,
    extra: &str,
    text: &str,
    status: WordStatus,
) -> Vec<(usize, [f32; 4])> {
    let html=format!("<page><word xmin='96' ymin='19' xmax='104' ymax='27' data-wmode='{mode}' data-advance-x='{ax}' data-advance-y='0' data-native-xmin='104' data-native-ymin='23' data-native-xmax='112' data-native-ymax='31'>{text}</word>{extra}</page>");
    let doc = Html::parse_document(&html);
    let selector = Selector::parse("page").unwrap();
    let mut w = word(0, text, [96., 19., 104., 27.]);
    w.status = status;
    native_signs(doc.select(&selector).next().unwrap(), &[w])
}
#[test]
fn native_box_requires_horizontal_wmode_one_advance() {
    assert_eq!(
        metadata("1", "8", "", "△", WordStatus::Used),
        vec![(0, [104., 23., 112., 31.])]
    );
    assert!(metadata("0", "8", "", "△", WordStatus::Used).is_empty());
    assert!(metadata("1", "0", "", "△", WordStatus::Used).is_empty());
}
#[test]
fn duplicate_unannotated_source_does_not_inherit_metadata() {
    assert!(metadata(
        "1",
        "8",
        "<word xmin='96' ymin='19' xmax='104' ymax='27'>△</word>",
        "△",
        WordStatus::Used
    )
    .is_empty());
}
#[test]
fn unknown_or_unowned_words_are_not_promoted() {
    assert!(metadata("1", "8", "", "未知", WordStatus::Used).is_empty());
    assert!(metadata("1", "8", "", "△", WordStatus::Noise).is_empty());
}
