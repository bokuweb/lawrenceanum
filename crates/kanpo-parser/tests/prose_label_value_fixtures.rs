fn analyze(pdf: &[u8]) -> kanpo_parser::Analysis {
    kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap())
}
fn fixture() -> kanpo_parser::Analysis {
    analyze(include_bytes!(
        "archive/pages/20260826h01776/20260826h017760008.pdf"
    ))
}
fn assert_ids(a: &kanpo_parser::Analysis) {
    let p = &a.pages[0];
    let mut seen = std::collections::HashSet::new();
    for l in p.regions.iter().flat_map(|r| &r.lines) {
        for &id in l.words.iter().chain(
            l.table
                .iter()
                .flat_map(|t| t.cells.iter().flat_map(|c| &c.words)),
        ) {
            assert!(seen.insert(id), "duplicate source word {id}");
        }
    }
    for w in &p.words {
        assert_eq!(
            seen.contains(&w.id),
            w.status == kanpo_parser::trace::WordStatus::Used,
            "source word {}",
            w.id
        );
    }
}
#[test]
fn aligned_numbered_labels_keep_each_prose_value_before_the_next_notice() {
    let a = fixture();
    assert_eq!(
        a.text,
        include_str!("archive/expected-prose-label-values/20260826h017760008.txt")
    );
    assert_ids(&a);
    // 原本の四つの対応を確認する。本文最後の縦列が次記事「公告」より先に来る。
    let ordered = [
        "（四）制限の対象とする占用物件新たに地上に設ける電柱",
        "（五）占用を制限する理由緊急輸送道路",
        "（六）占用の制限の開始の期日令和八年八月二十七日",
        "（七）図面縦覧場所九州地方整備局及び同局北九州国道事務所",
        "\n公告\n諸事項\n相続財産清算人の選任及び相\n続権主張の催告",
    ];
    let mut end = 0;
    for s in ordered {
        end += a.text[end..].find(s).expect(s) + s.len();
    }
}
#[test]
fn prose_values_rejoin_the_original_label_columns_without_heading_ids() {
    let a = fixture();
    assert_ids(&a);
    let lines: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines).collect();
    let expected: [&[usize]; 13] = [
        &[33, 29, 0],
        &[1],
        &[2],
        &[3],
        &[4],
        &[34, 30, 27, 22, 20, 17, 15, 14, 12, 10, 5],
        &[6],
        &[35, 31, 7],
        &[36, 32, 26, 19, 16, 13, 11, 8],
        &[21, 9],
        &[28, 25, 18],
        &[23],
        &[24],
    ];
    assert_eq!(lines.len(), expected.len());
    for (l, w) in lines.iter().zip(expected) {
        assert_eq!(l.words, w, "{}", l.text);
    }
}
#[test]
fn neighboring_ordinary_notice_and_numeric_area_table_keep_their_existing_tiers() {
    let a = analyze(include_bytes!(
        "archive/pages/20260826h01776/20260826h017760007.pdf"
    ));
    assert_eq!(
        a.text,
        include_str!("archive/expected-prose-label-values/20260826h017760007.txt")
    );
    assert_ids(&a);
    assert!(
        a.text
            .contains("（一）道路の種類一般国道\n（二）路線名二百一号\n（三）占用を制限する区域")
    );
}

#[test]
fn the_hokkaido_notice_keeps_all_four_labels_with_their_original_values() {
    let a = analyze(include_bytes!(
        "archive/pages/20260723h01753/20260723h017530007.pdf"
    ));
    assert_eq!(
        a.text,
        include_str!("archive/expected-prose-label-values/20260723h017530007.txt")
    );
}

#[test]
fn the_mixed_statistics_page_keeps_the_complete_notice_and_statistics() {
    let a = analyze(include_bytes!(
        "archive/pages/20260930g00215/20260930g002150475.pdf"
    ));
    assert_eq!(
        a.text,
        include_str!("archive/expected-prose-label-values/20260930g002150475.txt")
    );
}
