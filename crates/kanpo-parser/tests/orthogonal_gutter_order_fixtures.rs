//! Each assertion follows independently reviewed original printed fields.
//! These fixtures make every adopted changed-source relationship replayable.
//! Word IDs are source-test anchors only; production has no page/text branches.
fn source_order(bytes: &[u8], expected: &[usize]) {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(bytes).unwrap());
    let mut actual = Vec::new();
    for region in &a.pages[0].regions {
        for line in &region.lines {
            actual.extend(line.words.iter().copied());
            if let Some(table) = &line.table {
                let mut cells: Vec<_> = table.cells.iter().collect();
                cells.sort_by_key(|cell| (cell.row, cell.col));
                actual.extend(
                    cells
                        .into_iter()
                        .flat_map(|cell| cell.words.iter().copied()),
                );
            }
        }
    }
    let projected: Vec<_> = actual
        .into_iter()
        .filter(|id| expected.contains(id))
        .collect();
    assert_eq!(
        projected, expected,
        "Original source fields must retain their order and unique ownership"
    );
}

#[test]
fn original_20260824g001880032_fields_follow_their_printed_order() {
    source_order(
        include_bytes!("data/orthogonal-gutters/20260824g001880032.pdf"),
        &[10, 5, 0, 9, 6, 11, 7, 3, 121],
    );
}

#[test]
fn original_20260824g001880036_fields_follow_their_printed_order() {
    source_order(
        include_bytes!("data/orthogonal-gutters/20260824g001880036.pdf"),
        &[9, 5, 1, 8, 6, 11, 7, 4, 105],
    );
}

#[test]
fn original_20260831g001940039_fields_follow_their_printed_order() {
    source_order(
        include_bytes!("data/orthogonal-gutter-order/20260831g001940039.pdf"),
        &[171, 6, 5, 4, 3, 2, 1, 0, 212, 213, 210],
    );
}

#[test]
fn original_20260910g002020100_fields_follow_their_printed_order() {
    source_order(
        include_bytes!("data/orthogonal-gutter-order/20260910g002020100.pdf"),
        &[352, 351, 349, 348, 346, 338, 332, 323, 354, 270, 385, 372],
    );
}

#[test]
fn original_20260930g002160066_fields_follow_their_printed_order() {
    source_order(
        include_bytes!("data/orthogonal-gutter-order/20260930g002160066.pdf"),
        &[71, 45, 6, 63, 28, 36, 157, 152, 153, 154, 155],
    );
}

#[test]
fn original_20261016g002280369_fields_follow_their_printed_order() {
    source_order(
        include_bytes!("data/orthogonal-gutter-order/20261016g002280369.pdf"),
        &[76, 62, 58, 54, 45, 39, 28, 20, 69, 255, 229],
    );
}

#[test]
fn original_20261016g002280496_fields_follow_their_printed_order() {
    source_order(
        include_bytes!("data/orthogonal-gutter-order/20261016g002280496.pdf"),
        &[377, 378, 223, 222, 221, 220, 219, 218, 217, 422, 423, 420],
    );
}

#[test]
fn original_20261016g002280554_fields_follow_their_printed_order() {
    source_order(
        include_bytes!("data/orthogonal-gutter-order/20261016g002280554.pdf"),
        &[300, 183, 180, 178, 176, 174, 338, 339, 334],
    );
}
