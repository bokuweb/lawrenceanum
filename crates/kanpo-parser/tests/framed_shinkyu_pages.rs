//! 紙面全高の新旧対照表の後に、通常の上下段で次の記事が続くページ。

#[test]
fn explicit_leading_frame_finishes_before_next_article() {
    let pdf = include_bytes!("archive/pages/20260810g00179/20260810g001790057.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    let analysis = kanpo_parser::analyze_item(&pages);
    let text = &analysis.text;
    // 右の全高枠には上に改正後・下に改正前。左の次の記事の表にも両見出しがある。
    for label in ["改正後", "改正前"] {
        assert_eq!(text.lines().filter(|l| l.trim() == label).count(), 2, "{text}");
    }
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let old_before = compact.find("第一号基礎的電気通信役務収支表").unwrap();
    let note = compact.find("備考表中の〔〕の記載は注記である").unwrap();
    let next = compact.find("〇国家公安委員会規則第十七号").unwrap();
    assert!(old_before < note && note < next, "旧表の改正前・備考の後に次の記事を読む: {text}");
    let next_article = analysis.articles.iter().find(|a| a.text.contains("〇国家公安委員会規則第十七号")).unwrap();
    assert!(next_article.text.contains("暴力的不法行為"));
    assert!(!next_article.text.contains("第一号基礎的電気通信役務収支表"), "次の記事に前の表が混入しない: {}", next_article.text);
}

#[test]
fn trailing_change_frame_stays_with_its_notice_after_the_normal_tiers() {
    let pdf = include_bytes!("archive/pages/20260902g00196/20260902g001960024.pdf");
    let a = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap());
    let compact: String = a.text.chars().filter(|c| !c.is_whitespace()).collect();
    let positions: Vec<_> = ["ることができる。", "〇国土交通省告示第千百二号", "一設置者の氏名及び住所", "二変更した事項", "変更後（小値賀空港）", "変更前（小値賀空港）"]
        .iter().map(|s| compact.find(s).unwrap()).collect();
    assert!(positions.windows(2).all(|p| p[0] < p[1]), "{}", a.text);
    assert!(!a.articles[0].text.contains("変更した事項"));
    let notice = a.articles.iter().find(|x| x.text.contains("〇国土交通省告示第千百二号")).unwrap();
    for text in ["二変更した事項", "変更後", "変更前", "発光ダイオード", "白熱電灯"] {
        assert!(notice.text.contains(text), "{}", notice.text);
    }
}
