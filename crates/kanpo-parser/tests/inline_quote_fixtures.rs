use kanpo_parser::analyze_item;
fn analyze(page: &str) -> kanpo_parser::Analysis {
    let pdf: &[u8] = match page {
        "20260730g001710060" => {
            include_bytes!("archive/pages/20260730g00171/20260730g001710060.pdf")
        }
        "20260803g001740016" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740016.pdf")
        }
        "20260803g001740017" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740017.pdf")
        }
        "20260803g001740018" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740018.pdf")
        }
        "20260803g001740019" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740019.pdf")
        }
        "20260803g001740020" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740020.pdf")
        }
        "20260803g001740021" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740021.pdf")
        }
        "20260803g001740028" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740028.pdf")
        }
        "20260803g001740034" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740034.pdf")
        }
        "20260803g001740046" => {
            include_bytes!("archive/pages/20260803g00174/20260803g001740046.pdf")
        }
        "20260820g001860020" => {
            include_bytes!("archive/pages/20260820g00186/20260820g001860020.pdf")
        }
        "20261006g002200013" => {
            include_bytes!("archive/pages/20261006g00220/20261006g002200013.pdf")
        }
        "20260820g001860019" => {
            include_bytes!("archive/pages/20260820g00186/20260820g001860019.pdf")
        }
        "20260820g001860021" => {
            include_bytes!("archive/pages/20260820g00186/20260820g001860021.pdf")
        }
        "20260831g001940076" => {
            include_bytes!("archive/pages/20260831g00194/20260831g001940076.pdf")
        }
        "20260909g002010004" => {
            include_bytes!("archive/pages/20260909g00201/20260909g002010004.pdf")
        }
        _ => panic!("unknown fixture"),
    };
    analyze_item(&kanpo_parser::page_inputs(pdf).unwrap())
}
fn expected(page: &str) -> &'static str {
    match page {
        "20260730g001710060" => {
            include_str!("archive/expected-inline-quotes/20260730g001710060.txt")
        }
        "20260803g001740016" => {
            include_str!("archive/expected-inline-quotes/20260803g001740016.txt")
        }
        "20260803g001740017" => {
            include_str!("archive/expected-inline-quotes/20260803g001740017.txt")
        }
        "20260803g001740018" => {
            include_str!("archive/expected-inline-quotes/20260803g001740018.txt")
        }
        "20260803g001740019" => {
            include_str!("archive/expected-inline-quotes/20260803g001740019.txt")
        }
        "20260803g001740020" => {
            include_str!("archive/expected-inline-quotes/20260803g001740020.txt")
        }
        "20260803g001740021" => {
            include_str!("archive/expected-inline-quotes/20260803g001740021.txt")
        }
        "20260803g001740028" => {
            include_str!("archive/expected-inline-quotes/20260803g001740028.txt")
        }
        "20260803g001740034" => {
            include_str!("archive/expected-inline-quotes/20260803g001740034.txt")
        }
        "20260803g001740046" => {
            include_str!("archive/expected-inline-quotes/20260803g001740046.txt")
        }
        "20260820g001860020" => {
            include_str!("archive/expected-inline-quotes/20260820g001860020.txt")
        }
        "20261006g002200013" => {
            include_str!("archive/expected-inline-quotes/20261006g002200013.txt")
        }
        "20260820g001860019" => {
            include_str!("archive/expected-inline-quotes/20260820g001860019.txt")
        }
        "20260820g001860021" => {
            include_str!("archive/expected-inline-quotes/20260820g001860021.txt")
        }
        "20260831g001940076" => {
            include_str!("archive/expected-inline-quotes/20260831g001940076.txt")
        }
        "20260909g002010004" => {
            include_str!("archive/expected-inline-quotes/20260909g002010004.txt")
        }
        _ => panic!("unknown fixture"),
    }
}
#[test]
fn private_curly_quotes_survive_in_their_original_inline_words() {
    let a = analyze("20260820g001860020");
    assert!(
        a.text
            .lines()
            .any(|l| l.contains("ⅲ“Due to an absence of competition")),
        "{}",
        a.text
    );
    assert!(
        a.text.lines().any(|l| l.contains("for technical reasons”")),
        "{}",
        a.text
    );
    let words = &a.pages[0].words;
    assert_eq!(words[97].raw, "\u{E20A}Due");
    assert_eq!(words[97].text, "“Due");
    assert_eq!(words[81].raw, "reasons\u{E209}");
    assert_eq!(words[81].text, "reasons”");
    let ids: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| {
            l.words
                .iter()
                .chain(l.table.iter().flat_map(|t| &t.cells).flat_map(|c| &c.words))
        })
        .copied()
        .collect();
    assert_eq!(ids.iter().filter(|&&id| id == 97).count(), 1);
    assert_eq!(ids.iter().filter(|&&id| id == 81).count(), 1);
}
#[test]
fn japanese_and_english_quote_positions_match_all_original_hit_pages() {
    for page in [
        "20260730g001710060",
        "20260803g001740016",
        "20260803g001740017",
        "20260803g001740018",
        "20260803g001740019",
        "20260803g001740020",
        "20260803g001740021",
        "20260803g001740028",
        "20260803g001740034",
        "20260803g001740046",
        "20260820g001860020",
        "20261006g002200013",
    ] {
        assert_eq!(analyze(page).text, expected(page), "{page}")
    }
}
#[test]
fn adjacent_and_previously_repaired_financial_text_stays_exact() {
    for page in [
        "20260820g001860019",
        "20260820g001860021",
        "20260831g001940076",
        "20260909g002010004",
    ] {
        assert_eq!(analyze(page).text, expected(page), "{page}")
    }
}
