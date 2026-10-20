#[test]
fn ordinary_rate_continuation_precedes_only_the_actual_amendment() {
    let pdf = include_bytes!("archive/pages/20260930g00215/20260930g002150416.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    for label in ["改正後", "改正前"] {
        assert_eq!(a.text.lines().filter(|l| *l == label).count(), 1, "{}", a.text);
    }
    assert!(a.text.starts_with("７　1. 2　1. 1　1. 2　1. 1"));
    let notice = a.articles.iter().find(|x| x.text.starts_with("〇国土交通省告示第千百八十二号")).unwrap();
    assert!(notice.text.contains("改正後\n（準特定地域）"));
    assert!(notice.text.contains("改正前\n（準特定地域）"));
    assert!(!notice.text.contains("27　1. 2"));
    let prior = &a.articles[0];
    assert!(!prior.text.lines().any(|l| l == "改正後" || l == "改正前"));
    assert!(prior.text.contains("27　1. 2　1. 1　1. 2　1. 1"));
    assert!(prior.text.contains("令和元　1. 2　1. 1　1. 2　1. 1"));
    let tables: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .collect();
    let upper = tables.iter().find(|t| t.rows == 20 && t.cols == 5).unwrap();
    let lower = tables.iter().find(|t| t.rows == 13 && t.cols == 5).unwrap();
    let row = |t: &kanpo_parser::trace::Table, r| {
        (0..5)
            .map(|c| t.cells.iter().find(|x| x.row == r && x.col == c).unwrap().text.as_str())
            .collect::<Vec<_>>()
            .join("　")
    };
    assert_eq!(row(upper, 0), "７　1. 2　1. 1　1. 2　1. 1");
    assert_eq!(row(upper, 19), "26　1. 2　1. 1　1. 2　1. 1");
    assert_eq!(row(lower, 0), "27　1. 2　1. 1　1. 2　1. 1");
    assert_eq!(row(lower, 4), "令和元　1. 2　1. 1　1. 2　1. 1");
    assert_eq!(row(lower, 11), "８　1. 0　1. 0　1. 0　1. 0");
    let note = lower.cells.iter().find(|c| c.row == 12 && c.col == 0).unwrap();
    assert_eq!(note.colspan, 5);
    assert!(note.text.contains("簡易耐火構造") && note.text.contains("特殊耐火構造"));
    let mut ids = std::collections::HashSet::new();
    for id in a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))))
    {
        assert!(ids.insert(*id));
    }
}

#[test]
fn actual_leading_shinkyu_frame_keeps_both_pairs_and_article_ownership() {
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(include_bytes!("archive/pages/20260810g00179/20260810g001790057.pdf")).unwrap());
    for label in ["改正後", "改正前"] {
        assert_eq!(a.text.lines().filter(|l| *l == label).count(), 2, "{}", a.text);
    }
    let prior = &a.articles[0];
    assert!(prior.text.contains("第一号基礎的電気通信役務収支表"));
    let notice = a.articles.iter().find(|x| x.text.contains("〇国家公安委員会規則第十七号")).unwrap();
    assert!(!notice.text.contains("第一号基礎的電気通信役務収支表"));
    assert!(notice.text.contains("暴力的不法行為"));
}
