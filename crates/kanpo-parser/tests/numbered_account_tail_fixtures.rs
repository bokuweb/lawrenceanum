use kanpo_parser::analyze_item;
fn analyze(page: &str) -> kanpo_parser::Analysis {
    let pdf: &[u8] = match page {
        "20260831g001940071" => {
            include_bytes!("archive/pages/20260831g00194/20260831g001940071.pdf")
        }
        "20260831g001940076" => {
            include_bytes!("archive/pages/20260831g00194/20260831g001940076.pdf")
        }
        "20260925g002120051" => {
            include_bytes!("archive/pages/20260925g00212/20260925g002120051.pdf")
        }
        "20260831g001940070" => {
            include_bytes!("archive/pages/20260831g00194/20260831g001940070.pdf")
        }
        "20260831g001940072" => {
            include_bytes!("archive/pages/20260831g00194/20260831g001940072.pdf")
        }
        "20260831g001940079" => {
            include_bytes!("archive/pages/20260831g00194/20260831g001940079.pdf")
        }
        "20260831g001940080" => {
            include_bytes!("archive/pages/20260831g00194/20260831g001940080.pdf")
        }
        "20260831g001940082" => {
            include_bytes!("archive/pages/20260831g00194/20260831g001940082.pdf")
        }
        _ => panic!("unknown fixture"),
    };
    let a = analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let expected = match page {
        "20260831g001940071" => Some(include_str!("archive/expected-numbered-account-tail/20260831g001940071.txt")),
        "20260831g001940079" => Some(include_str!("archive/expected-numbered-account-tail/20260831g001940079.txt")),
        "20260831g001940080" => Some(include_str!("archive/expected-numbered-account-tail/20260831g001940080.txt")),
        "20260831g001940082" => Some(include_str!("archive/expected-numbered-account-tail/20260831g001940082.txt")),
        _ => None,
    };
    if let Some(expected) = expected { assert_eq!(a.text, expected, "full page {page}"); }
    // Literal original word IDs stay together in their native physical label cell.
    let bindings: &[(usize, usize, &[usize])] = match page {
        "20260831g001940071" => &[(7, 0, &[285, 270, 271])],
        "20260831g001940079" => &[(15, 0, &[271, 249, 250])],
        "20260831g001940080" => &[(4, 3, &[130, 115, 116]), (5, 0, &[289, 265, 266])],
        "20260831g001940082" => &[(0, 0, &[255, 234, 235])],
        _ => &[],
    };
    for &(row, col, ids) in bindings {
        let cells: Vec<_> = a.pages[0].regions.iter().flat_map(|r| &r.lines)
            .filter_map(|l| l.table.as_ref()).flat_map(|t| &t.cells)
            .filter(|c| c.words.contains(&ids[0])).collect();
        assert_eq!(cells.len(), 1, "unique original owner {page}");
        assert_eq!((cells[0].row, cells[0].col), (row, col));
        assert_eq!(cells[0].words, ids);
        for id in ids {
            let occurrences: usize = a.pages[0].regions.iter().flat_map(|r| &r.lines)
                .map(|l| l.words.iter().filter(|w| *w == id).count()
                    + l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words).filter(|w| *w == id).count()).sum();
            assert_eq!(occurrences, 1, "source word {page}/{id}");
        }
    }
    a
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn numbered_long_account_and_its_amount_share_the_original_cell_row() {
    let a = analyze("20260831g001940071");
    let t = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .find(|t| t.cols == 6 && t.cells.iter().any(|c| c.words.contains(&270)))
        .unwrap();
    let label = t.cells.iter().find(|c| c.words.contains(&270)).unwrap();
    assert_eq!(compact(&label.text), "３未収前期高齢者関係事務費拠出金");
    assert_eq!(label.words, vec![285, 270, 271]);
    assert!(t
        .cells
        .iter()
        .any(|c| c.row == label.row && c.col == 2 && c.text == "24,637"));
    assert!(a
        .text
        .lines()
        .any(|l| compact(l).contains("３未収前期高齢者関係事務費拠出金24,637")));
}
#[test]
fn already_repaired_and_neighboring_balance_sheets_are_unchanged() {
    for page in [
        "20260831g001940076",
        "20260925g002120051",
        "20260831g001940070",
        "20260831g001940072",
    ] {
        assert_eq!(
            analyze(page).text,
            match page {
                "20260831g001940076" =>
                    include_str!("archive/expected-numbered-account-tail/20260831g001940076.txt"),
                "20260925g002120051" =>
                    include_str!("archive/expected-numbered-account-tail/20260925g002120051.txt"),
                "20260831g001940070" =>
                    include_str!("archive/expected-numbered-account-tail/20260831g001940070.txt"),
                "20260831g001940072" =>
                    include_str!("archive/expected-numbered-account-tail/20260831g001940072.txt"),
                _ => unreachable!(),
            },
            "{page}"
        )
    }
}

#[test]
fn both_sides_of_special_grant_accounts_keep_their_distinct_amounts() {
    let a = analyze("20260831g001940080");
    for term in [
        "１未払特別事業助成費返還金3,242",
        "２未収特別事業助成費返還金3,130",
    ] {
        assert!(
            a.text.lines().any(|l| compact(l).contains(term)),
            "{term}: {}",
            a.text
        )
    }
}
#[test]
fn numbered_reserve_account_tails_keep_their_own_fund_amounts() {
    for (page, term) in [
        (
            "20260831g001940079",
            "４システム機器更新等経費積立資産16,697",
        ),
        (
            "20260831g001940082",
            "３システム機器更新等経費積立資産11,000",
        ),
    ] {
        let a = analyze(page);
        assert!(
            a.text.lines().any(|l| compact(l).contains(term)),
            "{page}: {}",
            a.text
        )
    }
}
