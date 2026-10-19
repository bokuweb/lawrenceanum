//! fixture は e-Gov 法令 API の実際の XML (現行版) から本則の大半と一部の附則を落としたもの
//! (`tests/fixtures/supplementary/{law_id}.xml`)。附則の中身は原文のまま。

use super::*;
use crate::parse_law_xml;

fn load(law_id: &str) -> SupplementaryDocument {
    let path = format!(
        "{}/tests/fixtures/supplementary/{law_id}.xml",
        env!("CARGO_MANIFEST_DIR")
    );
    let xml = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    analyze(&parse_law_xml(&xml, law_id).unwrap())
}

fn provision<'a>(doc: &'a SupplementaryDocument, amend_law_num: &str) -> &'a ProvisionAnalysis {
    doc.provisions
        .iter()
        .find(|p| p.amend_law_num.as_deref() == Some(amend_law_num))
        .unwrap_or_else(|| panic!("no provision {amend_law_num}"))
}

fn ids(refs: &[Ref]) -> Vec<Option<&str>> {
    refs.iter().map(|r| r.article_id.as_deref()).collect()
}

/// 民法: 政令委任 (期限つき) + ただし書の一部施行、経過措置のタイプ、他法の施行日への連動。
#[test]
fn minpo_delegated_with_proviso_and_transitional_kinds() {
    let doc = load("129AC0000000089");
    assert_eq!(doc.stats.provisions, 3);
    assert_eq!(doc.stats.enforcement_unresolved, 0);

    // 旧様式: 本法施行ノ期日ハ勅令ヲ以テ之ヲ定ム
    let old = provision(&doc, "大正一五年四月二四日法律第六九号");
    assert_eq!(old.promulgation_date.as_deref(), Some("1926-04-24"));
    let e = &old.enforcement[0];
    assert_eq!(
        (e.scope, e.kind),
        (EnforcementScope::Whole, EnforcementKind::Delegated)
    );
    assert_eq!(e.delegated_to.as_deref(), Some("勅令"));

    // 令和六年法律第三十三号 (父母の離婚後の子の養育)
    let p = provision(&doc, "令和六年五月二四日法律第三三号");
    assert_eq!(p.promulgation_date.as_deref(), Some("2024-05-24"));
    let main = main_enforcement(p).unwrap();
    assert_eq!(main.kind, EnforcementKind::Delegated);
    assert_eq!(main.delegated_to.as_deref(), Some("政令"));
    assert_eq!(main.period.as_deref(), Some("公布の日から起算して二年"));
    // 2024-05-24 から起算して二年 → 2026-05-23 に満了
    assert_eq!(main.deadline.as_deref(), Some("2026-05-23"));
    assert_eq!(main.date, None);

    let proviso = &p.enforcement[1];
    assert_eq!(proviso.scope, EnforcementScope::Partial);
    assert_eq!(proviso.kind, EnforcementKind::Promulgation);
    assert_eq!(proviso.date.as_deref(), Some("2024-05-24"));
    assert_eq!(
        proviso.targets.as_deref(),
        Some("附則第十六条から第十八条まで及び第十九条第一項の規定")
    );
    // 範囲 (第十六条から第十八条まで) は展開し、後続の「第十九条」も附則の条として解決する。
    assert_eq!(
        ids(&proviso.target_refs),
        vec![
            Some("suppl2_art_16"),
            Some("suppl2_art_17"),
            Some("suppl2_art_18"),
            Some("suppl2_art_19")
        ]
    );

    let kinds_of = |caption: &str| {
        p.transitional
            .iter()
            .find(|t| t.caption.as_deref() == Some(caption))
            .map(|t| t.kinds.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        kinds_of("（財産分与に関する経過措置）"),
        vec![TransitionalKind::PriorLaw]
    );
    assert_eq!(
        kinds_of("（親権者の変更の請求に関する経過措置）"),
        vec![TransitionalKind::Deemed]
    );
    assert_eq!(
        kinds_of("（子の監護費用に関する経過措置）"),
        vec![TransitionalKind::Application]
    );
    assert_eq!(
        kinds_of("（政令への委任）"),
        vec![TransitionalKind::Delegated]
    );
    assert_eq!(kinds_of("（検討）"), vec![TransitionalKind::Review]);

    // 改正附則の「第一条」は改正法自身の条なので解決しない。「新民法第…」は他の法令の規定。
    let principle = p
        .transitional
        .iter()
        .find(|t| t.caption.as_deref() == Some("（民法の一部改正に伴う経過措置の原則）"))
        .unwrap();
    assert_eq!(principle.refs[0].scope, RefScope::AmendingLaw);
    let custody = p
        .transitional
        .iter()
        .find(|t| t.caption.as_deref() == Some("（子の監護費用に関する経過措置）"))
        .unwrap();
    assert_eq!(custody.refs[0].scope, RefScope::OtherLaw);
    assert_eq!(custody.refs[0].text, "新民法第三百六条第三号");

    // 条に分かれない附則 (Paragraph のみ) も読む。
    let joto = provision(&doc, "令和七年六月六日法律第五七号");
    let main = main_enforcement(joto).unwrap();
    assert_eq!(main.location.article_id, None);
    assert_eq!(main.kind, EnforcementKind::Linked);
    assert_eq!(
        main.linked_to.as_deref(),
        Some("譲渡担保契約及び所有権留保契約に関する法律の施行の日")
    );
    let proviso = &joto.enforcement[1];
    assert_eq!(proviso.targets.as_deref(), Some("第三十三条の規定"));
    assert_eq!(proviso.date.as_deref(), Some("2025-06-06"));
}

/// 制定時の附則: 「第X条」は本則の条へ解決する。なお効力を有する + 読替え規定。
#[test]
fn original_suppl_resolves_main_articles_and_readings() {
    let doc = load("420CO0000000127");
    let p = &doc.provisions[0];
    assert_eq!(p.amend_law_num, None);
    assert_eq!(p.promulgation_date.as_deref(), Some("2008-03-31"));
    let main = main_enforcement(p).unwrap();
    assert_eq!(main.kind, EnforcementKind::FixedDate);
    assert_eq!(main.date.as_deref(), Some("2008-04-01"));
    assert_eq!(main.date_source, Some(DateSource::Explicit));

    let proviso = &p.enforcement[1];
    assert_eq!(proviso.scope, EnforcementScope::Partial);
    assert_eq!(proviso.date.as_deref(), Some("2008-03-31"));
    assert_eq!(
        ids(&proviso.target_refs),
        vec![Some("art_24"), Some("art_27")]
    );
    assert!(proviso
        .target_refs
        .iter()
        .all(|r| r.scope == RefScope::Main));

    let t = &p.transitional[0];
    assert_eq!(t.location.article_id.as_deref(), Some("suppl1_art_2"));
    assert_eq!(t.kinds, vec![TransitionalKind::ContinuedEffect]);
    // 「緑資源公団法（昭和三十一年法律第八十五号）第三十三条」は本則ではなく他の法令。
    let t2 = &p.transitional[1];
    let kodan = t2
        .refs
        .iter()
        .find(|r| r.text.contains("第三十三条"))
        .unwrap();
    assert_eq!(kodan.scope, RefScope::OtherLaw);
    assert!(kodan.text.starts_with("緑資源公団法（"));

    // 「旧機構法施行令第四十三条第一項中「A」とあるのは「B」と、旧機構法施行令第四十四条第二項中…」
    assert_eq!(
        p.readings[0].target.as_deref(),
        Some("旧機構法施行令第四十三条第一項")
    );
    assert_eq!(p.readings[0].from, "機構は、主たる事務所に");
    assert!(p.readings[0]
        .to
        .starts_with("国立研究開発法人森林研究・整備機構は、"));
    assert_eq!(p.readings[0].context.as_deref(), Some("この場合において"));
    assert_eq!(
        p.readings[1].target.as_deref(),
        Some("旧機構法施行令第四十四条第二項")
    );
    assert_eq!(p.readings[1].from, "機構");
    assert_eq!(p.readings[1].to, "国立研究開発法人森林研究・整備機構");
}

/// 旧法令 (片仮名) と罰則の経過措置。
#[test]
fn sabo_katakana_and_penalty() {
    let doc = load("130AC0000000029");
    assert_eq!(doc.stats.enforcement_unresolved, 0);
    let p = provision(&doc, "大正一三年七月一八日法律第三号");
    let e = main_enforcement(p).unwrap();
    assert_eq!(e.text, "本法ハ公布ノ日ヨリ之ヲ施行ス");
    assert_eq!(e.kind, EnforcementKind::Promulgation);
    assert_eq!(e.date.as_deref(), Some("1924-07-18"));

    let penalty = doc
        .provisions
        .iter()
        .flat_map(|p| &p.transitional)
        .find(|t| {
            t.text == "この法律の施行前にした行為に対する罰則の適用については、なお従前の例による。"
        })
        .unwrap();
    assert_eq!(
        penalty.kinds,
        vec![TransitionalKind::PriorLaw, TransitionalKind::Penalty]
    );
}

/// 「次の各号に掲げる規定は、当該各号に定める日から施行する」: 号の列ごとに施行日を読む。
#[test]
fn per_item_enforcement_with_linked_dates() {
    let doc = load("327CO0000000445");
    let p = &doc.provisions[0];
    let main = main_enforcement(p).unwrap();
    assert_eq!(main.kind, EnforcementKind::Linked);
    // 「…の施行の日（令和元年十一月一日）」の括弧書きの日付
    assert_eq!(main.date.as_deref(), Some("2019-11-01"));
    assert_eq!(main.date_source, Some(DateSource::Explicit));

    // 第一号は「略」なので読めず、第二号だけ規則になる。
    assert_eq!(p.enforcement.len(), 2);
    let item = &p.enforcement[1];
    assert_eq!(item.location.item.as_deref(), Some("二"));
    assert_eq!(item.scope, EnforcementScope::Partial);
    assert!(item
        .targets
        .as_deref()
        .unwrap()
        .starts_with("第三条中農地法施行令第三十条第一項の改正規定"));
    assert_eq!(item.kind, EnforcementKind::Linked);
    assert_eq!(item.date.as_deref(), Some("2020-04-01"));
}

/// 号ごとの施行日 (確定日 / 公布日 / 政令委任) と、刑法施行日までの読替え。
#[test]
fn yobo_sesshu_items_and_readings() {
    let doc = load("323AC0000000068");
    let p = &doc.provisions[0];
    assert_eq!(p.promulgation_date.as_deref(), Some("2022-12-09"));
    assert_eq!(
        main_enforcement(p).unwrap().date.as_deref(),
        Some("2024-04-01")
    );
    let delegated = p
        .enforcement
        .iter()
        .find(|e| e.kind == EnforcementKind::Delegated)
        .unwrap();
    // 2022-12-09 から起算して三年六月 → 2026-06-08 に満了
    assert_eq!(delegated.deadline.as_deref(), Some("2026-06-08"));
    assert_eq!(delegated.scope, EnforcementScope::Partial);

    let r = p.readings.iter().find(|r| r.from == "拘禁刑").unwrap();
    assert_eq!(r.to, "懲役");
    assert_eq!(r.target.as_deref(), Some("これらの規定"));
    assert!(r
        .context
        .as_deref()
        .unwrap()
        .contains("刑法施行日の前日までの間における"));
}

/// 公布日から計算する施行日: 「公布の日の属する月の翌月の初日」と「起算して七日を経過した日」。
#[test]
fn computed_dates_from_promulgation() {
    let doc = load("409AC0000000065");
    let p = &doc.provisions[0];
    assert_eq!(p.promulgation_date.as_deref(), Some("2005-11-07"));
    let main = main_enforcement(p).unwrap();
    assert_eq!(main.kind, EnforcementKind::AfterPromulgation);
    assert_eq!(main.date.as_deref(), Some("2005-12-01"));
    assert_eq!(main.date_source, Some(DateSource::Computed));
    assert_eq!(p.enforcement[1].date.as_deref(), Some("2006-04-01"));

    let doc = load("346M50000800038");
    let p = &doc.provisions[0];
    let item = p
        .enforcement
        .iter()
        .find(|e| e.kind == EnforcementKind::AfterPromulgation)
        .unwrap();
    // 1979-04-28 から起算して七日を経過した日
    assert_eq!(item.date.as_deref(), Some("1979-05-05"));
    assert_eq!(item.targets.as_deref(), Some("第十二条及び附則第四項"));
}

#[test]
fn numbers_and_dates() {
    let n = |s: &str| take_number(&s.chars().collect::<Vec<_>>(), 0).map(|(n, _)| n);
    assert_eq!(n("百二十三"), Some(123));
    assert_eq!(n("二千二十四"), Some(2024));
    assert_eq!(n("十"), Some(10));
    assert_eq!(n("一四"), Some(14)); // AmendLawNum の位取り表記
    assert_eq!(n("三〇"), Some(30));
    assert_eq!(n("元"), Some(1));
    assert_eq!(n("１２"), Some(12));
    assert_eq!(
        first_date("令和元年五月一日", None),
        NaiveDate::from_ymd_opt(2019, 5, 1)
    );
    assert_eq!(
        first_date("昭和六三年一二月三〇日法律第一〇九号", None),
        NaiveDate::from_ymd_opt(1988, 12, 30)
    );
    let last = NaiveDate::from_ymd_opt(2018, 4, 1);
    assert_eq!(
        first_date("同年十月一日", last),
        NaiveDate::from_ymd_opt(2018, 10, 1)
    );
    assert_eq!(kansuji(17), "十七");
    assert_eq!(kansuji(119), "百十九");
}

#[test]
fn period_arithmetic_follows_civil_code() {
    let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let p = |s: &str| period_after(s, Some("公布の日から起算して")).unwrap();
    // 起算して六月: 応当日 (8/1) の前日に満了 → 経過した日は応当日。
    assert_eq!(
        p("公布の日から起算して六月").elapsed_from(d(2024, 2, 1)),
        Some(d(2024, 8, 1))
    );
    // 応当日が無い (8/31 + 6 月 → 2/31): 2 月末に満了 → 3/1。
    assert_eq!(
        p("公布の日から起算して六月").elapsed_from(d(2023, 8, 31)),
        Some(d(2024, 3, 1))
    );
    assert_eq!(
        p("公布の日から起算して二十日").elapsed_from(d(2024, 4, 1)),
        Some(d(2024, 4, 21))
    );
    let one_half = p("公布の日から起算して一年六月");
    assert_eq!((one_half.months, one_half.days), (18, 0));
}

#[test]
fn law_num_keys_match_across_formats() {
    assert_eq!(
        law_num_key("令和六年五月二四日法律第三三号").as_deref(),
        Some("令和6|法律|33")
    );
    assert_eq!(
        law_num_key("令和六年法律第三十三号").as_deref(),
        Some("令和6|法律|33")
    );
    assert_eq!(
        law_num_key("平成二十七年三月十八日政令第七十四号"),
        law_num_key("平成二七年三月一八日政令第七四号")
    );
    assert_eq!(
        law_num_key("平成二十年政令第百二十七号").as_deref(),
        Some("平成20|政令|127")
    );
    assert_eq!(law_num_key("改正法"), None);
}

#[test]
fn schema_lists_every_kind() {
    let schema = json_schema().to_string();
    for k in [
        EnforcementKind::FixedDate,
        EnforcementKind::Promulgation,
        EnforcementKind::AfterPromulgation,
        EnforcementKind::Delegated,
        EnforcementKind::Linked,
        EnforcementKind::Unknown,
    ] {
        let name = serde_json::to_value(k).unwrap();
        assert!(schema.contains(&format!("\"{}\"", name.as_str().unwrap())));
    }
}

#[test]
fn timeline_payload_matches_amendment_by_law_num() {
    let doc = load("129AC0000000089");
    // e-Gov の改正履歴は「令和六年法律第三十三号」の表記。
    let v = timeline_enforcement(&doc, Some("令和六年法律第三十三号")).unwrap();
    assert_eq!(v["kind"], "delegated");
    assert_eq!(v["deadline"], "2026-05-23");
    assert_eq!(v["partial"][0]["date"], "2024-05-24");
    assert!(timeline_enforcement(&doc, Some("令和六年法律第九十九号")).is_none());

    // 制定時の附則は制定イベント (改正法令番号なし) に対応する。
    let doc = load("420CO0000000127");
    let v = timeline_enforcement(&doc, None).unwrap();
    assert_eq!(v["date"], "2008-04-01");
}

#[test]
fn promulgation_date_falls_back_to_revision_metadata() {
    let path = format!(
        "{}/tests/fixtures/supplementary/130AC0000000029.xml",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut law = parse_law_xml(&std::fs::read(path).unwrap(), "130AC0000000029").unwrap();
    // 公布日を含まない改正法令番号 (古い改正の一部はこの形) を模す。
    law.suppl_provisions[0].amend_law_num = Some("大正十三年法律第三号".to_string());
    law.suppl_provisions[0].label = None;
    assert_eq!(analyze(&law).provisions[0].promulgation_date, None);
    let dates = HashMap::from([(
        law_num_key("大正十三年法律第三号").unwrap(),
        NaiveDate::from_ymd_opt(1924, 7, 18).unwrap(),
    )]);
    let p = &analyze_with_dates(&law, &dates).provisions[0];
    assert_eq!(p.promulgation_date.as_deref(), Some("1924-07-18"));
    assert_eq!(p.enforcement[0].date.as_deref(), Some("1924-07-18"));
}

#[test]
fn splits_two_clauses_in_one_proviso() {
    let xml = r#"<?xml version="1.0"?>
<Law Era="Heisei" Year="26" PromulgateMonth="11" PromulgateDay="27">
  <LawNum>平成二十六年法律第百二十六号</LawNum>
  <LawBody>
    <LawTitle>テスト法</LawTitle>
    <MainProvision>
      <Article Num="3"><ArticleTitle>第三条</ArticleTitle><Paragraph Num="1"><ParagraphSentence><Sentence>三条。</Sentence></ParagraphSentence></Paragraph></Article>
      <Article Num="4"><ArticleTitle>第四条</ArticleTitle><Paragraph Num="1"><ParagraphSentence><Sentence>四条。</Sentence></ParagraphSentence></Paragraph></Article>
    </MainProvision>
    <SupplProvision>
      <SupplProvisionLabel>附　則</SupplProvisionLabel>
      <Paragraph Num="1"><ParagraphSentence>
        <Sentence Function="main" Num="1">この法律は、公布の日から施行する。</Sentence>
        <Sentence Function="proviso" Num="2">ただし、第三条の規定は公布の日から起算して二十日を経過した日から、第四条の規定は公布の日から起算して一月を経過した日から施行する。</Sentence>
      </ParagraphSentence></Paragraph>
    </SupplProvision>
  </LawBody>
</Law>"#;
    // 私事性的画像記録の提供等による被害の防止に関する法律 (平成二十六年法律第百二十六号) 附則第一条。
    let doc = analyze(&parse_law_xml(xml.as_bytes(), "426AC1000000126").unwrap());
    let e = &doc.provisions[0].enforcement;
    assert_eq!(e.len(), 3);
    assert_eq!(e[0].date.as_deref(), Some("2014-11-27"));
    assert_eq!(e[1].targets.as_deref(), Some("第三条の規定"));
    assert_eq!(e[1].date.as_deref(), Some("2014-12-17"));
    assert_eq!(ids(&e[1].target_refs), vec![Some("art_3")]);
    assert_eq!(e[2].targets.as_deref(), Some("第四条の規定"));
    assert_eq!(e[2].date.as_deref(), Some("2014-12-27"));
    assert!(e[2].text.starts_with("ただし、第三条の規定は"));
}

#[test]
fn provision_subject_with_trailing_parenthetical() {
    let c: Vec<char> = "ただし、第三条中母子保健法第十八条の改正規定（「又は保健所を設置する市」を「、保健所を設置する市又は特別区」に改める部分を除く。）は平成七年一月一日から施行する。".chars().collect();
    let (_, subject) = split_subject(&c);
    let (scope, targets, _) = classify_subject(subject.as_deref());
    assert_eq!(scope, EnforcementScope::Partial);
    assert!(targets
        .unwrap()
        .starts_with("第三条中母子保健法第十八条の改正規定（"));
}
