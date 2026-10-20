fn owners(a: &kanpo_parser::Analysis, id: usize) -> usize {
    a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .map(|l| {
            l.words.iter().filter(|&&i| i == id).count()
                + l.table
                    .iter()
                    .flat_map(|t| &t.cells)
                    .map(|c| c.words.iter().filter(|&&i| i == id).count())
                    .sum::<usize>()
        })
        .sum()
}
#[test]
fn reference_symbols_keep_their_table_records_and_following_paragraphs() {
    let pdf=include_bytes!("archive/pages/20261016g00228/20261016g002280106.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let t = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.rows == 8 && t.cols == 5)
        .unwrap();
    for (row, text, span, description, amount) in [
        (1, "(A)", 1, "全学業務用電話加入権", "1,666千円"),
        (2, "(B)", 3, "さくら寮", "36,886千円"),
        (5, "(C)", 3, "吼洋寮Ａ棟", "2,874千円"),
    ] {
        let c = t
            .cells
            .iter()
            .find(|c| c.row == row && c.col == 0)
            .expect("reference symbol cell");
        assert_eq!(c.text, text);
        assert_eq!(c.rowspan, span);
        assert!(t
            .cells
            .iter()
            .any(|c| c.row == row && c.col == 1 && c.text == description));
        assert!(t
            .cells
            .iter()
            .any(|c| c.row == row && c.col == 4 && c.text == amount));
    }
    let marked = a
        .text
        .lines()
        .filter(|l| l.starts_with("(A)") || l.starts_with("(B)") || l.starts_with("(C)"))
        .collect::<Vec<_>>();
    assert!(marked
        .iter()
        .any(|l| l.starts_with("(A)") && l.contains("固定資産の使用状況")));
    assert_eq!(
        marked
            .iter()
            .filter(|l| l.contains("入寮対象となる外国人留学生"))
            .count(),
        2
    );
    assert!(marked
        .iter()
        .any(|l| l.starts_with("(A)") && l.contains("使用価値相当額")));
    assert!(marked
        .iter()
        .any(|l| l.starts_with("(B)") && l.contains("一時募集停止期間")));
    assert!(marked
        .iter()
        .any(|l| l.starts_with("(C)") && l.contains("外国人留学生も含め")));
    for w in a.pages[0]
        .words
        .iter()
        .filter(|w| matches!(w.raw.as_str(), "\u{E94F}" | "\u{E950}" | "\u{E951}"))
    {
        assert_eq!(owners(&a, w.id), 1);
        assert!(matches!(w.text.as_str(), "(A)" | "(B)" | "(C)"));
    }
}
#[test]
fn a_mixed_reference_formula_stays_in_its_existing_account_cell() {
    let pdf=include_bytes!("archive/pages/20260810g00179/20260810g001790112.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let t = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| {
            t.rows == 5
                && t.cols == 2
                && t.cells
                    .iter()
                    .any(|c| c.text.starts_with("未認識数理計算上の差異"))
        })
        .unwrap();
    for (row, label, marker, value) in [
        (1, "退職給付債務", "(A)", "2,666,356,547"),
        (2, "未認識数理計算上の差異", "(B)", "△362,860,311"),
        (3, "未認識過去勤務費用", "(C)", "△309,918,534"),
        (4, "退職給付引当金", "(A)−(B)−(C)", "3,339,135,392"),
    ] {
        let c = t.cells.iter().find(|c| c.row == row && c.col == 0).unwrap();
        assert!(c.text.starts_with(label));
        assert!(c.text.contains(marker), "{}", c.text);
        assert_eq!(
            t.cells
                .iter()
                .find(|c| c.row == row && c.col == 1)
                .unwrap()
                .text,
            value
        );
    }
    let formula = a.pages[0]
        .words
        .iter()
        .find(|w| w.raw == "\u{E94F}−\u{E950}−\u{E951}")
        .unwrap();
    assert_eq!(formula.text, "(A)−(B)−(C)");
    assert_eq!(owners(&a, formula.id), 1);
    assert!(t
        .cells
        .iter()
        .find(|c| c.row == 4 && c.col == 0)
        .unwrap()
        .words
        .contains(&formula.id));
}
#[test]
fn unowned_header_symbols_are_not_attached_to_nearby_road_records() {
    let pdf=include_bytes!("archive/pages/20260821g00187/20260821g001870074.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    assert!(a
        .text
        .contains("次表の(A)、(B)及び(C)に掲げる区間を連続して通行する経路"));
    let headers = a.pages[0]
        .words
        .iter()
        .filter(|w| matches!(w.raw.as_str(), "\u{E94F}" | "\u{E950}" | "\u{E951}"))
        .collect::<Vec<_>>();
    assert_eq!(headers.len(), 3);
    // The existing table has no header-row owner. Restoration must not guess the first data row.
    for w in headers {
        assert!(w.text.is_empty());
        assert_eq!(owners(&a, w.id), 0);
    }
    let t = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.cells.iter().any(|c| c.text == "名古屋環状２号線"))
        .unwrap();
    assert!(t.cells.iter().any(|c| c.text == "イ"));
    assert!(!t
        .cells
        .iter()
        .any(|c| c.text.contains("(A)") || c.text.contains("(B)") || c.text.contains("(C)")));
}
#[test]
fn unknown_reference_d_keeps_the_existing_formula_and_owner_unchanged() {
    let pdf=include_bytes!("archive/pages/20260902g00196/20260902g001960133.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let formula = a.pages[0]
        .words
        .iter()
        .find(|w| w.raw == "\u{E952}＝\u{E94F}＋\u{E950}＋\u{E951}")
        .unwrap();
    // This PDF still has a pre-existing unstructured table and unknown D..G symbols.
    // Do not certify its broken output as correct: assert only that this candidate leaves the
    // partly unknown formula atomic and retains its existing explicit source owner.
    assert!(
        !formula.text.contains("(A)")
            && !formula.text.contains("(B)")
            && !formula.text.contains("(C)")
    );
    assert_eq!(owners(&a, formula.id), 1);
    for w in a.pages[0]
        .words
        .iter()
        .filter(|w| matches!(w.raw.as_str(), "\u{E94F}" | "\u{E950}" | "\u{E951}"))
    {
        assert!(w.text.is_empty());
        assert_eq!(owners(&a, w.id), 0);
    }
}
