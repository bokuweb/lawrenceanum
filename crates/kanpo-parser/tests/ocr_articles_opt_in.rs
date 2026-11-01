//! OCR を明示的に渡したときだけ画像本文を記事にする（opt-in）。OCR の実行は不要で、
//! NDLOCR の trace と同じ形の固定 JSON を使う。
use kanpo_parser::{analyze_item, analyze_item_with_ocr, ocr_articles::reading_order, OcrPage, OcrRegion};

fn page(name: &str) -> Vec<kanpo_parser::PageInput> {
    let path = format!("{}/tests/archive/pages/20260716h01749/{name}.pdf", env!("CARGO_MANIFEST_DIR"));
    kanpo_parser::page_inputs(&std::fs::read(path).unwrap()).unwrap()
}

fn region(text: &str, bbox: [f64; 4]) -> serde_json::Value {
    serde_json::json!({"text": text, "bbox": bbox, "is_vertical": false, "confidence": 0.99})
}

fn trace(regions: Vec<serde_json::Value>) -> String {
    serde_json::json!({"schema_version": 1, "width": 3509, "height": 2480, "stages": {"reading_order": regions}}).to_string()
}

#[test]
fn without_ocr_the_result_is_byte_identical_to_the_default() {
    let pages = page("20260716h017490002");
    let none = vec![None; pages.len()];
    assert_eq!(
        serde_json::to_string(&analyze_item_with_ocr(&pages, &none)).unwrap(),
        serde_json::to_string(&analyze_item(&pages)).unwrap()
    );
}

#[test]
fn ndlocr_trace_regions_keep_text_box_and_orientation() {
    let p = OcrPage::from_ndlocr_trace(&trace(vec![region("住所東京都港区", [10.0, 20.0, 300.0, 60.0])])).unwrap();
    assert_eq!(p.regions, vec![OcrRegion { text: "住所東京都港区".into(), bbox: [10.0, 20.0, 300.0, 60.0], vertical: false }]);
    assert!(OcrPage::from_ndlocr_trace("{}").is_err());
}

#[test]
fn reading_order_reads_columns_left_to_right_below_a_spanning_title() {
    let r = |t: &str, b: [f64; 4]| OcrRegion { text: t.into(), bbox: b, vertical: false };
    // 2 段の行が同じ高さに並び、上に両段を横切る標題がある。行の y が揃っても段を混ぜない。
    let regions = vec![r("右1", [600.0, 100.0, 1000.0, 140.0]), r("左1", [0.0, 100.0, 400.0, 140.0]), r("標題", [0.0, 0.0, 1000.0, 50.0]), r("左2", [0.0, 160.0, 400.0, 200.0]), r("右2", [600.0, 160.0, 1000.0, 200.0])];
    let order: Vec<&str> = reading_order(&regions).iter().map(|r| r.text.as_str()).collect();
    assert_eq!(order, ["標題", "左1", "左2", "右1", "右2"]);
    // 縦組みの領域だけなら右の行から読む。
    let v = |t: &str, x: f64| OcrRegion { text: t.into(), bbox: [x, 0.0, x + 30.0, 500.0], vertical: true };
    let vs = vec![v("二", 0.0), v("一", 100.0)];
    assert_eq!(reading_order(&vs).iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["一", "二"]);
}

#[test]
fn image_text_becomes_a_pickable_article_and_native_duplicates_are_dropped() {
    let pages = page("20260716h017490002");
    let base = analyze_item(&pages);
    // ネイティブの語の枠（ページの pt）を、3509×2480 の認識画像の画素へ写した位置に、二重に読んだ領域を置く。
    let xhtml = &pages[0].xhtml;
    let num = |tag: &str, name: &str| -> f64 { tag[tag.find(&format!(" {name}=\"")).unwrap() + name.len() + 3..].split('"').next().unwrap().parse().unwrap() };
    let page_tag = &xhtml[xhtml.find("<page ").unwrap()..];
    let scale = (3509.0 + 2480.0) / (num(page_tag, "width") + num(page_tag, "height"));
    let word = &xhtml[xhtml.find("<word ").unwrap()..];
    let native_box = ["xMin", "yMin", "xMax", "yMax"].map(|n| num(word, n) * scale);
    let native_line = "ネイティブの字を二重に読んだ領域";
    let title = "日本国に帰化を許可する件（法務省告示配一〇七）";
    let ocr = OcrPage::from_ndlocr_trace(&trace(vec![
        region(native_line, native_box),
        region("〇法務省告示配第百七号", [0.0, 3000.0, 800.0, 3040.0]),
        region("左記の者の申請に係る日本国に帰化の件は、これを許可する。", [0.0, 3050.0, 800.0, 3090.0]),
        region("住所東京都港区", [0.0, 3100.0, 800.0, 3140.0]),
    ]))
    .unwrap();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(ocr);
    let with = analyze_item_with_ocr(&pages, &ocrs);
    assert_eq!(serde_json::to_value(&with.articles[..base.articles.len()]).unwrap(), serde_json::to_value(&base.articles).unwrap(), "native articles are kept as they are");
    let added: Vec<&str> = with.articles[base.articles.len()..].iter().map(|a| a.text.as_str()).collect();
    assert!(added.iter().all(|t| !t.contains(native_line)), "OCR of native text is not added twice: {added:?}");
    let i = kanpo_parser::pick_article(title, &with.articles).expect("the OCR article is picked");
    assert!(with.articles[i].text.starts_with("〇法務省告示配第百七号"), "{:?}", with.articles.iter().map(|a| a.text.chars().take(40).collect::<String>()).collect::<Vec<_>>());
    assert_eq!(kanpo_parser::pick_article(title, &base.articles), None, "default parsing does not see the image");
}

#[test]
fn numbered_headings_without_a_recognized_circle_start_articles() {
    let pages = page("20260716h017490002");
    // 画像の見出しの丸印は認識されないか、別の領域「○」になる。
    let ocr = OcrPage::from_ndlocr_trace(&trace(vec![
        region("法務省告示配第百十五号", [0.0, 0.0, 800.0, 40.0]),
        region("北海道赤平市役所保存の次の除籍が滅失した。", [0.0, 50.0, 800.0, 90.0]),
        region("○", [0.0, 100.0, 40.0, 140.0]),
        region("法務省告示配第百十六号", [50.0, 100.0, 800.0, 140.0]),
        region("次の者を承認した。", [0.0, 150.0, 800.0, 190.0]),
    ]))
    .unwrap();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(ocr);
    let base = analyze_item(&pages).articles.len();
    let added: Vec<String> = analyze_item_with_ocr(&pages, &ocrs).articles[base..].iter().map(|a| a.text.clone()).collect();
    assert_eq!(added, ["法務省告示配第百十五号\n北海道赤平市役所保存の次の除籍が滅失した。", "法務省告示配第百十六号\n次の者を承認した。"]);
}

#[test]
fn native_column_rules_that_no_region_crosses_separate_columns() {
    use kanpo_parser::ocr_articles::reading_order_with_rules;
    let r = |t: &str, b: [f64; 4]| OcrRegion { text: t.into(), bbox: b, vertical: false };
    let ruled = vec![r("左1", [0.0, 0.0, 495.0, 40.0]), r("右1", [505.0, 0.0, 1000.0, 40.0]), r("左2", [0.0, 50.0, 495.0, 90.0]), r("右2", [505.0, 50.0, 1000.0, 90.0]), r("右3", [505.0, 100.0, 1000.0, 140.0])];
    let order: Vec<&str> = reading_order_with_rules(&ruled, &[[500.0, 0.0, 500.0, 200.0]]).iter().map(|r| r.text.as_str()).collect();
    assert_eq!(order, ["左1", "左2", "右1", "右2", "右3"]);
    // 罫を横切る領域があれば、その罫では分けない。
    let crossing = vec![r("標題", [0.0, 0.0, 1000.0, 40.0]), r("左", [0.0, 50.0, 495.0, 90.0]), r("右", [505.0, 50.0, 1000.0, 90.0])];
    let order: Vec<&str> = reading_order_with_rules(&crossing, &[[500.0, 0.0, 500.0, 200.0]]).iter().map(|r| r.text.as_str()).collect();
    assert_eq!(order, ["標題", "左", "右"]);
}

#[test]
fn vertical_pages_read_tiers_top_down_and_lines_right_to_left() {
    let v = |t: &str, x: f64, y: f64| OcrRegion { text: t.into(), bbox: [x, y, x + 30.0, y + 400.0], vertical: true };
    // 2 段の縦組み。各段の行の x は段をまたいで揃う。
    let regions = vec![v("下二", 0.0, 500.0), v("上二", 0.0, 0.0), v("下一", 100.0, 500.0), v("上一", 100.0, 0.0)];
    let order: Vec<&str> = reading_order(&regions).iter().map(|r| r.text.as_str()).collect();
    assert_eq!(order, ["上一", "上二", "下一", "下二"]);
}

#[test]
fn abbreviated_agencies_in_titles_pick_the_heading_with_the_same_number() {
    let pages = page("20260716h017490002");
    let body = "次の特定危険指定暴力団等につき、公示事項の一部に変更があったので、次のとおり告示する。";
    let ocr = OcrPage::from_ndlocr_trace(&trace(vec![
        region("山口県公安委員会告示配第一号", [0.0, 0.0, 800.0, 40.0]),
        region(body, [0.0, 50.0, 800.0, 90.0]),
        region("福岡県公安委員会告示配第四号", [0.0, 100.0, 800.0, 140.0]),
        region(body, [0.0, 150.0, 800.0, 190.0]),
        region("福岡県公安委員会委員長", [0.0, 200.0, 800.0, 240.0]),
    ]))
    .unwrap();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(ocr);
    let a = analyze_item_with_ocr(&pages, &ocrs).articles;
    let title = |t: &str| format!("特定危険指定暴力団等に係る公示事項の一部に変更があったことの告示（{t}）");
    let head = |i: Option<usize>| a[i.unwrap()].text.lines().next().unwrap().to_string();
    assert_eq!(head(kanpo_parser::pick_article(&title("山口県公安委告示配一"), &a)), "山口県公安委員会告示配第一号");
    assert_eq!(head(kanpo_parser::pick_article(&title("福岡県公安委告示配四"), &a)), "福岡県公安委員会告示配第四号");
}

#[test]
fn repeated_listed_headings_in_one_ocr_article_split_at_the_first() {
    let pages = page("20260716h017490002");
    let lines = ["令和8年(再イ)第9号", "再生債務者 株式会社甲", "教育職員免許状失効公告", "次の免許状は失効した", "令和8年8月4日 甲県教育委員会", "教育職員免許状失効公告", "次の免許状は失効した", "行旅死亡人", "本籍・住所・氏名不詳の男性"];
    let regions = lines.iter().enumerate().map(|(k, l)| region(l, [0.0, 3000.0 + 50.0 * k as f64, 800.0, 3040.0 + 50.0 * k as f64])).collect();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(OcrPage::from_ndlocr_trace(&trace(regions)).unwrap());
    let mut a = analyze_item_with_ocr(&pages, &ocrs).articles;
    let titles = ["教育職員免許状失効、行旅死亡人関係"];
    kanpo_parser::split_listed_sections(&titles, &mut a);
    let picked = &kanpo_parser::pick_items(&titles, &a)[0];
    assert!(a[picked[0]].text.starts_with("教育職員免許状失効公告"), "{:?}", picked.iter().map(|&i| &a[i].text).collect::<Vec<_>>());
}

#[test]
fn ocr_only_picking_tolerates_one_misread_char_and_short_headings_ending_with_the_word() {
    let pages = page("20260716h017490002");
    let lines = ["令和8年(再イ)第9号", "再生債務者 株式会社甲", "無縁填墓等改葬公告", "無縁墳墓について改葬することとなりました。", "工場財団", "工場財団に属する土地は次のとおり。"];
    let regions = lines.iter().enumerate().map(|(k, l)| region(l, [0.0, 3000.0 + 50.0 * k as f64, 800.0, 3040.0 + 50.0 * k as f64])).collect();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(OcrPage::from_ndlocr_trace(&trace(regions)).unwrap());
    let mut a = analyze_item_with_ocr(&pages, &ocrs).articles;
    let titles = ["無縁墳墓等改葬関係", "財団関係"];
    assert!(kanpo_parser::pick_items(&titles, &a).iter().all(Vec::is_empty), "the default picking needs the exact word");
    let picks = kanpo_parser::ocr_articles::pick_items_with_ocr(&titles, &mut a);
    let head = |k: usize| a[picks[k][0]].text.lines().next().unwrap().to_string();
    assert_eq!(head(0), "無縁填墓等改葬公告");
    assert_eq!(head(1), "工場財団");
}

#[test]
fn ocr_only_picking_tolerates_a_misread_kind_and_a_lost_heading_number() {
    let pages = page("20260716h017490002");
    let lines = [
        "法務省告示配第百十号",
        "次の者に対し、特定外国法を指定した。",
        "法務省告示記第百九号",
        "次の者に対し、承認をした。",
        "愛知県公安委員会告示配第",
        "次の盗品売買等防止団体について、代表者の変更の届出があった。",
    ];
    let regions = lines.iter().enumerate().map(|(k, l)| region(l, [0.0, 3000.0 + 50.0 * k as f64, 800.0, 3040.0 + 50.0 * k as f64])).collect();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(OcrPage::from_ndlocr_trace(&trace(regions)).unwrap());
    let mut a = analyze_item_with_ocr(&pages, &ocrs).articles;
    let titles = ["外国弁護士による法律事務の取扱い等に関する法律第九条の規定による承認をした件（法務省告示配一〇九）", "盗品売買等防止団体の公示事項の一部に係る変更の届出があった件（愛知県公安委告示配一）"];
    let picks = kanpo_parser::ocr_articles::pick_items_with_ocr(&titles, &mut a);
    let head = |k: usize| a[picks[k][0]].text.lines().next().unwrap().to_string();
    assert_eq!(head(0), "法務省告示記第百九号");
    assert_eq!(head(1), "愛知県公安委員会告示配第");
}

#[test]
fn ocr_only_picking_repicks_at_the_one_line_starting_with_the_first_listed_word() {
    let pages = page("20260716h017490002");
    let lines = ["解散公告", "当社は解散したので、債権者は申し出てください。", "旅行業者営業保証金の権利実行申立てに係る公告", "次のように公告する。"];
    let regions = lines.iter().enumerate().map(|(k, l)| region(l, [0.0, 3000.0 + 50.0 * k as f64, 800.0, 3040.0 + 50.0 * k as f64])).collect();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(OcrPage::from_ndlocr_trace(&trace(regions)).unwrap());
    let mut a = analyze_item_with_ocr(&pages, &ocrs).articles;
    let titles = ["旅行業者営業保証金の権利実行申立て関係"];
    let picks = kanpo_parser::ocr_articles::pick_items_with_ocr(&titles, &mut a);
    assert!(a[picks[0][0]].text.starts_with("旅行業者営業保証金の権利実行申立てに係る公告"), "{:?}", picks[0].iter().map(|&i| &a[i].text).collect::<Vec<_>>());
}

#[test]
fn ocr_only_repicking_accepts_a_heading_without_the_leading_agency_name() {
    let pages = page("20260716h017490002");
    let lines = ["懲戒処分の公告", "次の懲戒処分をしたので公告する。", "裁決取消訴訟の判決確定の公告", "次の判決が確定したので公告する。"];
    let regions = lines.iter().enumerate().map(|(k, l)| region(l, [0.0, 3000.0 + 50.0 * k as f64, 800.0, 3040.0 + 50.0 * k as f64])).collect();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(OcrPage::from_ndlocr_trace(&trace(regions)).unwrap());
    let mut a = analyze_item_with_ocr(&pages, &ocrs).articles;
    let titles = ["日本弁護士連合会裁決取消訴訟の判決確定・懲戒処分・公示送達関係"];
    let picks = kanpo_parser::ocr_articles::pick_items_with_ocr(&titles, &mut a);
    assert!(a[picks[0][0]].text.starts_with("裁決取消訴訟の判決確定の公告"), "{:?}", picks[0].iter().map(|&i| &a[i].text).collect::<Vec<_>>());
}

#[test]
fn ocr_only_picking_takes_the_first_of_several_listed_subheadings() {
    let pages = page("20260716h017490002");
    let lines = ["破産手続開始及び免責許可申立てに関する意見申述期間", "次の破産事件について、破産手続を開始した。", "破産手続廃止", "次の破産事件について、破産手続を廃止した。"];
    let regions = lines.iter().enumerate().map(|(k, l)| region(l, [0.0, 3000.0 + 50.0 * k as f64, 800.0, 3040.0 + 50.0 * k as f64])).collect();
    let mut ocrs = vec![None; pages.len()];
    ocrs[0] = Some(OcrPage::from_ndlocr_trace(&trace(regions)).unwrap());
    let mut a = analyze_item_with_ocr(&pages, &ocrs).articles;
    let titles = ["破産、免責、再生関係"];
    let picks = kanpo_parser::ocr_articles::pick_items_with_ocr(&titles, &mut a);
    assert!(a[picks[0][0]].text.starts_with("破産手続開始及び"), "{:?}", picks[0].iter().map(|&i| &a[i].text).collect::<Vec<_>>());
}
