use std::collections::HashSet;
#[test]
fn every_dated_record_keeps_its_date_and_rejects_non_date_triples() {
    let pdf = include_bytes!("archive/pages/20260901g00195/20260901g001950018.pdf");
    let pages = kanpo_parser::page_inputs(pdf).unwrap();
    let out = kanpo_parser::analyze_item(&pages);
    for (name, date) in [
        ("しば慎一後援会", "八、四、一〇"), ("社会保障政策懇話会", "八、六、一"),
        ("社会民主主義フォーラム", "七、一〇、一"), ("ＪＡＭ埼玉政治連盟", "八、二、一二"),
        ("ＪＡＭ山陰政治連盟", "八、一、二四"), ("ＪＡＭ山陽政治連盟", "八、一、二五"),
        ("ＪＡＭダイキン工業労働組", "七、一〇、三"),
        ("自由と創造クラブ", "七、八、一〇"), ("秀友会", "八、四、八"), ("淳風会", "八、二、一二"),
        ("少子化対策党まきはらひとし後援会", "八、六、一五"), ("情報公開", "八、三、三一"), ("進藤金日子後援会", "八、三、二六"),
    ] {
        let row = out.text.lines().find(|l| l.starts_with(name)).unwrap();
        assert!(row.ends_with(date), "{name} -> {date}: {row}");
    }
    let mut seen = HashSet::new();
    for id in out.pages[0].regions.iter().flat_map(|r| &r.lines).flat_map(|l| &l.words) { assert!(seen.insert(id)); }
    let mut other = pages;
    other[0].xhtml = other[0].xhtml.replace(['、', '︑'], "/");
    let text = kanpo_parser::analyze_item(&other).text;
    assert!(text.lines().any(|l| l == "八/三/二六"), "日付でない三つ組で整列条件を緩めない: {text}");
}

#[test]
fn dates_do_not_merge_the_existing_individual_changes() {
    // 日付を外せば、修正前のすべての小行が同じ順・同じ区切りで残る。
    let is_date = |text: &str| {
        let parts: Vec<_> = text.split('、').collect();
        parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| "〇零一二三四五六七八九十0123456789０１２３４５６７８９".contains(c)))
    };
    let without_dates = |text: &str| text.lines().filter(|l| !is_date(l)).map(|l| {
        l.rsplit_once('　').filter(|(_, value)| is_date(value)).map_or(l, |(row, _)| row).to_string()
    }).collect::<Vec<_>>();
    for (n, pdf, old) in [
        (18, include_bytes!("archive/pages/20260901g00195/20260901g001950018.pdf").as_slice(), include_str!("fixtures/political_record_subrows_18.txt")),
        (20, include_bytes!("archive/pages/20260901g00195/20260901g001950020.pdf").as_slice(), include_str!("fixtures/political_record_subrows_20.txt")),
    ] {
        let current = kanpo_parser::analyze_item(&kanpo_parser::page_inputs(pdf).unwrap()).text;
        assert_eq!(without_dates(&old), without_dates(&current), "p{n} small rows");
        if n == 20 {
            for (prefix, date) in [
                ("田中昌史全国後援会　斉藤　秀之", "八、四、二五"),
                ("田中昌史全国後援会　田中　昌史", "八、五、八"),
                ("中国地方電力総連政治活動委員会", "七、一〇、一七"), ("忠政会", "八、三、三一"),
                ("つばさの党", "六、五、一"), ("会計責任者の氏名　黒川敦彦　小谷繁", "八、三、二五"),
                ("強い福島を創る会", "八、三、二三"), ("定期健康診断を廃止する党", "七、一二、三一"),
                ("ＴＫＣ中国政経研究会", "八、四、二二"), ("東京公司大麻党連邦捜査局", "八、一、七"),
                ("闘心塾九州本部", "八、三、一〇"), ("呑舟会", "八、三、一"),
            ] { let row = current.lines().find(|l| l.starts_with(prefix)).unwrap(); assert!(row.ends_with(date), "{row}"); }
        }
    }
}
